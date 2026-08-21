//! UI interaction scenarios that assert state and accessibility rather than pixels.

use super::*;

#[test]
fn resource_navigation_uses_the_persisted_expansion_state() {
    let mut state = oracle_resource_table_state();
    state.set_resource_navigation_node_expanded("section:Apps & Containers", true);
    let harness = application_harness_with_state(state);

    harness.get_by_label("Pods");
}

#[test]
fn no_current_context_leaves_cluster_selection_manual() {
    let mut harness = application_harness::<MockWorker>();
    harness.run();
    harness.deliver_worker_result(KubernetesClustersUpdated(vec![Cluster {
        name: "dev".into(),
        is_current: false,
    }]));

    assert_eq!(harness.state().ui_state.selected_cluster, None);
    assert!(harness.state().worker.commands.is_empty());
}
