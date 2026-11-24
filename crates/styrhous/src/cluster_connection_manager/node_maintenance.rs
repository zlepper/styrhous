//! Node maintenance — cordon, uncordon and drain — the way `kubectl` does it.
//!
//! A drain cordons the Node, then evicts its Pods through the Eviction API so
//! that PodDisruptionBudgets are honoured: an eviction the budget refuses
//! (HTTP 429) is retried until a deadline and then reported, never bypassed
//! with a plain delete. Pod selection follows `kubectl drain
//! --ignore-daemonsets` without `--force` or `--delete-emptydir-data`:
//! DaemonSet-managed and mirror Pods stay on the Node, and a running Pod that
//! declares no controller or uses `emptyDir` storage would be lost, so it
//! stops the drain before anything is evicted. Unlike `kubectl`, a drain is
//! complete once every eviction is accepted; it does not wait for the evicted
//! Pods to finish terminating.

use anyhow::{Context, Result};
use futures_util::stream::{self, StreamExt};
use k8s_openapi::api::core::v1::{Node, Pod};
use k8s_openapi::serde_json::{Value, json};
use kube::Api;
use kube::api::{DeleteParams, EvictParams, ListParams, Patch, PatchParams, Preconditions};
use std::time::Duration;
use tokio::time::Instant;
use tracing::info;

/// The annotation the kubelet puts on the API mirror of a static Pod.
const MIRROR_POD_ANNOTATION: &str = "kubernetes.io/config.mirror";
/// The status cause of a request refused because the namespace is terminating.
const NAMESPACE_TERMINATING_CAUSE: &str = "NamespaceTerminating";

/// Cordons (`true`) or uncordons (`false`) `node_name`.
pub(crate) async fn set_node_unschedulable(
    client: kube::Client,
    node_name: &str,
    unschedulable: bool,
) -> Result<()> {
    info!(
        "{} Node {node_name}",
        if unschedulable {
            "Cordoning"
        } else {
            "Uncordoning"
        }
    );
    // `kubectl uncordon` removes the field rather than setting it to false.
    let value = if unschedulable {
        Value::Bool(true)
    } else {
        Value::Null
    };
    Api::<Node>::all(client)
        .patch(
            node_name,
            &PatchParams::default(),
            &Patch::Merge(json!({ "spec": { "unschedulable": value } })),
        )
        .await
        .with_context(|| format!("Updating spec.unschedulable of Node {node_name}"))?;
    Ok(())
}

/// One Pod of a drained Node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DrainPod {
    pub(crate) namespace: String,
    pub(crate) name: String,
    /// Evictions are made with this UID as a precondition, so a replacement
    /// Pod of the same name (a StatefulSet's) is never evicted by mistake.
    pub(crate) uid: Option<String>,
}

impl DrainPod {
    fn of(pod: &Pod) -> Self {
        Self {
            namespace: pod.metadata.namespace.clone().unwrap_or_default(),
            name: pod.metadata.name.clone().unwrap_or_default(),
            uid: pod.metadata.uid.clone(),
        }
    }

    /// `namespace/name`, as `kubectl` names Pods in its drain messages.
    pub(crate) fn display_name(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}

/// Why a Pod stops a drain before anything is evicted: evicting it would
/// lose it or its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrainBlocker {
    /// No controller would recreate it (`kubectl drain --force` overrides).
    NoController,
    /// Its `emptyDir` data would be deleted (`--delete-emptydir-data`).
    LocalStorage,
}

impl DrainBlocker {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::NoController => "declares no controller, so nothing would recreate it",
            Self::LocalStorage => "uses emptyDir storage, whose data would be deleted",
        }
    }
}

/// What a drain does with each Pod on the Node.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct DrainPlan {
    pub(crate) evict: Vec<DrainPod>,
    pub(crate) daemon_set_pods: Vec<DrainPod>,
    pub(crate) mirror_pods: Vec<DrainPod>,
    pub(crate) blocked: Vec<(DrainPod, DrainBlocker)>,
}

/// Sorts `pods` like `kubectl drain --ignore-daemonsets`. A finished Pod is
/// always removed, whatever its controller or storage.
pub(crate) fn plan_drain<'a>(pods: impl IntoIterator<Item = &'a Pod>) -> DrainPlan {
    let mut plan = DrainPlan::default();
    for pod in pods {
        let finished = is_finished(pod);
        let controller_kind = pod
            .metadata
            .owner_references
            .iter()
            .flatten()
            .find(|owner| owner.controller == Some(true))
            .map(|owner| owner.kind.as_str());
        let target = DrainPod::of(pod);
        if controller_kind == Some("DaemonSet") && !finished {
            plan.daemon_set_pods.push(target);
        } else if pod
            .metadata
            .annotations
            .as_ref()
            .is_some_and(|annotations| annotations.contains_key(MIRROR_POD_ANNOTATION))
        {
            plan.mirror_pods.push(target);
        } else if !finished && uses_local_storage(pod) {
            plan.blocked.push((target, DrainBlocker::LocalStorage));
        } else if !finished && controller_kind.is_none() {
            plan.blocked.push((target, DrainBlocker::NoController));
        } else {
            plan.evict.push(target);
        }
    }
    plan
}

/// Whether `pod` has succeeded or failed, so it no longer occupies its Node.
pub(crate) fn is_finished(pod: &Pod) -> bool {
    matches!(
        pod.status
            .as_ref()
            .and_then(|status| status.phase.as_deref()),
        Some("Succeeded" | "Failed")
    )
}

fn uses_local_storage(pod: &Pod) -> bool {
    pod.spec
        .as_ref()
        .and_then(|spec| spec.volumes.as_ref())
        .is_some_and(|volumes| volumes.iter().any(|volume| volume.empty_dir.is_some()))
}

/// How long and how hard a drain asks for evictions PodDisruptionBudgets refuse.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DrainOptions {
    /// For each Pod, from its first eviction request.
    pub(crate) eviction_timeout: Duration,
    /// Between two requests for the same Pod (`kubectl` waits 5 s).
    pub(crate) retry_interval: Duration,
    /// Evictions requested at once.
    pub(crate) concurrency: usize,
}

impl Default for DrainOptions {
    fn default() -> Self {
        Self {
            eviction_timeout: Duration::from_secs(120),
            retry_interval: Duration::from_secs(5),
            concurrency: 8,
        }
    }
}

/// The outcome of a drain whose Node was cordoned and whose Pods were listed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NodeDrainReport {
    /// Evictions accepted, or Pods that were already gone.
    pub(crate) evicted: Vec<DrainPod>,
    pub(crate) daemon_set_pods: usize,
    pub(crate) mirror_pods: usize,
    /// Pods that stopped the drain before anything was evicted.
    pub(crate) blocked: Vec<(DrainPod, DrainBlocker)>,
    /// Evictions that failed or that a PodDisruptionBudget kept refusing.
    pub(crate) failed: Vec<(DrainPod, String)>,
}

impl NodeDrainReport {
    pub(crate) fn is_complete(&self) -> bool {
        self.blocked.is_empty() && self.failed.is_empty()
    }
}

/// Cordons `node_name` and evicts its Pods; see the module documentation.
/// The Node stays cordoned whatever the outcome, as with `kubectl drain`.
pub(crate) async fn drain_node(
    client: kube::Client,
    node_name: &str,
    options: DrainOptions,
) -> Result<NodeDrainReport> {
    set_node_unschedulable(client.clone(), node_name, true).await?;
    let pods = Api::<Pod>::all(client.clone())
        .list(&ListParams::default().fields(&format!("spec.nodeName={node_name}")))
        .await
        .with_context(|| format!("Listing the Pods on Node {node_name}"))?;
    let plan = plan_drain(&pods.items);
    let mut report = NodeDrainReport {
        daemon_set_pods: plan.daemon_set_pods.len(),
        mirror_pods: plan.mirror_pods.len(),
        blocked: plan.blocked,
        ..Default::default()
    };
    if !report.blocked.is_empty() {
        return Ok(report);
    }
    info!(
        "Draining Node {node_name}: evicting {} Pods",
        plan.evict.len()
    );
    let outcomes = stream::iter(plan.evict)
        .map(|pod| {
            let client = client.clone();
            async move {
                // Each Pod gets the whole timeout from its own first request, however long it queued.
                let deadline = Instant::now() + options.eviction_timeout;
                let outcome = evict_pod(client, &pod, deadline, options).await;
                (pod, outcome)
            }
        })
        .buffer_unordered(options.concurrency.max(1))
        .collect::<Vec<_>>()
        .await;
    for (pod, outcome) in outcomes {
        match outcome {
            Ok(()) => report.evicted.push(pod),
            Err(error) => report.failed.push((pod, error)),
        }
    }
    report.evicted.sort();
    report.failed.sort();
    Ok(report)
}

/// Evicts `pod`, retrying until `deadline` while a PodDisruptionBudget refuses.
/// `Ok` means the eviction was accepted or the Pod is already gone.
async fn evict_pod(
    client: kube::Client,
    pod: &DrainPod,
    deadline: Instant,
    options: DrainOptions,
) -> std::result::Result<(), String> {
    let pods: Api<Pod> = Api::namespaced(client, &pod.namespace);
    let delete_options = DeleteParams::default().preconditions(Preconditions {
        uid: pod.uid.clone(),
        resource_version: None,
    });
    let params = EvictParams {
        delete_options: Some(delete_options.clone()),
        ..Default::default()
    };
    loop {
        let status = match pods.evict(&pod.name, &params).await {
            Ok(_) => return Ok(()),
            Err(kube::Error::Api(status)) => status,
            Err(error) => return Err(error.to_string()),
        };
        // A failed UID precondition means the Pod was replaced, so ours is gone.
        let replaced = status.code == 409 && pod_was_replaced(&pods, pod).await;
        match status.code {
            404 => return Ok(()),
            409 if replaced => return Ok(()),
            // A budget refused the eviction, or another one changed it meanwhile.
            409 | 429 if Instant::now() + options.retry_interval < deadline => {
                tokio::time::sleep(options.retry_interval).await;
            }
            429 => {
                return Err(format!(
                    "Still refused after {}s: {}",
                    options.eviction_timeout.as_secs(),
                    status.message
                ));
            }
            // A terminating namespace accepts no evictions; `kubectl` deletes instead.
            403 if status.details.as_ref().is_some_and(|details| {
                details
                    .causes
                    .iter()
                    .any(|cause| cause.reason == NAMESPACE_TERMINATING_CAUSE)
            }) =>
            {
                return match pods.delete(&pod.name, &delete_options).await {
                    Ok(_) => Ok(()),
                    Err(kube::Error::Api(status)) if matches!(status.code, 404 | 409) => Ok(()),
                    Err(kube::Error::Api(status)) => Err(status.message),
                    Err(error) => Err(error.to_string()),
                };
            }
            _ => return Err(status.message),
        }
    }
}

async fn pod_was_replaced(pods: &Api<Pod>, pod: &DrainPod) -> bool {
    match pods.get_opt(&pod.name).await {
        Ok(Some(current)) => current.metadata.uid != pod.uid,
        Ok(None) => true,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_kube_api::{FakeKubeApi, Reply};
    use k8s_openapi::api::core::v1::{EmptyDirVolumeSource, PodSpec, PodStatus, Volume};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
    use std::collections::BTreeMap;

    fn pod(name: &str, controller: Option<&str>) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_owned()),
                namespace: Some("apps".to_owned()),
                uid: Some(format!("{name}-uid")),
                owner_references: controller.map(|kind| {
                    vec![OwnerReference {
                        api_version: "apps/v1".to_owned(),
                        kind: kind.to_owned(),
                        name: format!("{name}-owner"),
                        uid: format!("{name}-owner-uid"),
                        controller: Some(true),
                        block_owner_deletion: None,
                    }]
                }),
                ..Default::default()
            },
            spec: Some(PodSpec::default()),
            status: Some(PodStatus {
                phase: Some("Running".to_owned()),
                ..Default::default()
            }),
        }
    }

    fn with_phase(mut pod: Pod, phase: &str) -> Pod {
        pod.status.as_mut().unwrap().phase = Some(phase.to_owned());
        pod
    }

    fn with_empty_dir(mut pod: Pod) -> Pod {
        pod.spec.as_mut().unwrap().volumes = Some(vec![Volume {
            name: "scratch".to_owned(),
            empty_dir: Some(EmptyDirVolumeSource::default()),
            ..Default::default()
        }]);
        pod
    }

    fn names(pods: &[DrainPod]) -> Vec<&str> {
        pods.iter().map(|pod| pod.name.as_str()).collect()
    }

    #[test]
    fn drain_evicts_controlled_pods_and_leaves_daemon_set_and_mirror_pods() {
        let mut mirror = pod("kube-apiserver-node", Some("Node"));
        mirror.metadata.annotations = Some(BTreeMap::from([(
            MIRROR_POD_ANNOTATION.to_owned(),
            "hash".to_owned(),
        )]));
        let mut non_controller_owner = pod("adopted", Some("ReplicaSet"));
        non_controller_owner
            .metadata
            .owner_references
            .as_mut()
            .unwrap()[0]
            .controller = None;
        let pods = [
            pod("api", Some("ReplicaSet")),
            pod("db-0", Some("StatefulSet")),
            pod("kube-proxy", Some("DaemonSet")),
            mirror,
        ];

        let plan = plan_drain(&pods);

        assert_eq!(names(&plan.evict), ["api", "db-0"]);
        assert_eq!(names(&plan.daemon_set_pods), ["kube-proxy"]);
        assert_eq!(names(&plan.mirror_pods), ["kube-apiserver-node"]);
        assert!(plan.blocked.is_empty());
        assert_eq!(
            plan_drain([&non_controller_owner]).blocked,
            [(
                DrainPod::of(&non_controller_owner),
                DrainBlocker::NoController
            )],
            "an owner that is not the controller does not recreate the Pod"
        );
    }

    #[test]
    fn running_pods_without_a_controller_or_with_empty_dir_storage_block_the_drain() {
        let pods = [
            pod("bare", None),
            with_empty_dir(pod("cache", Some("ReplicaSet"))),
            pod("api", Some("ReplicaSet")),
        ];

        let plan = plan_drain(&pods);

        assert_eq!(
            plan.blocked,
            [
                (DrainPod::of(&pods[0]), DrainBlocker::NoController),
                (DrainPod::of(&pods[1]), DrainBlocker::LocalStorage),
            ]
        );
        assert_eq!(names(&plan.evict), ["api"]);
    }

    #[test]
    fn finished_pods_are_removed_whatever_their_controller_or_storage() {
        let pods = [
            with_phase(pod("bare-done", None), "Succeeded"),
            with_phase(with_empty_dir(pod("cache-failed", None)), "Failed"),
            with_phase(pod("agent-done", Some("DaemonSet")), "Succeeded"),
        ];

        let plan = plan_drain(&pods);

        assert_eq!(
            names(&plan.evict),
            ["bare-done", "cache-failed", "agent-done"]
        );
        assert!(plan.blocked.is_empty() && plan.daemon_set_pods.is_empty());
    }

    #[test]
    fn a_report_is_complete_only_without_blocked_or_failed_pods() {
        let api = DrainPod::of(&pod("api", Some("ReplicaSet")));
        assert!(
            NodeDrainReport {
                evicted: vec![api.clone()],
                daemon_set_pods: 1,
                ..Default::default()
            }
            .is_complete()
        );
        assert!(
            !NodeDrainReport {
                failed: vec![(api.clone(), "refused".to_owned())],
                ..Default::default()
            }
            .is_complete()
        );
        assert!(
            !NodeDrainReport {
                blocked: vec![(api, DrainBlocker::NoController)],
                ..Default::default()
            }
            .is_complete()
        );
        assert_eq!(
            DrainPod::of(&pod("api", None)).display_name(),
            "apps/api",
            "Pods are named like kubectl names them"
        );
    }

    const EVICT: &str = "POST /api/v1/namespaces/apps/pods/api/eviction";
    const DELETE: &str = "DELETE /api/v1/namespaces/apps/pods/api";
    const GET: &str = "GET /api/v1/namespaces/apps/pods/api";

    fn status(code: u16, message: &str, causes: &str) -> (u16, String) {
        let outcome = if code < 300 { "Success" } else { "Failure" };
        let reason = if code == 404 { "NotFound" } else { "" };
        (
            code,
            format!(
                r#"{{"kind":"Status","apiVersion":"v1","status":"{outcome}","message":"{message}","reason":"{reason}","code":{code},"details":{{"causes":[{causes}]}}}}"#
            ),
        )
    }

    fn pod_with_uid(uid: &str) -> (u16, String) {
        (
            200,
            format!(
                r#"{{"apiVersion":"v1","kind":"Pod","metadata":{{"name":"api","namespace":"apps","uid":"{uid}"}}}}"#
            ),
        )
    }

    fn api_pod() -> DrainPod {
        DrainPod {
            namespace: "apps".to_owned(),
            name: "api".to_owned(),
            uid: Some("api-uid".to_owned()),
        }
    }

    fn quick_options() -> DrainOptions {
        DrainOptions {
            eviction_timeout: Duration::from_millis(300),
            retry_interval: Duration::from_millis(10),
            concurrency: 1,
        }
    }

    /// Evicts the `api` Pod from the server `reply` scripts and returns the outcome with the
    /// requests made.
    async fn evict(
        reply: impl Fn(&str, usize) -> (u16, String) + Send + Sync + 'static,
    ) -> (std::result::Result<(), String>, Vec<(String, String)>) {
        let api = FakeKubeApi::start(move |request| {
            let (status, body) = reply(&request.route(), request.nth);
            Reply::json(status, body)
        })
        .await;
        let options = quick_options();
        let deadline = Instant::now() + options.eviction_timeout;
        let outcome = evict_pod(api.client(), &api_pod(), deadline, options).await;
        let requests = api
            .requests()
            .into_iter()
            .map(|request| (request.route(), request.body))
            .collect();
        (outcome, requests)
    }

    fn routes(requests: &[(String, String)]) -> Vec<&str> {
        requests.iter().map(|(route, _)| route.as_str()).collect()
    }

    #[tokio::test]
    async fn an_accepted_eviction_names_the_pod_uid_as_a_precondition() {
        for code in [200, 201] {
            let (outcome, requests) = evict(move |_, _| status(code, "", "")).await;
            assert_eq!(outcome, Ok(()), "HTTP {code}");
            assert_eq!(routes(&requests), [EVICT]);
            assert!(
                requests[0]
                    .1
                    .contains(r#""preconditions":{"uid":"api-uid"}"#),
                "the eviction only removes the Pod it listed: {}",
                requests[0].1
            );
        }
    }

    #[tokio::test]
    async fn a_pod_that_is_already_gone_counts_as_evicted() {
        let (outcome, requests) = evict(|_, _| status(404, "pods \"api\" not found", "")).await;
        assert_eq!(outcome, Ok(()));
        assert_eq!(routes(&requests), [EVICT]);
    }

    #[tokio::test]
    async fn a_failed_uid_precondition_is_fine_when_the_pod_was_replaced_or_removed() {
        for (case, current) in [
            ("replaced", pod_with_uid("replacement-uid")),
            ("removed", status(404, "gone", "")),
        ] {
            let (outcome, requests) = evict(move |route, _| match route {
                EVICT => status(409, "uid mismatch", ""),
                _ => current.clone(),
            })
            .await;
            assert_eq!(outcome, Ok(()), "{case}: {}", requests.len());
            assert_eq!(routes(&requests), [EVICT, GET]);
        }
    }

    #[tokio::test]
    async fn a_conflict_on_the_same_pod_is_retried_until_the_deadline() {
        let (outcome, requests) = evict(|route, _| match route {
            EVICT => status(409, "the object has been modified", ""),
            _ => pod_with_uid("api-uid"),
        })
        .await;
        assert_eq!(outcome, Err("the object has been modified".to_owned()));
        assert!(
            routes(&requests)
                .iter()
                .filter(|route| **route == EVICT)
                .count()
                > 2,
            "retried: {requests:?}"
        );
    }

    #[tokio::test]
    async fn a_budget_that_refuses_at_first_is_retried_until_it_allows_the_eviction() {
        let (outcome, requests) = evict(|_, nth| {
            if nth < 2 {
                status(
                    429,
                    "Cannot evict pod as it would violate the pod's disruption budget.",
                    "",
                )
            } else {
                status(201, "", "")
            }
        })
        .await;
        assert_eq!(outcome, Ok(()));
        assert_eq!(routes(&requests), [EVICT; 3]);
    }

    #[tokio::test]
    async fn a_budget_that_keeps_refusing_is_reported_and_never_bypassed_with_a_delete() {
        let (outcome, requests) = evict(|_, _| status(429, "violates the budget", "")).await;
        assert_eq!(
            outcome,
            Err("Still refused after 0s: violates the budget".to_owned())
        );
        assert!(routes(&requests).len() > 2);
        assert!(
            routes(&requests).iter().all(|route| *route == EVICT),
            "only evictions were requested: {requests:?}"
        );
    }

    #[tokio::test]
    async fn a_terminating_namespace_gets_the_pod_deleted_instead() {
        let terminating = r#"{"reason":"NamespaceTerminating"}"#;
        for (deleted, expected) in [
            (status(200, "", ""), Ok(())),
            (status(404, "gone", ""), Ok(())),
            (status(500, "etcd down", ""), Err("etcd down".to_owned())),
        ] {
            let (outcome, requests) = evict(move |route, _| match route {
                EVICT => status(403, "namespace is terminating", terminating),
                _ => deleted.clone(),
            })
            .await;
            assert_eq!(outcome, expected);
            assert_eq!(routes(&requests), [EVICT, DELETE]);
            assert!(
                requests[1]
                    .1
                    .contains(r#""preconditions":{"uid":"api-uid"}"#),
                "the delete is also limited to the listed Pod: {}",
                requests[1].1
            );
        }
    }

    #[tokio::test]
    async fn any_other_refusal_fails_the_pod_without_a_retry_or_delete() {
        let (outcome, requests) = evict(|_, _| status(403, "pods is forbidden", "")).await;
        assert_eq!(outcome, Err("pods is forbidden".to_owned()));
        assert_eq!(routes(&requests), [EVICT]);
    }
}
