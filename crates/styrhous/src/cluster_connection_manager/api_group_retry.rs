//! Background recovery of API groups whose discovery failed during the initial inspection,
//! so a transient failure (for example an aggregated API that was still starting) does not
//! leave the cluster half-loaded until the application restarts.

use super::{
    ApiGroupTarget, DiscoveredApiResources, KubernetesApiInspector, node_metrics_api_available,
    pod_metrics_api_available,
};
use crate::api_resource::ApiResource;
use crate::helpers::ResultExt;
use crate::worker::*;
use anyhow::Result;
use futures_util::stream::StreamExt;
use kube::runtime::watcher::DefaultBackoff;
use std::collections::BTreeSet;
use std::time::Duration;

/// A retry that has not answered by now is counted as failed, like in the initial discovery.
const GROUP_RETRY_TIMEOUT: Duration = Duration::from_secs(15);
const CONCURRENT_GROUP_RETRIES: usize = 4;
const CRD_METADATA_TIMEOUT: Duration = Duration::from_secs(5);

/// Groups that answered a retry, with the resources they expose.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RecoveredApiGroups {
    pub(crate) group_versions: Vec<String>,
    pub(crate) api_resources: Vec<ApiResource>,
    pub(crate) scalable_api_resources: BTreeSet<ApiResource>,
    /// Groups that are still failing and will be retried.
    pub(crate) still_failing: Vec<String>,
}

/// What one round hands to the publisher.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RetryUpdate {
    /// Groups recovered in this round, if any.
    pub(crate) recovered: Option<RecoveredApiGroups>,
    /// API group names whose custom resource metadata has not been delivered yet.
    pub(crate) metadata_groups: BTreeSet<String>,
}

/// Retries `pending` forever, passing recoveries to `publish`, until every group has
/// recovered and its metadata is delivered. `publish` returns `None` when nobody is
/// listening any more, which ends the loop, or `Some(true)` once the metadata was delivered
/// (`Some(false)` keeps it pending for the next round). The owner stops the loop by
/// dropping (aborting) the future.
///
/// `delays` supplies the wait before each round and `sleep` performs it, so tests can run
/// the schedule without real time.
pub(crate) async fn retry_failed_api_groups<Sleep, Fetch, Publish>(
    mut delays: impl Iterator<Item = Duration>,
    sleep: impl Fn(Duration) -> Sleep,
    fetch_group: impl Fn(ApiGroupTarget) -> Fetch,
    mut publish: impl FnMut(RetryUpdate) -> Publish,
    mut pending: Vec<ApiGroupTarget>,
) where
    Sleep: Future<Output = ()>,
    Fetch: Future<Output = Result<DiscoveredApiResources>>,
    Publish: Future<Output = Option<bool>>,
{
    let mut metadata_pending = BTreeSet::<String>::new();
    while !pending.is_empty() || !metadata_pending.is_empty() {
        let Some(delay) = delays.next() else {
            return;
        };
        sleep(delay).await;

        let attempts = futures_util::stream::iter(std::mem::take(&mut pending))
            .map(|target| {
                let fetch_group = &fetch_group;
                async move {
                    let outcome = fetch_group(target.clone()).await;
                    (target, outcome)
                }
            })
            .buffered(CONCURRENT_GROUP_RETRIES)
            .collect::<Vec<_>>()
            .await;

        let mut recovered = RecoveredApiGroups::default();
        for (target, outcome) in attempts {
            match outcome {
                Ok(discovered) => {
                    metadata_pending.insert(target.group_name);
                    recovered.group_versions.push(target.group_version);
                    recovered.api_resources.extend(discovered.api_resources);
                    recovered
                        .scalable_api_resources
                        .extend(discovered.scalable_api_resources);
                }
                Err(error) => {
                    tracing::debug!(
                        group_version = %target.group_version,
                        error = %format!("{error:#}"),
                        "API group discovery retry failed"
                    );
                    pending.push(target);
                }
            }
        }
        let recovered = (!recovered.group_versions.is_empty()).then(|| RecoveredApiGroups {
            still_failing: pending
                .iter()
                .map(|target| target.group_version.clone())
                .collect(),
            ..recovered
        });
        if recovered.is_none() && metadata_pending.is_empty() {
            continue;
        }
        let update = RetryUpdate {
            recovered,
            metadata_groups: metadata_pending.clone(),
        };
        match publish(update).await {
            None => return,
            Some(true) => metadata_pending.clear(),
            Some(false) => {}
        }
    }
}

/// Retries against a real cluster and reports recoveries to the UI.
pub(crate) async fn retry_failed_api_groups_for_cluster(
    cluster_key: i32,
    client: kube::Client,
    event_output: WorkerResultSender,
    failed: Vec<ApiGroupTarget>,
) {
    retry_cluster_api_groups(
        cluster_key,
        client,
        event_output,
        failed,
        DefaultBackoff::default(),
    )
    .await;
}

pub(crate) async fn retry_cluster_api_groups(
    cluster_key: i32,
    client: kube::Client,
    event_output: WorkerResultSender,
    failed: Vec<ApiGroupTarget>,
    delays: impl Iterator<Item = Duration>,
) {
    retry_failed_api_groups(
        delays,
        tokio::time::sleep,
        |target| {
            let client = client.clone();
            async move { fetch_group(&client, &target).await }
        },
        |update| {
            let client = client.clone();
            let event_output = event_output.clone();
            async move { publish_update(cluster_key, &client, &event_output, update).await }
        },
        failed,
    )
    .await;
}

async fn fetch_group(
    client: &kube::Client,
    target: &ApiGroupTarget,
) -> Result<DiscoveredApiResources> {
    let inspector = KubernetesApiInspector {
        client: client.clone(),
    };
    match tokio::time::timeout(
        GROUP_RETRY_TIMEOUT,
        inspector.get_api_resources_for_group_versions(target),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => anyhow::bail!("timed out waiting for the API server to respond"),
    }
}

async fn publish_update(
    cluster_key: i32,
    client: &kube::Client,
    event_output: &WorkerResultSender,
    update: RetryUpdate,
) -> Option<bool> {
    if let Some(recovered) = update.recovered {
        let pod_metrics_api_available = pod_metrics_api_available(&recovered.api_resources);
        let node_metrics_api_available = node_metrics_api_available(&recovered.api_resources);
        event_output
            .send(KubernetesApiGroupsRecovered {
                cluster_key,
                group_versions: recovered.group_versions,
                api_resources: recovered.api_resources,
                scalable_api_resources: recovered.scalable_api_resources,
                still_failing_group_versions: recovered.still_failing,
                pod_metrics_api_available,
                node_metrics_api_available,
            })
            .await
            .ok()?;
    }

    // A recovered group may be an aggregated API that serves custom resources, so fetch
    // their optional printer columns and schemas, bounded like the initial discovery.
    let inspector = KubernetesApiInspector {
        client: client.clone(),
    };
    let metadata = tokio::time::timeout(
        CRD_METADATA_TIMEOUT,
        inspector.try_custom_resource_metadata(),
    )
    .await;
    let (mut columns, mut schemas) = match metadata {
        Ok(Ok(metadata)) => metadata,
        Ok(Err(error)) => {
            tracing::warn!(%error, "listing CustomResourceDefinitions failed; will retry");
            return Some(false);
        }
        Err(_) => {
            tracing::warn!("listing CustomResourceDefinitions timed out; will retry");
            return Some(false);
        }
    };
    columns.retain(|resource, _| update.metadata_groups.contains(&resource.group));
    schemas.retain(|resource, _| update.metadata_groups.contains(&resource.group));
    if !columns.is_empty() {
        event_output
            .send(KubernetesCustomResourceColumnsLoaded {
                cluster_key,
                columns,
            })
            .await
            .log_if_error("Failed to send custom resource columns");
    }
    if !schemas.is_empty() {
        event_output
            .send(KubernetesResourceSchemasLoaded {
                cluster_key,
                schemas,
            })
            .await
            .log_if_error("Failed to send custom resource schemas");
    }
    Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::GroupVersionForDiscovery;
    use std::cell::RefCell;
    use std::collections::{BTreeMap, VecDeque};

    fn target(group: &str) -> ApiGroupTarget {
        ApiGroupTarget {
            group_version: format!("{group}/v1"),
            group_name: group.to_owned(),
            versions: vec![GroupVersionForDiscovery {
                group_version: format!("{group}/v1"),
                version: "v1".to_owned(),
            }],
        }
    }

    fn resource(group: &str) -> ApiResource {
        ApiResource {
            group: group.to_owned(),
            version: "v1".to_owned(),
            kind: "Thing".to_owned(),
            name: "things".to_owned(),
            namespaced: true,
        }
    }

    /// Scripted per-group results (exhausted scripts keep failing), a recorded sleep
    /// schedule and recorded updates.
    #[derive(Default)]
    struct Fake {
        scripts: RefCell<BTreeMap<String, VecDeque<bool>>>,
        sleeps: RefCell<Vec<Duration>>,
        updates: RefCell<Vec<RetryUpdate>>,
        metadata_outcomes: RefCell<VecDeque<bool>>,
        stop_after_sleeps: Option<usize>,
    }

    impl Fake {
        fn script(self, group: &str, succeeds: &[bool]) -> Self {
            self.scripts
                .borrow_mut()
                .insert(format!("{group}/v1"), succeeds.iter().copied().collect());
            self
        }

        async fn run(&self, delays: &[u64], pending: Vec<ApiGroupTarget>) {
            retry_failed_api_groups(
                delays.iter().copied().map(Duration::from_secs),
                |delay| async move {
                    self.sleeps.borrow_mut().push(delay);
                    if self.stop_after_sleeps == Some(self.sleeps.borrow().len()) {
                        std::future::pending::<()>().await;
                    }
                },
                |target| async move {
                    let succeeds = self
                        .scripts
                        .borrow_mut()
                        .get_mut(&target.group_version)
                        .and_then(VecDeque::pop_front)
                        .unwrap_or(false);
                    if !succeeds {
                        bail!("503 Service Unavailable");
                    }
                    let resource = resource(&target.group_name);
                    Ok(DiscoveredApiResources {
                        scalable_api_resources: BTreeSet::from([resource.clone()]),
                        api_resources: vec![resource],
                    })
                },
                |update| async move {
                    self.updates.borrow_mut().push(update);
                    Some(
                        self.metadata_outcomes
                            .borrow_mut()
                            .pop_front()
                            .unwrap_or(true),
                    )
                },
                pending,
            )
            .await;
        }
    }

    const DELAYS: [u64; 8] = [5, 10, 20, 40, 40, 40, 40, 40];

    #[tokio::test]
    async fn a_group_that_fails_twice_then_succeeds_is_published_once() {
        let fake = Fake::default().script("metrics.k8s.io", &[false, false, true]);

        fake.run(&DELAYS, vec![target("metrics.k8s.io")]).await;

        assert_eq!(
            *fake.updates.borrow(),
            vec![RetryUpdate {
                recovered: Some(RecoveredApiGroups {
                    group_versions: vec!["metrics.k8s.io/v1".to_owned()],
                    api_resources: vec![resource("metrics.k8s.io")],
                    scalable_api_resources: BTreeSet::from([resource("metrics.k8s.io")]),
                    still_failing: Vec::new(),
                }),
                metadata_groups: BTreeSet::from(["metrics.k8s.io".to_owned()]),
            }]
        );
        assert_eq!(
            *fake.sleeps.borrow(),
            [5, 10, 20].map(Duration::from_secs),
            "the loop ends once nothing is left to retry"
        );
    }

    #[tokio::test]
    async fn recoveries_are_published_per_round_and_report_the_groups_still_failing() {
        let fake = Fake::default()
            .script("a.example.com", &[true])
            .script("b.example.com", &[false, true]);

        fake.run(
            &DELAYS,
            vec![target("a.example.com"), target("b.example.com")],
        )
        .await;

        let updates = fake.updates.borrow();
        assert_eq!(updates.len(), 2);
        let first = updates[0].recovered.as_ref().unwrap();
        assert_eq!(first.group_versions, vec!["a.example.com/v1"]);
        assert_eq!(first.still_failing, vec!["b.example.com/v1"]);
        let second = updates[1].recovered.as_ref().unwrap();
        assert_eq!(second.group_versions, vec!["b.example.com/v1"]);
        assert!(second.still_failing.is_empty());
    }

    #[tokio::test]
    async fn a_group_that_never_recovers_keeps_retrying_on_the_supplied_growing_delays() {
        let fake = Fake {
            stop_after_sleeps: Some(8),
            ..Default::default()
        };

        let outcome = tokio::time::timeout(
            Duration::from_millis(200),
            fake.run(&DELAYS, vec![target("stale.example.com")]),
        )
        .await;

        assert!(outcome.is_err(), "the loop must not give up by itself");
        assert!(fake.updates.borrow().is_empty());
        assert_eq!(*fake.sleeps.borrow(), DELAYS.map(Duration::from_secs));
    }

    #[tokio::test]
    async fn metadata_that_could_not_be_fetched_is_retried_on_the_next_round() {
        let fake = Fake::default().script("example.com", &[true]);
        fake.metadata_outcomes
            .borrow_mut()
            .extend([false, false, true]);

        fake.run(&DELAYS, vec![target("example.com")]).await;

        let updates = fake.updates.borrow();
        assert_eq!(updates.len(), 3);
        assert!(updates[0].recovered.is_some());
        assert!(updates[1..].iter().all(|update| update.recovered.is_none()));
        assert!(
            updates
                .iter()
                .all(|update| update.metadata_groups.contains("example.com"))
        );
        assert_eq!(fake.sleeps.borrow().len(), 3);
    }

    #[tokio::test]
    async fn the_loop_ends_when_the_receiver_is_gone() {
        let sleeps = RefCell::new(0);
        retry_failed_api_groups(
            DELAYS.map(Duration::from_secs).into_iter(),
            |_| async { *sleeps.borrow_mut() += 1 },
            |target| async move {
                let resource = resource(&target.group_name);
                Ok(DiscoveredApiResources {
                    scalable_api_resources: BTreeSet::new(),
                    api_resources: vec![resource],
                })
            },
            |_| async { None },
            vec![target("a.example.com"), target("b.example.com")],
        )
        .await;

        assert_eq!(*sleeps.borrow(), 1);
    }

    #[test]
    fn the_cluster_backoff_grows_and_stays_bounded() {
        let delays = DefaultBackoff::default().take(20).collect::<Vec<_>>();
        // kube jitters the 30s cap itself, so delays stay within twice of it.
        assert!(
            delays.iter().all(|delay| *delay <= Duration::from_secs(60)),
            "{delays:?}"
        );
        assert!(delays[0] < Duration::from_secs(2));
        assert!(delays[19] > delays[0]);
    }
}
