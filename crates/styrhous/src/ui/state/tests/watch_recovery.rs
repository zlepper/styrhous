use super::*;
use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::Cluster;

fn pods() -> ApiResource {
    ApiResource {
        group: "core".to_owned(),
        version: "v1".to_owned(),
        kind: "Pod".to_owned(),
        name: "pods".to_owned(),
        namespaced: true,
    }
}

fn connected_state() -> UiState {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![Cluster {
        name: "kind".to_owned(),
        is_current: true,
        presentation: Default::default(),
    }])
    .apply(&mut state, &mut commands);
    state
}

#[test]
fn resource_reconnecting_keeps_rows_and_the_active_watcher_until_recovery() {
    let mut state = connected_state();
    let mut commands = Vec::new();
    let key = (pods(), Some("apps".to_owned()));
    let cluster = state.clusters.get_mut(&1).unwrap();
    cluster.active_watchers.insert(key.clone());
    cluster.resource_cache.insert(
        key.clone(),
        ResourceWatchState {
            is_synced: true,
            ..Default::default()
        },
    );

    KubernetesResourceWatchReconnecting {
        cluster_key: 1,
        api_resource: pods(),
        namespace: Some("apps".to_owned()),
        error: "connection lost".to_owned(),
    }
    .apply(&mut state, &mut commands);

    let cluster = &state.clusters[&1];
    assert!(cluster.active_watchers.contains(&key));
    assert!(cluster.resource_cache[&key].is_synced);
    assert_eq!(
        cluster.resource_cache[&key].error.as_deref(),
        Some("connection lost")
    );

    KubernetesResourceWatchRecovered {
        cluster_key: 1,
        api_resource: pods(),
        namespace: Some("apps".to_owned()),
    }
    .apply(&mut state, &mut commands);
    assert_eq!(state.clusters[&1].resource_cache[&key].error, None);
}

#[test]
fn all_namespace_reconnecting_marks_every_selected_namespace() {
    let mut state = connected_state();
    let mut commands = Vec::new();
    let cluster = state.clusters.get_mut(&1).unwrap();
    cluster.selected_namespaces = ["a".to_owned(), "b".to_owned()].into();
    cluster.active_watchers.insert((pods(), None));

    KubernetesResourceWatchReconnecting {
        cluster_key: 1,
        api_resource: pods(),
        namespace: None,
        error: "lost".to_owned(),
    }
    .apply(&mut state, &mut commands);
    for namespace in ["a", "b"] {
        assert!(
            state.clusters[&1].resource_cache[&(pods(), Some(namespace.to_owned()))]
                .error
                .is_some()
        );
    }

    KubernetesResourceWatchRecovered {
        cluster_key: 1,
        api_resource: pods(),
        namespace: None,
    }
    .apply(&mut state, &mut commands);
    for namespace in ["a", "b"] {
        assert!(
            state.clusters[&1].resource_cache[&(pods(), Some(namespace.to_owned()))]
                .error
                .is_none()
        );
    }
}

#[test]
fn namespace_reconnecting_keeps_a_loaded_list_usable_and_clears_on_recovery() {
    let mut state = connected_state();
    let mut commands = Vec::new();
    state.clusters.get_mut(&1).unwrap().namespaces_load = ClusterLoadState::Ready;

    KubernetesNamespacesReconnecting {
        cluster_key: 1,
        error: "lost".to_owned(),
    }
    .apply(&mut state, &mut commands);
    let cluster = &state.clusters[&1];
    assert!(matches!(cluster.namespaces_load, ClusterLoadState::Ready));
    assert_eq!(cluster.namespaces_reconnecting.as_deref(), Some("lost"));

    KubernetesNamespacesRecovered { cluster_key: 1 }.apply(&mut state, &mut commands);
    assert_eq!(state.clusters[&1].namespaces_reconnecting, None);
}

#[test]
fn namespace_reconnecting_before_the_first_list_surfaces_a_load_failure() {
    let mut state = connected_state();
    let mut commands = Vec::new();

    KubernetesNamespacesReconnecting {
        cluster_key: 1,
        error: "lost".to_owned(),
    }
    .apply(&mut state, &mut commands);
    assert!(matches!(
        &state.clusters[&1].namespaces_load,
        ClusterLoadState::Failed(error) if error == "lost"
    ));

    KubernetesNamespacesReplaced {
        cluster_key: 1,
        namespaces: Vec::new(),
    }
    .apply(&mut state, &mut commands);
    assert!(matches!(
        state.clusters[&1].namespaces_load,
        ClusterLoadState::Ready
    ));
}

#[test]
fn helm_backend_recovery_clears_only_that_backend() {
    let mut state = connected_state();
    let mut commands = Vec::new();
    for (backend, error) in [("Secrets", "a"), ("ConfigMaps", "b")] {
        HelmReleaseBackendFailed {
            cluster_key: 1,
            namespace: "apps".to_owned(),
            backend,
            error: error.to_owned(),
        }
        .apply(&mut state, &mut commands);
    }

    HelmReleaseBackendRecovered {
        cluster_key: 1,
        namespace: "apps".to_owned(),
        backend: "Secrets",
    }
    .apply(&mut state, &mut commands);

    let errors = &state.clusters[&1].helm_release_cache["apps"].backend_errors;
    assert!(!errors.contains_key("Secrets"));
    assert!(errors.contains_key("ConfigMaps"));
}

#[test]
fn late_resource_watch_results_for_inactive_watchers_do_nothing() {
    let mut state = connected_state();
    let mut commands = Vec::new();

    KubernetesResourceWatchReconnecting {
        cluster_key: 1,
        api_resource: pods(),
        namespace: Some("apps".to_owned()),
        error: "lost".to_owned(),
    }
    .apply(&mut state, &mut commands);
    KubernetesResourceWatchRecovered {
        cluster_key: 1,
        api_resource: pods(),
        namespace: Some("apps".to_owned()),
    }
    .apply(&mut state, &mut commands);

    assert!(state.clusters[&1].resource_cache.is_empty());
}

#[test]
fn reconnecting_before_the_first_list_creates_the_entry_for_an_active_watcher() {
    let mut state = connected_state();
    let mut commands = Vec::new();
    let key = (pods(), Some("apps".to_owned()));
    state
        .clusters
        .get_mut(&1)
        .unwrap()
        .active_watchers
        .insert(key.clone());

    KubernetesResourceWatchReconnecting {
        cluster_key: 1,
        api_resource: pods(),
        namespace: Some("apps".to_owned()),
        error: "lost".to_owned(),
    }
    .apply(&mut state, &mut commands);

    let watch = &state.clusters[&1].resource_cache[&key];
    assert_eq!(watch.error.as_deref(), Some("lost"));
    assert!(!watch.is_synced);
}

#[test]
fn results_for_an_unknown_cluster_are_ignored() {
    let mut state = connected_state();
    let mut commands = Vec::new();

    KubernetesResourceWatchReconnecting {
        cluster_key: 99,
        api_resource: pods(),
        namespace: None,
        error: "lost".to_owned(),
    }
    .apply(&mut state, &mut commands);
    KubernetesResourceWatchRecovered {
        cluster_key: 99,
        api_resource: pods(),
        namespace: None,
    }
    .apply(&mut state, &mut commands);
    KubernetesNamespacesReconnecting {
        cluster_key: 99,
        error: "lost".to_owned(),
    }
    .apply(&mut state, &mut commands);
    KubernetesNamespacesRecovered { cluster_key: 99 }.apply(&mut state, &mut commands);
    HelmReleaseBackendRecovered {
        cluster_key: 99,
        namespace: "apps".to_owned(),
        backend: "Secrets",
    }
    .apply(&mut state, &mut commands);
    ResourceDetailWatchRecovered {
        cluster_key: 99,
        history_entry_id: 1,
        stream: DetailStream::Object,
    }
    .apply(&mut state, &mut commands);
    ManagedResourcesWatchRecovered {
        cluster_key: 99,
        history_entry_id: 1,
    }
    .apply(&mut state, &mut commands);

    assert_eq!(state.clusters.len(), 1);
    let cluster = &state.clusters[&1];
    assert!(cluster.resource_cache.is_empty());
    assert!(cluster.helm_release_cache.is_empty());
    assert_eq!(cluster.namespaces_reconnecting, None);
}

fn state_with_failed_detail_entry() -> UiState {
    let mut state = connected_state();
    let mut commands = Vec::new();
    let mut entry = pod_detail_history_entry();
    entry.detail_error = Some("object".to_owned());
    entry.events_error = Some("events".to_owned());
    entry.managed_resources_error = Some("managed".to_owned());
    state.replace_global_blade(Box::new(entry), &mut commands);
    state
}

#[test]
fn detail_recovery_clears_only_the_matching_stream() {
    let mut state = state_with_failed_detail_entry();
    let mut commands = Vec::new();

    ResourceDetailWatchRecovered {
        cluster_key: 1,
        history_entry_id: 1,
        stream: DetailStream::Events,
    }
    .apply(&mut state, &mut commands);
    let entry = state.resource_detail_entry_mut(1).unwrap();
    assert_eq!(entry.events_error, None);
    assert_eq!(entry.detail_error.as_deref(), Some("object"));
    assert_eq!(entry.managed_resources_error.as_deref(), Some("managed"));

    ResourceDetailWatchRecovered {
        cluster_key: 1,
        history_entry_id: 1,
        stream: DetailStream::Object,
    }
    .apply(&mut state, &mut commands);
    let entry = state.resource_detail_entry_mut(1).unwrap();
    assert_eq!(entry.detail_error, None);
    assert_eq!(entry.managed_resources_error.as_deref(), Some("managed"));
}

#[test]
fn only_managed_recovery_clears_the_managed_error() {
    let mut state = state_with_failed_detail_entry();
    let mut commands = Vec::new();

    ManagedResourcesReplaced {
        cluster_key: 1,
        history_entry_id: 1,
        resources: Vec::new(),
        node_pod_resources: None,
    }
    .apply(&mut state, &mut commands);
    assert_eq!(
        state
            .resource_detail_entry_mut(1)
            .unwrap()
            .managed_resources_error
            .as_deref(),
        Some("managed"),
        "another kind may still be degraded"
    );

    ManagedResourcesWatchRecovered {
        cluster_key: 1,
        history_entry_id: 1,
    }
    .apply(&mut state, &mut commands);
    assert_eq!(
        state
            .resource_detail_entry_mut(1)
            .unwrap()
            .managed_resources_error,
        None
    );
}
