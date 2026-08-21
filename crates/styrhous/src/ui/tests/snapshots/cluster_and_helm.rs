//! Cluster connection and Helm UI scenarios.

use super::*;

#[test]
fn namespace_selector_reconciles_watch_sources_when_selection_changes() {
    let pods = fixture_api_resource("", "Pod", "pods");
    let mut cluster = fixture_cluster(1, "dev");
    cluster.connection = ClusterConnectionState::Connected;
    cluster.resource_navigation = build_resource_navigation(vec![pods.clone()]);
    for namespace in ["default", "kube-system", "monitoring"] {
        cluster.namespaces.insert(
            namespace.into(),
            MinimalNamespace {
                name: namespace.into(),
                display_name: None,
            },
        );
    }

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    };
    harness.run_steps(1);

    select_namespace(&mut harness, "default");
    assert_eq!(
        harness.state().ui_state.clusters[&1].selected_namespaces,
        HashSet::from(["default".to_owned()])
    );

    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run_steps(1);
    let namespace_position = harness.get_by_label("kube-system").rect().center();
    let modifiers = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    harness.event(egui::Event::ModifiersChanged(modifiers));
    harness.event(egui::Event::PointerMoved(namespace_position));
    harness.event(egui::Event::PointerButton {
        pos: namespace_position,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    harness.event(egui::Event::PointerButton {
        pos: namespace_position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.run();
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::default()));
    assert_eq!(
        harness.state().ui_state.clusters[&1].selected_namespaces,
        HashSet::from(["default".to_owned(), "kube-system".to_owned()])
    );

    harness.get_by_label("default").click();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1].selected_namespaces,
        HashSet::from(["default".to_owned()])
    );

    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run();

    harness.get_by_label("Select all").click();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1].selected_namespaces,
        HashSet::from([
            "default".to_owned(),
            "kube-system".to_owned(),
            "monitoring".to_owned(),
        ])
    );

    harness.get_by_label("Select all").click();
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&1]
            .selected_namespaces
            .is_empty()
    );

    harness.get_by_label("Select all").click();
    harness.run();

    let mut commands = Vec::new();
    harness
        .state_mut()
        .ui_state
        .select_api_resource(1, pods.clone(), &mut commands);
    harness.state_mut().ui_state.replace_selected_namespaces(
        1,
        ["default".to_owned()],
        &mut commands,
    );
    let cluster = &harness.state().ui_state.clusters[&1];
    assert_eq!(
        cluster.selected_namespaces,
        HashSet::from(["default".to_owned()])
    );
    assert!(
        !cluster
            .active_watchers
            .contains(&(pods.clone(), Some("kube-system".to_owned())))
    );
    assert!(
        !cluster
            .active_watchers
            .contains(&(pods.clone(), Some("monitoring".to_owned())))
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| command_is::<ReconcileResourceWatches>(command).is_some())
            .count(),
        2
    );
}

#[test]
fn cluster_scoped_resources_load_once_without_a_namespace_selection() {
    let nodes = fixture_cluster_scoped_api_resource("core", "Node", "nodes");
    let mut cluster = fixture_cluster(1, "dev");
    cluster.connection = ClusterConnectionState::Connected;
    cluster.resource_navigation = build_resource_navigation(vec![nodes.clone()]);

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    };
    harness.run();
    harness.get_by_label("Nodes").click_accesskit();
    harness.run_steps(1);

    assert!(
        harness.state().ui_state.clusters[&1]
            .selected_namespaces
            .is_empty()
    );
    assert_eq!(harness.state().worker.commands.len(), 2);
    assert!(
        harness.state().worker.commands[0]
            .as_ref()
            .as_any()
            .downcast_ref::<ReconcileResourceWatches>()
            .is_some_and(|command| command.cluster_key == 1
                && command.api_resource == nodes
                && matches!(command.sources.as_slice(), [ResourceWatchSource::Cluster]))
    );
    assert!(harness.state().worker.commands.iter().any(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<StartNodeMetricsWatch>()
            .is_some_and(|command| command.cluster_key == 1)
    }));
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesResourcesReplaced {
            cluster_key: 1,
            api_resource: nodes.clone(),
            namespace: None,
            resources: vec![MinimalResource {
                uid: "node-uid".into(),
                name: "kind-control-plane".into(),
                namespace: None,
                creation_timestamp: None,
                controller_owner: None,
                labels: Default::default(),
                annotations: Default::default(),
                cells: Default::default(),
                log_containers: Vec::new(),
            }],
        }) as WorkerResultBox);
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(NodeMetricsUpdated {
            cluster_key: 1,
            usages: BTreeMap::from([(
                "kind-control-plane".into(),
                NodeUsage {
                    timestamp: OffsetDateTime::now_utc(),
                    cpu_nanocores: 500_000_000,
                    memory_bytes: 1024 * 1024 * 1024,
                },
            )]),
        }) as WorkerResultBox);
    harness.run();
    harness.get_by_label("Cluster-wide");
    harness.get_by_label("Open details for kind-control-plane");
    harness.get_by_label("500m");
    harness.get_by_label("1Gi");
    harness.ui_harness(
        "resource_tables/cluster_scoped_resources_load_once_without_a_namespace_selection/node_metrics",
    );

    let cluster = harness.state_mut().ui_state.clusters.get_mut(&1).unwrap();
    cluster.selected_namespaces = HashSet::from(["default".into(), "kube-system".into()]);
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1]
            .resource_cache
            .get(&(nodes, None))
            .expect("cluster-scoped watch state should be retained")
            .resources
            .len(),
        1
    );
    assert_eq!(
        harness
            .state()
            .worker
            .commands_of::<ReconcileResourceWatches>()
            .count(),
        1
    );
}

#[test]
fn namespace_selector_search_snapshot_shows_active_watches() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.namespaces.insert(
        "monitoring".into(),
        MinimalNamespace {
            name: "monitoring".into(),
            display_name: Some("Monitoring".into()),
        },
    );
    cluster.selected_namespaces = HashSet::from(["kube-system".into(), "monitoring".into()]);
    cluster.resource_cache.insert(
        (pods.clone(), Some("monitoring".into())),
        ResourceWatchState {
            is_synced: true,
            ..Default::default()
        },
    );
    cluster.active_watchers = HashSet::from([
        (pods.clone(), Some("kube-system".into())),
        (pods, Some("monitoring".into())),
    ]);

    let mut harness = application_harness::<MockWorker>();
    harness.seed_ui_state(state);
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("m".into()));
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/namespace_selector_search_snapshot_shows_active_watches/namespace_selector_open_filtered_active_watches",
    ));
}

#[test]
fn oracle_resource_table_snapshot_uses_injected_cluster_state() {
    let mut harness = application_harness_with_state(oracle_resource_table_state());
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.ui_harness("resource_tables/oracle_resource_table_snapshot_uses_injected_cluster_state/oracle_resource_table_injected");
}

#[test]
fn helm_releases_snapshot_shows_read_only_inventory() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).unwrap();
    cluster.selected_api_resource = Some(crate::api_resource::ApiResource::helm_releases());
    cluster.helm_release_cache.insert(
        "kube-system".into(),
        HelmReleaseWatchState {
            releases: vec![fixture_helm_release()],
            is_synced: true,
            backend_errors: BTreeMap::new(),
        },
    );
    harness.state_mut().ui_state = state;

    harness.run();

    harness.ui_harness(
        "helm_releases/helm_releases_snapshot_shows_read_only_inventory/releases_inventory",
    );
}

#[test]
fn selecting_a_helm_release_from_the_workspace_opens_its_inspector() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).unwrap();
    cluster.selected_api_resource = Some(crate::api_resource::ApiResource::helm_releases());
    cluster.helm_release_cache.insert(
        "kube-system".into(),
        HelmReleaseWatchState {
            releases: vec![fixture_helm_release()],
            is_synced: true,
            backend_errors: BTreeMap::new(),
        },
    );
    harness.state_mut().ui_state = state;

    harness.run();
    harness.get_by_label("Inspect Helm release demo").click();
    harness.run();

    harness.get_by_label("Select revision 2");
}

#[test]
fn helm_release_inspector_snapshot_shows_values_warning_and_revision_details() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state
        .clusters
        .get_mut(&2)
        .unwrap()
        .helm_release_cache
        .insert(
            "kube-system".into(),
            HelmReleaseWatchState {
                releases: fixture_helm_release_revisions(),
                is_synced: true,
                backend_errors: BTreeMap::new(),
            },
        );
    let mut commands = Vec::new();
    state.open_helm_release_detail(2, "demo".into(), "kube-system".into(), &mut commands);
    assert!(commands.is_empty());
    harness.state_mut().ui_state = state;

    harness.run();

    harness.ui_harness(
        "helm_releases/helm_release_inspector_snapshot_shows_values_warning_and_revision_details/release_inspector",
    );
}

#[test]
fn helm_release_inspector_selects_a_previous_revision() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state
        .clusters
        .get_mut(&2)
        .unwrap()
        .helm_release_cache
        .insert(
            "kube-system".into(),
            HelmReleaseWatchState {
                releases: fixture_helm_release_revisions(),
                is_synced: true,
                backend_errors: BTreeMap::new(),
            },
        );
    let mut commands = Vec::new();
    state.open_helm_release_detail(2, "demo".into(), "kube-system".into(), &mut commands);
    harness.state_mut().ui_state = state;

    harness.run();
    harness.get_by_label("Select revision 1").click();
    harness.run();

    harness.get_by_label("Revision 1 selected");
    harness.ui_harness(
        "helm_releases/helm_release_inspector_selects_a_previous_revision/previous_revision",
    );
}

#[test]
fn helm_release_inspector_expands_values_with_an_accessible_disclosure() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state
        .clusters
        .get_mut(&2)
        .unwrap()
        .helm_release_cache
        .insert(
            "kube-system".into(),
            HelmReleaseWatchState {
                releases: fixture_helm_release_revisions(),
                is_synced: true,
                backend_errors: BTreeMap::new(),
            },
        );
    let mut commands = Vec::new();
    state.open_helm_release_detail(2, "demo".into(), "kube-system".into(), &mut commands);
    harness.state_mut().ui_state = state;

    harness.run();
    harness
        .get_by_label("Values (sensitive values may be present)")
        .click();
    harness.run();

    harness.get_by_label("Treat this content like a Kubernetes Secret.");
    harness.ui_harness(
        "helm_releases/helm_release_inspector_expands_values_with_an_accessible_disclosure/values_expanded",
    );
}
