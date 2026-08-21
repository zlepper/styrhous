use super::*;

#[test]
fn pod_usage_history_replaces_duplicates_and_prunes_against_the_current_time() {
    let now = time::OffsetDateTime::now_utc();
    let mut entry = pod_detail_history_entry();
    entry.pod_usage_history = vec![
        pod_usage(
            now - POD_USAGE_HISTORY_WINDOW - time::Duration::seconds(1),
            1,
        ),
        pod_usage(now - POD_USAGE_HISTORY_WINDOW, 2),
    ];
    entry.prune_pod_usage_history(now);
    assert_eq!(entry.pod_usage_history.len(), 1);
    assert_eq!(entry.pod_usage_history[0].cpu_nanocores, 2);

    entry.pod_usage_history.clear();
    let timestamp = now - time::Duration::seconds(1);
    entry.record_pod_usage(pod_usage(timestamp, 3));
    entry.pod_usage_error = Some("temporary outage".to_owned());
    entry.record_pod_usage(pod_usage(timestamp, 4));

    assert_eq!(entry.pod_usage_history.len(), 1);
    assert_eq!(entry.pod_usage_history[0].cpu_nanocores, 4);
    assert_eq!(
        entry.pod_usage.as_ref().map(|usage| usage.cpu_nanocores),
        Some(4)
    );
    assert!(entry.pod_usage_error.is_none());
}

#[test]
fn node_usage_history_replaces_duplicates_and_prunes_against_the_current_time() {
    let now = time::OffsetDateTime::now_utc();
    let mut entry = pod_detail_history_entry();
    entry.node_usage_history = vec![NodeUsage {
        timestamp: now - POD_USAGE_HISTORY_WINDOW - time::Duration::seconds(1),
        cpu_nanocores: 1,
        memory_bytes: 1,
    }];
    entry.prune_node_usage_history(now);
    assert!(entry.node_usage_history.is_empty());

    let timestamp = now - time::Duration::seconds(1);
    entry.record_node_usage(NodeUsage {
        timestamp,
        cpu_nanocores: 2,
        memory_bytes: 2,
    });
    entry.node_usage_error = Some("temporary outage".to_owned());
    entry.record_node_usage(NodeUsage {
        timestamp,
        cpu_nanocores: 3,
        memory_bytes: 3,
    });

    assert_eq!(entry.node_usage_history.len(), 1);
    assert_eq!(entry.node_usage_history[0].cpu_nanocores, 3);
    assert_eq!(
        entry.node_usage.as_ref().map(|usage| usage.cpu_nanocores),
        Some(3)
    );
    assert!(entry.node_usage_error.is_none());
}

#[test]
fn pod_metrics_watches_follow_pod_selection_and_namespace_scope() {
    let pod = ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: "Pod".to_owned(),
        name: "pods".to_owned(),
        namespaced: true,
    };
    let config_map = ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: "ConfigMap".to_owned(),
        name: "configmaps".to_owned(),
        namespaced: true,
    };
    let mut state = UiState::default();
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![Cluster {
        name: "kind".to_owned(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);
    state
        .clusters
        .get_mut(&1)
        .unwrap()
        .pod_metrics_api_available = true;

    state.select_api_resource(1, pod.clone(), &mut commands);
    commands.clear();
    state.toggle_namespace(1, "default".to_owned(), &mut commands);
    assert_eq!(
        commands
            .iter()
            .filter(|command| {
                command
                    .as_ref()
                    .as_any()
                    .downcast_ref::<StartPodMetricsWatch>()
                    .is_some()
            })
            .count(),
        1
    );

    commands.clear();
    state.select_api_resource(1, pod, &mut commands);
    assert!(commands.iter().all(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<StartPodMetricsWatch>()
            .is_none()
    }));

    commands.clear();
    state.select_api_resource(1, config_map, &mut commands);
    assert_eq!(
        commands
            .iter()
            .filter(|command| {
                command
                    .as_ref()
                    .as_any()
                    .downcast_ref::<StopPodMetricsWatch>()
                    .is_some()
            })
            .count(),
        1
    );
}

#[test]
fn helm_release_selection_watches_each_selected_namespace() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![Cluster {
        name: "kind".to_owned(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);
    state
        .clusters
        .get_mut(&1)
        .unwrap()
        .selected_namespaces
        .insert("apps".to_owned());

    state.select_api_resource(1, ApiResource::helm_releases(), &mut commands);

    assert!(commands.iter().any(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<ReconcileResourceWatches>()
            .is_some_and(|command| {
                command.api_resource.is_helm_releases()
                    && matches!(
                        command.sources.as_slice(),
                        [ResourceWatchSource::Namespace(namespace)] if namespace == "apps"
                    )
            })
    }));
}

#[test]
fn helm_release_watch_start_failure_unblocks_the_namespace_with_an_error() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![Cluster {
        name: "kind".to_owned(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);

    KubernetesResourceWatchFailed {
        cluster_key: 1,
        api_resource: ApiResource::helm_releases(),
        namespace: Some("apps".to_owned()),
        error: "forbidden".to_owned(),
    }
    .apply(&mut state, &mut commands);

    let watch = &state.clusters[&1].helm_release_cache["apps"];
    assert!(watch.is_synced);
    assert_eq!(
        watch.backend_errors.get("Helm storage"),
        Some(&"forbidden".to_owned())
    );
}

#[test]
fn node_metrics_watch_follows_node_selection_and_stops_independently() {
    let node = ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: "Node".to_owned(),
        name: "nodes".to_owned(),
        namespaced: false,
    };
    let config_map = ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: "ConfigMap".to_owned(),
        name: "configmaps".to_owned(),
        namespaced: true,
    };
    let mut state = UiState::default();
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![Cluster {
        name: "kind".to_owned(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);
    state
        .clusters
        .get_mut(&1)
        .unwrap()
        .node_metrics_api_available = true;

    state.select_api_resource(1, node, &mut commands);
    assert!(state.clusters[&1].node_metrics_active);
    assert!(
        commands
            .iter()
            .any(|command| command.as_ref().as_any().is::<StartNodeMetricsWatch>())
    );

    commands.clear();
    state.select_api_resource(1, config_map, &mut commands);
    assert!(!state.clusters[&1].node_metrics_active);
    assert!(
        commands
            .iter()
            .any(|command| command.as_ref().as_any().is::<StopNodeMetricsWatch>())
    );
}

#[test]
fn unavailable_metrics_api_from_discovery_does_not_start_metrics_watches() {
    let pod = ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: "Pod".to_owned(),
        name: "pods".to_owned(),
        namespaced: true,
    };
    let mut state = UiState::default();
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![Cluster {
        name: "kind".to_owned(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);

    state.select_api_resource(1, pod.clone(), &mut commands);
    commands.clear();
    state.toggle_namespace(1, "default".to_owned(), &mut commands);
    assert!(commands.iter().all(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<StartPodMetricsWatch>()
            .is_none()
    }));

    commands.clear();
    state.open_resource_detail(
        1,
        pod,
        "api".to_owned(),
        Some("default".to_owned()),
        "uid".to_owned(),
        &mut commands,
    );
    assert_eq!(
        commands.iter().find_map(|command| {
            command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .map(|command| command.pod_metrics_api_available)
        }),
        Some(false)
    );
}

#[test]
fn unavailable_metrics_api_stops_namespace_watches_and_marks_pod_details() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![Cluster {
        name: "kind".to_owned(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);
    let cluster = state.clusters.get_mut(&1).unwrap();
    cluster.pod_metrics_api_available = true;
    cluster.active_pod_metrics.insert("default".into());
    cluster
        .pod_metrics
        .insert("default".into(), Default::default());
    state.replace_global_blade(Box::new(pod_detail_history_entry()), &mut commands);
    commands.clear();

    PodMetricsApiUnavailable { cluster_key: 1 }.apply(&mut state, &mut commands);

    assert!(!state.clusters[&1].pod_metrics_api_available);
    assert!(state.clusters[&1].pod_metrics.is_empty());
    assert!(
        state
            .resource_detail_entry_mut(1)
            .unwrap()
            .pod_metrics_api_unavailable
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| command.as_ref().as_any().is::<StopPodMetricsWatch>())
            .count(),
        1
    );
}

#[test]
fn unavailable_node_metrics_do_not_disable_pod_metrics() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![Cluster {
        name: "kind".to_owned(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);
    let cluster = state.clusters.get_mut(&1).unwrap();
    cluster.pod_metrics_api_available = true;
    cluster.node_metrics_api_available = true;
    cluster.node_metrics_active = true;
    cluster.node_metrics.usages.insert(
        "worker-a".into(),
        NodeUsage {
            timestamp: time::OffsetDateTime::now_utc(),
            cpu_nanocores: 1,
            memory_bytes: 1,
        },
    );
    commands.clear();

    NodeMetricsApiUnavailable { cluster_key: 1 }.apply(&mut state, &mut commands);

    assert!(state.clusters[&1].pod_metrics_api_available);
    assert!(!state.clusters[&1].node_metrics_api_available);
    assert!(state.clusters[&1].node_metrics.usages.is_empty());
    assert!(
        commands
            .iter()
            .any(|command| command.as_ref().as_any().is::<StopNodeMetricsWatch>())
    );
}
