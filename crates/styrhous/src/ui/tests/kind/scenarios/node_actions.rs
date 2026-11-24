//! Kind Node maintenance scenarios: cordon, uncordon and drain.
//!
//! They act only on a fake Node of their own (`IntegrationNamespaceFixture::create_fake_node`):
//! cordoning or draining the cluster's real Node would break every concurrent test.

use super::*;
use crate::cluster_connection_manager::{DrainBlocker, DrainOptions, drain_node};
use crate::ui::MyEguiApp;
use crate::ui::state::OperationOutcome;
use crate::worker::Worker;
use egui_kittest::Harness;
use k8s_openapi::api::apps::v1::{DaemonSet, DaemonSetSpec, DaemonSetUpdateStrategy};
use k8s_openapi::api::core::v1::{Node, Toleration};
use k8s_openapi::api::policy::v1::{PodDisruptionBudget, PodDisruptionBudgetSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use std::time::Duration;

const NODE_ACTION_TIMEOUT_MS: u64 = 20_000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// A pause Pod bound to `node_name` directly, labelled `app=<app>` and
/// controlled by `controller`. It tolerates every taint, so the Node lifecycle
/// controller never evicts it from the kubelet-less Node.
fn node_pod(
    namespace: &str,
    name: &str,
    app: &str,
    node_name: &str,
    controller: Option<OwnerReference>,
) -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(BTreeMap::from([("app".to_owned(), app.to_owned())])),
            owner_references: controller.map(|owner| vec![owner]),
            ..Default::default()
        },
        spec: Some(PodSpec {
            node_name: Some(node_name.to_owned()),
            tolerations: Some(vec![Toleration {
                operator: Some("Exists".to_owned()),
                ..Default::default()
            }]),
            containers: vec![pause_container()],
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn controller(api_version: &str, kind: &str, name: &str, uid: Option<String>) -> OwnerReference {
    OwnerReference {
        api_version: api_version.to_owned(),
        kind: kind.to_owned(),
        name: name.to_owned(),
        uid: uid.expect("the controller has a UID"),
        controller: Some(true),
        block_owner_deletion: None,
    }
}

/// The fixture's ConfigMap as a Pod controller: a Pod that names a controller
/// is one `kubectl drain` evicts, and the garbage collector keeps it because
/// the owner exists.
fn anchor_controller(fixture: &IntegrationNamespaceFixture) -> OwnerReference {
    let anchor = kubernetes_request(
        &fixture.runtime,
        REQUEST_TIMEOUT,
        fixture.configmaps.get(&fixture.name),
    )
    .expect("the anchor ConfigMap exists");
    controller("v1", "ConfigMap", &fixture.name, anchor.metadata.uid)
}

fn create_pods(fixture: &IntegrationNamespaceFixture, pods: &Api<Pod>, new_pods: Vec<Pod>) {
    for pod in new_pods {
        kubernetes_request(
            &fixture.runtime,
            REQUEST_TIMEOUT,
            pods.create(&Default::default(), &pod),
        )
        .expect("Failed to create a Pod on the fake Node");
    }
}

fn is_deleting(fixture: &IntegrationNamespaceFixture, pods: &Api<Pod>, name: &str) -> bool {
    kubernetes_request(&fixture.runtime, REQUEST_TIMEOUT, pods.get(name))
        .expect("the Pod can be read")
        .metadata
        .deletion_timestamp
        .is_some()
}

fn is_unschedulable(fixture: &IntegrationNamespaceFixture, node_name: &str) -> bool {
    let nodes: Api<Node> = Api::all(fixture.client.clone());
    kubernetes_request(&fixture.runtime, REQUEST_TIMEOUT, nodes.get(node_name))
        .expect("the fake Node can be read")
        .spec
        .and_then(|spec| spec.unschedulable)
        .unwrap_or(false)
}

/// Waits until the Nodes table's watch shows `node_name` cordoned or not.
fn wait_for_cordon_shown(
    harness: &mut Harness<MyEguiApp<Worker>>,
    cluster_key: i32,
    node_name: &str,
    cordoned: bool,
) {
    wait_for_resource_watch(
        harness,
        &format!("the Nodes table to show {node_name} cordoned: {cordoned}"),
        cluster_key,
        &crate::resource_handlers::node::api_resource(),
        None,
        |watch| {
            watch
                .resources
                .values()
                .find(|resource| resource.name == node_name)
                .filter(|resource| {
                    crate::resource_handlers::node::is_cordoned(resource) == cordoned
                })
                .map(|_| ())
        },
        NODE_ACTION_TIMEOUT_MS,
    );
}

/// Opens the row menu of `node_name` and clicks `item` in it.
fn click_node_menu_item(harness: &mut Harness<MyEguiApp<Worker>>, node_name: &str, item: &str) {
    let more = format!("More actions for {node_name}");
    wait_for_settled_clickable(harness, &more, NODE_ACTION_TIMEOUT_MS);
    harness.get_by_label(&more).click();
    wait_for_settled_clickable(harness, item, 5_000);
    harness.get_by_label(item).click();
    harness.run_steps(1);
}

/// Waits for the newest outcome titled `title` about `node_name` and returns its details.
fn wait_for_node_outcome(
    harness: &mut Harness<MyEguiApp<Worker>>,
    cluster_key: i32,
    node_name: &str,
    title: &str,
) -> Option<String> {
    wait_for_with_diagnostic(
        harness,
        &format!("the outcome {title} for Node {node_name}"),
        |app| {
            app.ui_state.clusters[&cluster_key]
                .operation_history
                .iter()
                .rev()
                .find(|entry| entry.title == title && entry.target == node_name)
                .map(|entry| entry.details.clone())
        },
        |app| {
            app.ui_state.clusters[&cluster_key]
                .operation_history
                .iter()
                .rev()
                .find(|entry| {
                    entry.outcome == OperationOutcome::Failure && entry.target == node_name
                })
                .map(|entry| format!("{}: {:?}", entry.title, entry.details))
        },
        NODE_ACTION_TIMEOUT_MS,
    )
}

/// A PodDisruptionBudget keeping at least one `app=<app>` Pod available.
fn budget(namespace: &str, app: &str) -> PodDisruptionBudget {
    PodDisruptionBudget {
        metadata: ObjectMeta {
            name: Some(app.to_owned()),
            namespace: Some(namespace.to_owned()),
            ..Default::default()
        },
        spec: Some(PodDisruptionBudgetSpec {
            min_available: Some(IntOrString::Int(1)),
            selector: Some(LabelSelector {
                match_labels: Some(BTreeMap::from([("app".to_owned(), app.to_owned())])),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Creates the Pod `name` on the fake Node, marked running and guarded by a
/// PodDisruptionBudget that refuses its eviction: it is not ready, so the
/// budget has no disruption to spare. (The Eviction API ignores budgets for
/// Pending Pods, and no kubelet runs this one.)
fn create_guarded_pod(
    fixture: &IntegrationNamespaceFixture,
    pods: &Api<Pod>,
    node_name: &str,
    name: &str,
) {
    let budgets: Api<PodDisruptionBudget> =
        Api::namespaced(fixture.client.clone(), &fixture.namespace);
    create_pods(
        fixture,
        pods,
        vec![node_pod(
            &fixture.namespace,
            name,
            name,
            node_name,
            Some(anchor_controller(fixture)),
        )],
    );
    fixture.runtime.block_on(async {
        pods.patch_status(
            name,
            &Default::default(),
            &Patch::Merge(k8s_openapi::serde_json::json!({ "status": { "phase": "Running" } })),
        )
        .await
        .expect("Failed to mark the guarded Pod running");
        budgets
            .create(&Default::default(), &budget(&fixture.namespace, name))
            .await
            .expect("Failed to create the PodDisruptionBudget");
    });
}

/// Cordons, uncordons and drains a fake Node from its row menu, and checks
/// the Nodes table counts its Pods. The drain evicts the controlled Pods and
/// leaves the DaemonSet's; the evicted Pods never terminate on a kubelet-less
/// Node, so the evictions show as deletion timestamps.
#[test]
fn test_node_cordon_and_drain_integration() {
    let mut fixture = IntegrationNamespaceFixture::create("node-drain", "anchor", "unused");
    let node_name = fixture.create_fake_node();
    let namespace = fixture.namespace.clone();
    let pods: Api<Pod> = Api::namespaced(fixture.client.clone(), &namespace);
    let daemon_sets: Api<DaemonSet> = Api::namespaced(fixture.client.clone(), &namespace);
    // The DaemonSet targets only the fake Node, whose taint it does not tolerate, and updates
    // only on delete: its controller neither creates a Pod there nor removes the one the test
    // binds there.
    let agent_labels = BTreeMap::from([("app".to_owned(), "agent".to_owned())]);
    let daemon_set = kubernetes_request(
        &fixture.runtime,
        REQUEST_TIMEOUT,
        daemon_sets.create(
            &Default::default(),
            &DaemonSet {
                metadata: ObjectMeta {
                    name: Some("agent".to_owned()),
                    namespace: Some(namespace.clone()),
                    ..Default::default()
                },
                spec: Some(DaemonSetSpec {
                    // A rolling update would replace the Pod the test made, which lacks the
                    // controller revision label of the DaemonSet's own Pods.
                    update_strategy: Some(DaemonSetUpdateStrategy {
                        type_: Some("OnDelete".to_owned()),
                        ..Default::default()
                    }),
                    selector: LabelSelector {
                        match_labels: Some(agent_labels.clone()),
                        ..Default::default()
                    },
                    template: PodTemplateSpec {
                        metadata: Some(ObjectMeta {
                            labels: Some(agent_labels),
                            ..Default::default()
                        }),
                        spec: Some(PodSpec {
                            node_selector: Some(BTreeMap::from([(
                                "kubernetes.io/hostname".to_owned(),
                                node_name.clone(),
                            )])),
                            containers: vec![pause_container()],
                            ..Default::default()
                        }),
                    },
                    ..Default::default()
                }),
                ..Default::default()
            },
        ),
    )
    .expect("Failed to create the DaemonSet");
    let anchor = anchor_controller(&fixture);
    create_pods(
        &fixture,
        &pods,
        vec![
            node_pod(&namespace, "web-0", "web", &node_name, Some(anchor.clone())),
            node_pod(&namespace, "web-1", "web", &node_name, Some(anchor)),
            node_pod(
                &namespace,
                "agent-0",
                "agent",
                &node_name,
                Some(controller(
                    "apps/v1",
                    "DaemonSet",
                    "agent",
                    daemon_set.metadata.uid,
                )),
            ),
        ],
    );

    let (mut harness, cluster_key) = connected_kind_harness();
    let more = format!("More actions for {node_name}");
    wait_for_settled_clickable(&mut harness, "Nodes", NODE_ACTION_TIMEOUT_MS);
    harness.get_by_label("Nodes").click();
    harness.run_steps(1);
    let node_resource = crate::resource_handlers::node::api_resource();
    wait_for_resource_sync(&mut harness, cluster_key, &node_resource, None);
    wait_for(
        &mut harness,
        &format!("the Nodes table to count the 3 Pods on {node_name}"),
        |app| {
            (app.ui_state.clusters[&cluster_key]
                .node_pod_counts
                .count(&node_name)
                == Ok(Some(3)))
            .then_some(())
        },
        NODE_ACTION_TIMEOUT_MS,
    );

    click_node_menu_item(&mut harness, &node_name, "Cordon");
    wait_for_node_outcome(&mut harness, cluster_key, &node_name, "Node cordoned");
    assert!(
        is_unschedulable(&fixture, &node_name),
        "Cordon marks the Node unschedulable"
    );

    // The row menu offers Uncordon once the Node watch reports the cordon.
    wait_for_cordon_shown(&mut harness, cluster_key, &node_name, true);
    click_node_menu_item(&mut harness, &node_name, "Uncordon");
    wait_for_node_outcome(&mut harness, cluster_key, &node_name, "Node uncordoned");
    assert!(
        !is_unschedulable(&fixture, &node_name),
        "Uncordon makes the Node schedulable"
    );

    wait_for_cordon_shown(&mut harness, cluster_key, &node_name, false);
    click_node_menu_item(&mut harness, &node_name, "Drain…");
    assert_eq!(
        harness.state().ui_state.clusters[&cluster_key]
            .pending_node_drain
            .as_ref()
            .map(|pending| pending.node_name.as_str()),
        Some(node_name.as_str()),
        "Drain… asks for confirmation first"
    );
    wait_for_settled_clickable(&mut harness, "Drain", 5_000);
    harness.get_by_label("Drain").click();
    harness.run_steps(1);
    let details = wait_for_node_outcome(&mut harness, cluster_key, &node_name, "Node drained");
    assert_eq!(
        details.as_deref(),
        Some("Evicted 2 Pods.\nLeft 1 DaemonSet Pod running.")
    );
    assert!(
        harness.query_by_label(&more).is_some(),
        "the drained Node stays listed"
    );

    assert!(
        is_unschedulable(&fixture, &node_name),
        "a drain cordons the Node"
    );
    assert!(
        is_deleting(&fixture, &pods, "web-0") && is_deleting(&fixture, &pods, "web-1"),
        "the controlled Pods were evicted"
    );
    assert!(
        !is_deleting(&fixture, &pods, "agent-0"),
        "the DaemonSet Pod stays"
    );
}

/// Drains a fake Node directly through the worker's drain: a Pod nothing
/// would recreate stops the drain before any eviction, and a
/// PodDisruptionBudget's refusal is retried and then reported, never bypassed.
#[test]
fn test_node_drain_respects_unmanaged_pods_and_disruption_budgets_integration() {
    let mut fixture = IntegrationNamespaceFixture::create("node-drain-pdb", "anchor", "unused");
    let node_name = fixture.create_fake_node();
    let pods: Api<Pod> = Api::namespaced(fixture.client.clone(), &fixture.namespace);
    let options = DrainOptions {
        eviction_timeout: Duration::from_secs(3),
        retry_interval: Duration::from_millis(500),
        concurrency: 4,
    };
    create_guarded_pod(&fixture, &pods, &node_name, "guarded");
    create_pods(
        &fixture,
        &pods,
        vec![node_pod(
            &fixture.namespace,
            "bare",
            "bare",
            &node_name,
            None,
        )],
    );

    let report = fixture
        .runtime
        .block_on(drain_node(fixture.client.clone(), &node_name, options))
        .expect("the drain lists the Node's Pods");
    assert_eq!(
        report
            .blocked
            .iter()
            .map(|(pod, blocker)| (pod.name.as_str(), *blocker))
            .collect::<Vec<_>>(),
        [("bare", DrainBlocker::NoController)]
    );
    assert!(report.evicted.is_empty() && report.failed.is_empty());
    assert!(
        !is_deleting(&fixture, &pods, "guarded") && !is_deleting(&fixture, &pods, "bare"),
        "a blocked drain evicts nothing"
    );

    kubernetes_request(
        &fixture.runtime,
        REQUEST_TIMEOUT,
        pods.delete("bare", &kube::api::DeleteParams::default().grace_period(0)),
    )
    .expect("Failed to delete the unmanaged Pod");
    let report = fixture
        .runtime
        .block_on(drain_node(fixture.client.clone(), &node_name, options))
        .expect("the drain lists the Node's Pods");
    assert!(report.blocked.is_empty() && report.evicted.is_empty());
    let [(pod, error)] = report.failed.as_slice() else {
        panic!("only the guarded Pod should fail: {report:?}");
    };
    assert_eq!(pod.name, "guarded");
    assert!(
        error.starts_with("Still refused after 3s"),
        "the budget's refusal is reported: {error}"
    );
    assert!(
        !is_deleting(&fixture, &pods, "guarded"),
        "the budget is never bypassed"
    );
}

/// A drain keeps asking for an eviction its PodDisruptionBudget refuses, and
/// evicts the Pod once the budget allows it.
#[test]
fn test_node_drain_evicts_once_the_disruption_budget_allows_it_integration() {
    let mut fixture = IntegrationNamespaceFixture::create("node-drain-retry", "anchor", "unused");
    let node_name = fixture.create_fake_node();
    let pods: Api<Pod> = Api::namespaced(fixture.client.clone(), &fixture.namespace);
    let budgets: Api<PodDisruptionBudget> =
        Api::namespaced(fixture.client.clone(), &fixture.namespace);
    create_guarded_pod(&fixture, &pods, &node_name, "guarded");
    let options = DrainOptions {
        eviction_timeout: Duration::from_secs(30),
        retry_interval: Duration::from_millis(250),
        concurrency: 1,
    };

    let report = fixture.runtime.block_on(async {
        let drain = drain_node(fixture.client.clone(), &node_name, options);
        let lift_budget = async {
            // Long enough for the drain to be refused at least once.
            tokio::time::sleep(Duration::from_secs(1)).await;
            budgets
                .delete("guarded", &Default::default())
                .await
                .expect("Failed to delete the PodDisruptionBudget");
        };
        let (report, ()) = tokio::join!(drain, lift_budget);
        report
    });

    let report = report.expect("the drain lists the Node's Pods");
    assert!(
        report.failed.is_empty() && report.blocked.is_empty(),
        "{report:?}"
    );
    assert_eq!(
        report
            .evicted
            .iter()
            .map(|pod| pod.name.as_str())
            .collect::<Vec<_>>(),
        ["guarded"]
    );
    assert!(is_deleting(&fixture, &pods, "guarded"));
}
