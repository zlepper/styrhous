use super::*;

#[test]
fn selecting_context_remembers_the_latest_user_selection() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![
        Cluster {
            name: "dev".to_owned(),
            is_current: false,
        },
        Cluster {
            name: "prod".to_owned(),
            is_current: false,
        },
    ])
    .apply(&mut state, &mut commands);

    assert!(state.select_cluster(1).is_some());
    assert_eq!(
        state.cluster_selections.last_selected_context.as_deref(),
        Some("dev")
    );

    state.clusters.get_mut(&2).unwrap().connection = ClusterConnectionState::Connected;
    assert!(state.select_cluster(2).is_none());
    assert_eq!(
        state.cluster_selections.last_selected_context.as_deref(),
        Some("prod")
    );
}

#[test]
fn reconnect_reset_clears_connection_derived_resource_state() {
    let mut cluster = ClusterState::new(7, "dev".to_owned());
    let resource = ApiResource::helm_releases();
    cluster.selected_namespaces.insert("default".to_owned());
    cluster.selected_api_resource = Some(resource.clone());
    cluster.active_watchers.insert((resource.clone(), None));
    cluster.resource_cache.insert(
        (resource.clone(), None),
        ResourceWatchState {
            is_synced: true,
            ..Default::default()
        },
    );
    cluster.pod_metrics_api_available = true;
    cluster.node_metrics_api_available = true;
    cluster
        .resource_searches
        .insert(resource.clone(), ResourceSearchState::default());
    cluster
        .resource_selections
        .insert(resource, HashSet::from(["uid".to_owned()]));
    cluster.resource_detail_panel = Some(ResourceDetailPanelState {
        dismiss_on_outside_click: true,
    });
    cluster.next_detail_generation = 4;
    cluster.pending_delete = Some(PendingDelete {
        api_resource: ApiResource::helm_releases(),
        resource_name: "demo".to_owned(),
        namespace: Some("default".to_owned()),
        confirmation_available_at: Instant::now(),
    });
    cluster.next_bulk_delete_id = 3;

    cluster.reset_for_connection();

    assert!(matches!(
        cluster.connection,
        ClusterConnectionState::Connecting
    ));
    assert!(cluster.selected_namespaces.is_empty());
    assert!(cluster.selected_api_resource.is_none());
    assert!(cluster.active_watchers.is_empty());
    assert!(cluster.resource_cache.is_empty());
    assert!(!cluster.pod_metrics_api_available);
    assert!(!cluster.node_metrics_api_available);
    assert!(cluster.resource_searches.is_empty());
    assert!(cluster.resource_selections.is_empty());
    assert!(cluster.resource_detail_panel.is_none());
    assert_eq!(cluster.next_detail_generation, 0);
    assert!(cluster.pending_delete.is_none());
    assert_eq!(cluster.next_bulk_delete_id, 0);
}

#[test]
fn remembered_context_is_preferred_over_current_context_at_startup() {
    let mut state = UiState::default();
    state.cluster_selections.last_selected_context = Some("prod".to_owned());
    let mut commands = Vec::new();

    KubernetesClustersUpdated(vec![
        Cluster {
            name: "dev".to_owned(),
            is_current: true,
        },
        Cluster {
            name: "prod".to_owned(),
            is_current: false,
        },
    ])
    .apply(&mut state, &mut commands);

    assert_eq!(state.selected_cluster, Some(2));
    assert!(matches!(
        commands.as_slice(),
        [command] if command
            .as_ref()
            .as_any()
            .downcast_ref::<ConnectToCluster>()
            .is_some_and(|command| command.cluster_key == 2 && command.cluster == "prod")
    ));
}

#[test]
fn managed_cluster_import_adds_kubeconfig_context_without_disrupting_a_cluster() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.managed_cluster_discovery.importing = Some(ManagedClusterImport::Tailscale {
        host_name: "api-proxy".into(),
    });

    ManagedClusterImported.apply(&mut state, &mut commands);

    assert!(state.managed_cluster_discovery.importing.is_none());
    assert!(state.managed_cluster_discovery.loading);
    assert!(state.managed_cluster_discovery.error.is_none());
    assert!(matches!(
        commands.as_slice(),
        [reload] if reload.as_ref().as_any().is::<LoadImportedClusters>()
    ));

    commands.clear();
    let existing_cluster = Cluster {
        name: "existing".into(),
        is_current: true,
    };
    KubernetesClustersUpdated(vec![existing_cluster.clone()]).apply(&mut state, &mut commands);
    commands.clear();
    let existing_key = state
        .selected_cluster
        .expect("existing cluster is selected");
    ImportedKubernetesClusters(vec![
        existing_cluster,
        Cluster {
            name: "imported".into(),
            is_current: true,
        },
    ])
    .apply(&mut state, &mut commands);

    assert_eq!(state.selected_cluster, Some(existing_key));
    assert_eq!(state.clusters.len(), 2);
    assert!(
        state
            .clusters
            .values()
            .any(|cluster| cluster.name == "imported")
    );
    assert!(matches!(
        commands.as_slice(),
        [refresh] if refresh.as_ref().as_any().is::<LoadManagedClusterDiscovery>()
    ));
}

#[test]
fn managed_cluster_discovery_failure_clears_pending_import() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.managed_cluster_discovery.loading = true;
    state.managed_cluster_discovery.importing = Some(ManagedClusterImport::Tailscale {
        host_name: "api-proxy".into(),
    });

    ManagedClusterDiscoveryFailed {
        error: "Tailscale is not logged in".into(),
    }
    .apply(&mut state, &mut commands);

    assert!(state.managed_cluster_discovery.importing.is_none());
    assert_eq!(
        state.managed_cluster_discovery.error.as_deref(),
        Some("Tailscale is not logged in")
    );
    assert!(!state.managed_cluster_discovery.loading);
    assert!(commands.is_empty());
}

#[test]
fn missing_remembered_context_falls_back_without_overwriting_the_preference() {
    let mut state = UiState::default();
    state.cluster_selections.last_selected_context = Some("temporarily-missing".to_owned());
    let mut commands = Vec::new();

    KubernetesClustersUpdated(vec![Cluster {
        name: "dev".to_owned(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);

    assert_eq!(state.selected_cluster, Some(1));
    assert_eq!(
        state.cluster_selections.last_selected_context.as_deref(),
        Some("temporarily-missing")
    );
    assert!(commands.iter().any(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<ConnectToCluster>()
            .is_some_and(|command| command.cluster_key == 1 && command.cluster == "dev")
    }));

    state.clusters.get_mut(&1).unwrap().connection =
        ClusterConnectionState::Failed("unavailable".to_owned());
    commands.clear();
    state.retry_selected_load(1, &mut commands);

    assert_eq!(
        state.cluster_selections.last_selected_context.as_deref(),
        Some("temporarily-missing")
    );
    assert!(commands.iter().any(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<ConnectToCluster>()
            .is_some_and(|command| command.cluster_key == 1 && command.cluster == "dev")
    }));
}

#[test]
fn missing_remembered_context_without_a_current_context_leaves_selection_manual() {
    let mut state = UiState::default();
    state.cluster_selections.last_selected_context = Some("temporarily-missing".to_owned());
    let mut commands = Vec::new();

    KubernetesClustersUpdated(vec![Cluster {
        name: "dev".to_owned(),
        is_current: false,
    }])
    .apply(&mut state, &mut commands);

    assert_eq!(state.selected_cluster, None);
    assert!(commands.is_empty());
    assert_eq!(
        state.cluster_selections.last_selected_context.as_deref(),
        Some("temporarily-missing")
    );
}
