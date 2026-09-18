use super::*;

#[test]
fn transient_operation_toasts_show_the_newest_three_globally_and_expire() {
    let mut state = UiState::default();
    let mut cluster = ClusterState::for_test(7, "dev");
    cluster.connection = ClusterConnectionState::Connected;
    state.clusters.insert(7, cluster);
    let mut second_cluster = ClusterState::for_test(8, "prod");
    second_cluster.connection = ClusterConnectionState::Connected;
    state.clusters.insert(8, second_cluster);

    for (cluster_key, title) in [(7, "first"), (8, "second"), (7, "third"), (8, "fourth")] {
        state.record_operation_outcome(
            cluster_key,
            OperationOutcome::Success,
            title,
            None,
            "api",
            None,
        );
    }

    let now = Instant::now();
    assert_eq!(
        state
            .visible_operation_toasts(now)
            .iter()
            .map(|toast| toast.title.as_str())
            .collect::<Vec<_>>(),
        vec!["second", "third", "fourth"]
    );

    for cluster in state.clusters.values_mut() {
        for toast in &mut cluster.transient_operation_toasts {
            toast.expires_at = now - Duration::from_secs(1);
        }
    }
    assert!(state.visible_operation_toasts(now).is_empty());
    assert!(
        state
            .clusters
            .values()
            .all(|cluster| cluster.transient_operation_toasts.is_empty())
    );
    assert_eq!(state.unread_operation_count(), 4);
    assert_eq!(state.operation_history_entries().len(), 4);
}
