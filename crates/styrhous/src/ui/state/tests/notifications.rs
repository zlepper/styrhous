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

fn api_resource(group: &str, kind: &str, name: &str) -> ApiResource {
    ApiResource {
        group: group.to_owned(),
        version: "v1".to_owned(),
        kind: kind.to_owned(),
        name: name.to_owned(),
        namespaced: true,
    }
}

#[test]
fn recovered_api_groups_merge_into_the_navigation_without_changing_the_selection() {
    let mut state = UiState::default();
    state.clusters.insert(7, ClusterState::new(7, "dev".into()));
    let pods = api_resource("core", "Pod", "pods");
    let widgets = api_resource("example.com", "Widget", "widgets");
    let mut commands = Vec::new();
    KubernetesApisLoaded {
        cluster_key: 7,
        api_resources: vec![pods.clone()],
        scalable_api_resources: Default::default(),
        failed_api_groups: vec![
            FailedApiGroup {
                group_version: "example.com/v1".to_owned(),
                error: "503".to_owned(),
            },
            FailedApiGroup {
                group_version: "metrics.k8s.io/v1beta1".to_owned(),
                error: "503".to_owned(),
            },
        ],
        pod_metrics_api_available: false,
        node_metrics_api_available: false,
    }
    .apply(&mut state, &mut commands);
    state.clusters.get_mut(&7).unwrap().selected_api_resource = Some(pods.clone());

    KubernetesApiGroupsRecovered {
        cluster_key: 7,
        group_versions: vec!["example.com/v1".to_owned()],
        api_resources: vec![widgets.clone(), pods.clone()],
        scalable_api_resources: [widgets.clone()].into(),
        still_failing_group_versions: vec!["metrics.k8s.io/v1beta1".to_owned()],
        pod_metrics_api_available: false,
        node_metrics_api_available: false,
    }
    .apply(&mut state, &mut commands);

    let cluster = &state.clusters[&7];
    assert_eq!(cluster.selected_api_resource, Some(pods.clone()));
    assert_eq!(
        cluster.discovered_api_resources,
        vec![pods, widgets.clone()]
    );
    assert_eq!(
        cluster.resource_navigation.other_api_groups["example.com"],
        vec![widgets.clone()]
    );
    assert!(cluster.scalable_api_resources.contains(&widgets));
    assert!(!cluster.pod_metrics_api_available);

    let history = state.operation_history_entries();
    assert_eq!(history.len(), 2, "the failure stays in the history");
    let recovery = &history[0].entry;
    assert_eq!(recovery.outcome, OperationOutcome::Success);
    assert_eq!(recovery.title, "1 API group loaded");
    assert_eq!(recovery.target, "example.com/v1");
    let details = recovery.details.as_deref().expect("details");
    assert!(details.contains("Still retrying: metrics.k8s.io/v1beta1"));
}

#[test]
fn recovered_metrics_group_restores_metrics_availability_and_restarts_the_watch() {
    let mut state = UiState::default();
    state.clusters.insert(7, ClusterState::new(7, "dev".into()));
    let pods = api_resource("core", "Pod", "pods");
    let mut commands = Vec::new();
    KubernetesApisLoaded {
        cluster_key: 7,
        api_resources: vec![pods.clone()],
        scalable_api_resources: Default::default(),
        failed_api_groups: vec![FailedApiGroup {
            group_version: "metrics.k8s.io/v1beta1".to_owned(),
            error: "503".to_owned(),
        }],
        pod_metrics_api_available: false,
        node_metrics_api_available: false,
    }
    .apply(&mut state, &mut commands);
    {
        let cluster = state.clusters.get_mut(&7).unwrap();
        cluster.selected_api_resource = Some(pods);
        cluster.selected_namespaces.insert("default".to_owned());
    }
    commands.clear();

    KubernetesApiGroupsRecovered {
        cluster_key: 7,
        group_versions: vec!["metrics.k8s.io/v1beta1".to_owned()],
        api_resources: vec![
            api_resource("metrics.k8s.io", "PodMetrics", "pods"),
            api_resource("metrics.k8s.io", "NodeMetrics", "nodes"),
        ],
        scalable_api_resources: Default::default(),
        still_failing_group_versions: Vec::new(),
        pod_metrics_api_available: true,
        node_metrics_api_available: true,
    }
    .apply(&mut state, &mut commands);

    let cluster = &state.clusters[&7];
    assert!(cluster.pod_metrics_api_available);
    assert!(cluster.node_metrics_api_available);
    assert!(cluster.active_pod_metrics.contains("default"));
    assert_eq!(
        commands.len(),
        1,
        "the pod metrics watch starts for the selection"
    );
    let toasts = state.visible_operation_toasts(Instant::now());
    assert_eq!(toasts.last().unwrap().title, "1 API group loaded");
}

#[test]
fn recovered_api_groups_are_ignored_before_the_session_has_loaded() {
    let mut state = UiState::default();
    state.clusters.insert(7, ClusterState::new(7, "dev".into()));
    let mut commands = Vec::new();

    KubernetesApiGroupsRecovered {
        cluster_key: 7,
        group_versions: vec!["example.com/v1".to_owned()],
        api_resources: vec![api_resource("example.com", "Widget", "widgets")],
        scalable_api_resources: Default::default(),
        still_failing_group_versions: Vec::new(),
        pod_metrics_api_available: false,
        node_metrics_api_available: false,
    }
    .apply(&mut state, &mut commands);

    assert!(state.clusters[&7].discovered_api_resources.is_empty());
    assert!(state.operation_history_entries().is_empty());
}

fn failing_group(group_version: &str) -> FailedApiGroup {
    FailedApiGroup {
        group_version: group_version.to_owned(),
        error: "503".to_owned(),
    }
}

fn recovered(
    api_resources: Vec<ApiResource>,
    group_versions: &[&str],
    still_failing: &[&str],
) -> KubernetesApiGroupsRecovered {
    KubernetesApiGroupsRecovered {
        cluster_key: 7,
        group_versions: group_versions
            .iter()
            .map(|group| group.to_string())
            .collect(),
        api_resources,
        scalable_api_resources: Default::default(),
        still_failing_group_versions: still_failing
            .iter()
            .map(|group| group.to_string())
            .collect(),
        pod_metrics_api_available: false,
        node_metrics_api_available: false,
    }
}

#[test]
fn saved_selection_in_a_failed_group_is_kept_and_restored_when_the_group_recovers() {
    let mut state = UiState::default();
    let mut cluster = ClusterState::new(7, "dev".into());
    cluster.selected_namespaces.insert("default".to_owned());
    state.clusters.insert(7, cluster);
    state.cluster_selections.selections.insert(
        "dev".into(),
        PersistedClusterSelection {
            selected_namespaces: Default::default(),
            selected_api_resource: Some(PersistedApiResource {
                group: "example.com".into(),
                name: "widgets".into(),
            }),
        },
    );
    let widgets = api_resource("example.com", "Widget", "widgets");
    let mut commands = Vec::new();
    KubernetesApisLoaded {
        cluster_key: 7,
        api_resources: vec![api_resource("core", "Pod", "pods")],
        scalable_api_resources: Default::default(),
        failed_api_groups: vec![failing_group("example.com/v1")],
        pod_metrics_api_available: true,
        node_metrics_api_available: true,
    }
    .apply(&mut state, &mut commands);

    assert_eq!(state.clusters[&7].selected_api_resource, None);
    assert!(state.cluster_selections.selections.contains_key("dev"));
    commands.clear();

    recovered(vec![widgets.clone()], &["example.com/v1"], &[]).apply(&mut state, &mut commands);

    assert_eq!(state.clusters[&7].selected_api_resource, Some(widgets));
    assert!(commands.iter().any(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<ReconcileResourceWatches>()
            .is_some()
    }));
}

#[test]
fn recovery_does_not_override_a_manual_selection() {
    let mut state = UiState::default();
    state.clusters.insert(7, ClusterState::new(7, "dev".into()));
    state.cluster_selections.selections.insert(
        "dev".into(),
        PersistedClusterSelection {
            selected_namespaces: Default::default(),
            selected_api_resource: Some(PersistedApiResource {
                group: "example.com".into(),
                name: "widgets".into(),
            }),
        },
    );
    let pods = api_resource("core", "Pod", "pods");
    let mut commands = Vec::new();
    KubernetesApisLoaded {
        cluster_key: 7,
        api_resources: vec![pods.clone()],
        scalable_api_resources: Default::default(),
        failed_api_groups: vec![failing_group("example.com/v1")],
        pod_metrics_api_available: true,
        node_metrics_api_available: true,
    }
    .apply(&mut state, &mut commands);
    state.select_api_resource(7, pods.clone(), &mut commands);

    recovered(
        vec![api_resource("example.com", "Widget", "widgets")],
        &["example.com/v1"],
        &[],
    )
    .apply(&mut state, &mut commands);

    assert_eq!(state.clusters[&7].selected_api_resource, Some(pods));
}

#[test]
fn recovered_metrics_restart_the_watches_of_open_inspectors() {
    let mut state = UiState::default();
    state.clusters.insert(7, ClusterState::new(7, "dev".into()));
    let pods = api_resource("core", "Pod", "pods");
    let mut commands = Vec::new();
    KubernetesApisLoaded {
        cluster_key: 7,
        api_resources: vec![pods.clone()],
        scalable_api_resources: Default::default(),
        failed_api_groups: vec![failing_group("metrics.k8s.io/v1beta1")],
        pod_metrics_api_available: false,
        node_metrics_api_available: false,
    }
    .apply(&mut state, &mut commands);
    state.open_resource_detail(
        7,
        pods,
        "api".to_owned(),
        Some("default".to_owned()),
        "uid".to_owned(),
        &mut commands,
    );
    commands.clear();

    let mut recovery = recovered(
        vec![api_resource("metrics.k8s.io", "PodMetrics", "pods")],
        &["metrics.k8s.io/v1beta1"],
        &[],
    );
    recovery.pod_metrics_api_available = true;
    recovery.apply(&mut state, &mut commands);

    let entry = state
        .global_blades
        .navigator()
        .unwrap()
        .entries()
        .find_map(|entry| entry.resource_detail())
        .unwrap();
    assert!(!entry.pod_metrics_api_unavailable);
    let history_entry_id = entry.history_entry_id;
    assert!(commands.iter().any(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<StopResourceDetailWatch>()
            .is_some_and(|stop| stop.history_entry_id == history_entry_id)
    }));
    assert!(commands.iter().any(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<StartResourceDetailWatch>()
            .is_some_and(|start| {
                start.history_entry_id == history_entry_id && start.pod_metrics_api_available
            })
    }));
}

#[test]
fn recovered_api_groups_are_ignored_after_the_session_failed_to_load() {
    let mut state = UiState::default();
    let mut cluster = ClusterState::new(7, "dev".into());
    cluster.api_resources_load = ClusterLoadState::Failed("boom".into());
    state.clusters.insert(7, cluster);
    let mut commands = Vec::new();

    recovered(
        vec![api_resource("example.com", "Widget", "widgets")],
        &["example.com/v1"],
        &[],
    )
    .apply(&mut state, &mut commands);

    assert!(state.clusters[&7].discovered_api_resources.is_empty());
    assert!(state.operation_history_entries().is_empty());
}
