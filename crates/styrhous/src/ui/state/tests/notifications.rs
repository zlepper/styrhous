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

#[test]
fn partial_api_discovery_readies_the_cluster_and_reports_the_failed_groups() {
    let mut state = UiState::default();
    // Discovery runs concurrently with the connection handshake, so the cluster may
    // not be marked connected yet when its result arrives.
    state.clusters.insert(7, ClusterState::new(7, "dev".into()));
    let deployment = ApiResource {
        group: "apps".to_owned(),
        version: "v1".to_owned(),
        kind: "Deployment".to_owned(),
        name: "deployments".to_owned(),
        namespaced: true,
    };
    let mut commands = Vec::new();

    KubernetesApisLoaded {
        cluster_key: 7,
        api_resources: vec![deployment],
        scalable_api_resources: Default::default(),
        failed_api_groups: vec![
            FailedApiGroup {
                group_version: "metrics.k8s.io/v1beta1".to_owned(),
                error: "the server is currently unable to handle the request".to_owned(),
            },
            FailedApiGroup {
                group_version: "stuck.example.com/v1".to_owned(),
                error: "timed out waiting for the API server to respond".to_owned(),
            },
        ],
        pod_metrics_api_available: false,
        node_metrics_api_available: false,
    }
    .apply(&mut state, &mut commands);

    let cluster = &state.clusters[&7];
    assert!(matches!(
        cluster.api_resources_load,
        ClusterLoadState::Ready
    ));
    assert!(!cluster.pod_metrics_api_available);
    assert!(!cluster.node_metrics_api_available);
    assert!(!cluster.resource_navigation.curated_entries.is_empty());

    let toasts = state.visible_operation_toasts(Instant::now());
    assert_eq!(toasts.len(), 1);
    assert_eq!(toasts[0].outcome, OperationOutcome::Failure);
    assert_eq!(toasts[0].title, "2 API groups could not be loaded");
    assert_eq!(
        toasts[0].target,
        "metrics.k8s.io/v1beta1, stuck.example.com/v1"
    );

    let history = state.operation_history_entries();
    let details = history[0].entry.details.as_deref().expect("details");
    assert!(details.contains("metrics.k8s.io/v1beta1: the server is currently unable"));
    assert!(details.contains("stuck.example.com/v1: timed out"));
}

#[test]
fn complete_api_discovery_reports_no_failed_groups() {
    let mut state = UiState::default();
    state.clusters.insert(7, ClusterState::new(7, "dev".into()));
    let mut commands = Vec::new();

    KubernetesApisLoaded {
        cluster_key: 7,
        api_resources: Vec::new(),
        scalable_api_resources: Default::default(),
        failed_api_groups: Vec::new(),
        pod_metrics_api_available: false,
        node_metrics_api_available: false,
    }
    .apply(&mut state, &mut commands);

    assert!(state.visible_operation_toasts(Instant::now()).is_empty());
    assert!(state.operation_history_entries().is_empty());
}
