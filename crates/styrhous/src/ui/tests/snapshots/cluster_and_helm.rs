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
                labels: Default::default(),
                annotations: Default::default(),
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
        SelectedNamespaces::from(["default"])
    );

    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace scope")
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
        SelectedNamespaces::from(["default", "kube-system"])
    );

    harness.get_by_label("default").click();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1].selected_namespaces,
        SelectedNamespaces::from(["default"])
    );

    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace scope")
        .click();
    harness.run();

    harness.get_by_label("All namespaces").click();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1].selected_namespaces,
        SelectedNamespaces::All
    );

    harness.get_by_label("All namespaces").click();
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&1]
            .namespace_scope()
            .is_empty()
    );

    harness.get_by_label("All namespaces").click();
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
        SelectedNamespaces::from(["default"])
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
fn select_all_namespaces_includes_namespaces_created_afterwards() {
    let pods = fixture_api_resource("", "Pod", "pods");
    let mut cluster = fixture_cluster(1, "dev");
    cluster.connection = ClusterConnectionState::Connected;
    cluster.selected_api_resource = Some(pods.clone());
    for namespace in ["default", "kube-system"] {
        cluster
            .namespaces
            .insert(namespace.into(), MinimalNamespace::named(namespace));
    }
    let list_pods = || KubernetesResourcesReplacedInEveryNamespace {
        cluster_key: 1,
        api_resource: pods.clone(),
        resources_by_namespace: BTreeMap::new(),
    };
    let mut harness = application_harness::<MockWorker>();
    harness.seed_ui_state(UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    });

    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace scope")
        .click();
    harness.run();
    harness.get_by_label("All namespaces").click();
    // Loading watches show a spinner, which keeps repainting until they list.
    harness.run_steps(2);
    assert!(
        harness
            .state()
            .worker
            .commands_of::<ReconcileResourceWatches>()
            .any(|command| matches!(
                command.sources.as_slice(),
                [ResourceWatchSource::AllNamespaces(WatchedNamespaces::Every)]
            ))
    );
    harness.state_mut().worker.enqueue_result(list_pods());
    harness.run_steps(2);
    harness.state_mut().worker.commands.clear();

    harness
        .state_mut()
        .worker
        .enqueue_result(KubernetesNamespacesAdded {
            cluster_key: 1,
            namespace: MinimalNamespace::named("monitoring"),
        });
    harness.run_steps(2);

    assert_eq!(
        harness.cluster(1).selected_namespaces,
        SelectedNamespaces::All
    );
    assert_eq!(
        harness
            .state()
            .worker
            .commands_of::<ReconcileResourceWatches>()
            .count(),
        0,
        "the watch already covers namespaces created later"
    );
    assert!(harness.cluster(1).is_namespace_watched(&pods, "monitoring"));
    for namespace in ["All namespaces", "default", "kube-system", "monitoring"] {
        assert_eq!(
            harness.get_by_label(namespace).accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True),
            "{namespace} must be shown as selected"
        );
    }

    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/select_all_namespaces_includes_namespaces_created_afterwards/namespace_selector_all_namespaces_after_creation",
    ));
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
            .namespace_scope()
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
    // A cluster-scoped kind's scope pill says "Cluster-wide" and cannot be
    // opened to choose namespaces.
    let scope = harness.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace scope");
    assert_eq!(scope.value().as_deref(), Some("Cluster-wide"));
    scope.click();
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(egui::accesskit::Role::TextInput, "Search Namespace")
            .is_none()
    );
    harness.get_by_label("Open details for kind-control-plane");
    harness.get_by_label("0.500");
    harness.get_by_label("1Gi");
    harness.ui_harness(
        "resource_tables/cluster_scoped_resources_load_once_without_a_namespace_selection/node_metrics",
    );

    let cluster = harness.state_mut().ui_state.clusters.get_mut(&1).unwrap();
    cluster.selected_namespaces = SelectedNamespaces::from(["default", "kube-system"]);
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
            labels: Default::default(),
            annotations: Default::default(),
        },
    );
    cluster.selected_namespaces = SelectedNamespaces::from(["kube-system", "monitoring"]);
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
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace scope")
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
fn namespace_selector_shows_a_reconnecting_indicator_only_while_the_watcher_is_degraded() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.selected_api_resource = Some(pods);
    let mut harness = application_harness::<MockWorker>();
    harness.seed_ui_state(state);
    harness.run();
    assert!(harness.query_by_label("namespaces reconnecting").is_none());

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesNamespacesReconnecting {
            cluster_key: 2,
            error: "connection lost".into(),
        }) as WorkerResultBox);
    harness.run();
    assert!(harness.query_by_label("namespaces reconnecting").is_some());
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/namespace_selector_reconnecting_indicator/namespace_selector_reconnecting",
    ));

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesNamespacesRecovered { cluster_key: 2 }) as WorkerResultBox);
    harness.run();
    assert!(harness.query_by_label("namespaces reconnecting").is_none());
}

#[test]
fn namespace_selector_searches_configured_identity_values() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.namespaces.insert(
        "company-a1b2c3".into(),
        MinimalNamespace {
            name: "company-a1b2c3".into(),
            labels: BTreeMap::from([("company.example/customer".into(), "Acme".into())]),
            annotations: BTreeMap::from([(
                "company.example/environment".into(),
                "Production".into(),
            )]),
        },
    );
    cluster.selected_api_resource = Some(pods);
    let mut harness = application_harness::<MockWorker>();
    harness.seed_ui_state(state);
    harness.state_mut().namespace_selector_settings =
        crate::ui::namespace_selector::NamespaceSelectorSettings {
            fields: vec![
                crate::ui::namespace_selector::NamespaceMetadataField {
                    alias: "customer".into(),
                    source: crate::ui::table_preferences::MetadataColumnSource::Label,
                    key: "company.example/customer".into(),
                },
                crate::ui::namespace_selector::NamespaceMetadataField {
                    alias: "environment".into(),
                    source: crate::ui::table_preferences::MetadataColumnSource::Annotation,
                    key: "company.example/environment".into(),
                },
            ],
            templates: vec![crate::ui::namespace_selector::NamespaceIdentityTemplate {
                template: "Customer: {{customer}}".into(),
            }],
        };
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace scope")
        .click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("production".into()));
    harness.run();

    harness.get_by_label("Customer: Acme (company-a1b2c3)");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/namespace_selector_searches_configured_identity_values/identity_value_search",
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

fn demo_release() -> HelmReleaseKey {
    HelmReleaseKey {
        namespace: "kube-system".into(),
        release_name: "demo".into(),
    }
}

fn set_helm_releases(state: &mut UiState, releases: Vec<HelmRelease>) {
    state
        .clusters
        .get_mut(&2)
        .expect("kind fixture exists")
        .helm_release_cache
        .insert(
            "kube-system".into(),
            HelmReleaseWatchState {
                releases,
                is_synced: true,
                backend_errors: BTreeMap::new(),
            },
        );
}

/// The demo release inspector (revisions 2 and 1) with revision 1 selected.
fn helm_rollback_harness(
    customize: impl FnOnce(&mut UiState),
) -> Harness<'static, MyEguiApp<MockWorker>> {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let mut releases = fixture_helm_release_revisions();
    releases[1].chart_version = "1.0.0".into();
    set_helm_releases(&mut state, releases);
    customize(&mut state);
    let mut commands = Vec::new();
    state.open_helm_release_detail(2, "demo".into(), "kube-system".into(), &mut commands);
    harness.seed_ui_state(state);
    harness.get_by_label("Select revision 1").click();
    harness.run();
    harness
}

fn open_rollback_confirmation(harness: &mut Harness<'static, MyEguiApp<MockWorker>>) {
    harness.get_by_label("Roll back to revision 1").click();
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();
    harness.get_by_label("Roll back demo to revision 1?");
}

fn confirm_rollback(harness: &mut Harness<'static, MyEguiApp<MockWorker>>) -> u64 {
    harness.get_by_label("Roll back").click();
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();
    harness
        .state()
        .worker
        .last_command::<RollbackHelmRelease>()
        .expect("rollback command sent")
        .operation_id
}

fn history(
    harness: &Harness<'static, MyEguiApp<MockWorker>>,
) -> Vec<(String, String, Option<String>)> {
    harness.state().ui_state.clusters[&2]
        .operation_history
        .iter()
        .map(|entry| {
            (
                entry.title.clone(),
                entry.target.clone(),
                entry.details.clone(),
            )
        })
        .collect()
}

fn rollback_completed(cluster_key: i32, operation_id: u64) -> HelmRollbackCompleted {
    HelmRollbackCompleted {
        cluster_key,
        operation_id,
        release: demo_release(),
        revision: 1,
    }
}

fn rollback_failed(operation_id: u64, error: &str, helm_missing: bool) -> HelmRollbackFailed {
    HelmRollbackFailed {
        cluster_key: 2,
        operation_id,
        release: demo_release(),
        revision: 1,
        error: error.into(),
        helm_missing,
    }
}

fn rollback_button_enabled(harness: &Harness<'static, MyEguiApp<MockWorker>>) -> bool {
    !harness
        .get_by_label("Roll back to revision 1")
        .accesskit_node()
        .is_disabled()
}

#[test]
fn helm_release_rollback_is_confirmed_tracked_and_reported() {
    let mut harness = helm_rollback_harness(|_| {});
    harness.get_by_label("Revision 1 selected");
    open_rollback_confirmation(&mut harness);
    harness.ui_harness(
        "helm_releases/helm_release_rollback_is_confirmed_tracked_and_reported/rollback_confirmation",
    );
    assert_eq!(confirm_rollback(&mut harness), 1);
    assert_eq!(
        harness
            .state()
            .worker
            .last_command::<RollbackHelmRelease>()
            .map(|command| (command.cluster_key, command.target.clone())),
        Some((
            2,
            HelmRollbackTarget {
                kube_context: "kind-kind".into(),
                release: demo_release(),
                storage: StorageDriver::Secret,
                revision: 1,
            }
        ))
    );
    assert!(
        harness
            .get_by_label("Rolling back…")
            .accesskit_node()
            .is_disabled(),
        "a running rollback blocks another submission"
    );
    harness.ui_harness(
        "helm_releases/helm_release_rollback_is_confirmed_tracked_and_reported/rollback_in_progress",
    );

    harness.deliver_worker_result(rollback_failed(
        1,
        "helm is not installed or not on PATH",
        true,
    ));
    assert!(rollback_button_enabled(&harness));
    open_rollback_confirmation(&mut harness);
    assert_eq!(confirm_rollback(&mut harness), 2);
    let helm_error =
        "helm rollback --namespace=kube-system -- demo 1 failed: Error: release: not found";
    harness.deliver_worker_result(rollback_failed(2, helm_error, false));
    assert!(rollback_button_enabled(&harness));

    open_rollback_confirmation(&mut harness);
    assert_eq!(confirm_rollback(&mut harness), 3);
    for operation_id in [3, 3, 99] {
        // The duplicate and the untracked result must both be ignored.
        harness.deliver_worker_result(rollback_completed(2, operation_id));
    }
    assert!(rollback_button_enabled(&harness));
    assert!(
        harness.state().ui_state.clusters[&2]
            .helm_rollbacks
            .ids()
            .is_empty()
    );

    let target = "demo • kube-system".to_owned();
    assert_eq!(
        history(&harness),
        vec![
            (
                "Couldn’t roll back to revision 1".to_owned(),
                target.clone(),
                Some(crate::ui::dialogs::HELM_INSTALL_GUIDANCE.to_owned())
            ),
            (
                "Couldn’t roll back to revision 1".to_owned(),
                target.clone(),
                Some(helm_error.to_owned())
            ),
            (
                "Helm release rolled back".to_owned(),
                target,
                Some("Rolled back to revision 1".to_owned())
            ),
        ]
    );
}

#[test]
fn helm_rollback_is_unavailable_for_the_current_revision() {
    let mut harness = helm_rollback_harness(|_| {});
    harness.get_by_label("Select revision 2").click();
    harness.run();
    assert!(harness.query_by_label("Roll back to revision 2").is_none());
    assert!(harness.query_by_label("Roll back to revision 1").is_none());
}

#[test]
fn helm_rollback_confirmation_can_be_cancelled_or_dismissed_with_escape() {
    let mut harness = helm_rollback_harness(|_| {});
    open_rollback_confirmation(&mut harness);
    harness.get_by_label("Cancel").click();
    harness.run();

    open_rollback_confirmation(&mut harness);
    harness.key_press(egui::Key::Escape);
    harness.run();

    let cluster = &harness.state().ui_state.clusters[&2];
    assert!(cluster.pending_helm_rollback.is_none());
    assert!(cluster.helm_rollbacks.ids().is_empty());
    assert!(
        harness
            .state()
            .worker
            .last_command::<RollbackHelmRelease>()
            .is_none()
    );
    assert!(
        rollback_button_enabled(&harness),
        "Escape dismisses only the dialog, not the inspector beneath it"
    );
}

#[test]
fn helm_rollback_progress_and_results_are_scoped_to_their_release_and_cluster() {
    let mut harness = helm_rollback_harness(|state| {
        let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
        // Rollbacks of other releases: same name elsewhere, other name here.
        for (namespace, release_name) in [("default", "demo"), ("kube-system", "other")] {
            cluster.helm_rollbacks.start(HelmReleaseKey {
                namespace: namespace.into(),
                release_name: release_name.into(),
            });
        }
        let other_cluster = state.clusters.get_mut(&3).expect("fixture exists");
        other_cluster.connection = ClusterConnectionState::Connected;
        other_cluster.helm_rollbacks.start(demo_release());
    });
    assert!(
        rollback_button_enabled(&harness),
        "only a rollback of this release disables its action"
    );

    open_rollback_confirmation(&mut harness);
    assert_eq!(confirm_rollback(&mut harness), 3);
    harness.deliver_worker_result(rollback_completed(3, 1));
    harness.deliver_worker_result(rollback_completed(3, 3));
    assert_eq!(
        harness.state().ui_state.clusters[&2].helm_rollbacks.ids(),
        vec![1, 2, 3],
        "results from another cluster must not settle this cluster's rollbacks"
    );
    assert!(harness.query_by_label("Rolling back…").is_some());
    assert!(history(&harness).is_empty());
}

#[test]
fn helm_rollback_dialog_follows_revisions_that_land_while_it_is_open() {
    let mut harness = helm_rollback_harness(|_| {});
    open_rollback_confirmation(&mut harness);

    // Someone upgrades the release (now stored in ConfigMaps) meanwhile.
    let mut state = std::mem::take(&mut harness.state_mut().ui_state);
    let mut releases = state.helm_releases(2, "kube-system", "demo");
    let mut upgraded = fixture_helm_release();
    upgraded.revision = 3;
    upgraded.chart_version = "2.0.0".into();
    upgraded.storage = StorageDriver::ConfigMap;
    releases.push(upgraded);
    set_helm_releases(&mut state, releases);
    harness.seed_ui_state(state);
    harness.get_by_label(
        "Helm will create a new revision of demo in the kube-system namespace from \
         revision 1 (nginx 1.0.0). The current one is revision 3 (nginx 2.0.0).",
    );

    confirm_rollback(&mut harness);
    assert_eq!(
        harness
            .state()
            .worker
            .last_command::<RollbackHelmRelease>()
            .map(|command| (command.target.storage, command.target.revision)),
        Some((StorageDriver::ConfigMap, 1)),
        "Helm reads the release through the current revision's storage driver"
    );
}

#[test]
fn helm_rollback_dialog_disables_rolling_back_to_a_pruned_revision() {
    let mut harness = helm_rollback_harness(|_| {});
    open_rollback_confirmation(&mut harness);

    let mut state = std::mem::take(&mut harness.state_mut().ui_state);
    let releases = state
        .helm_releases(2, "kube-system", "demo")
        .into_iter()
        .filter(|release| release.revision != 1)
        .collect();
    set_helm_releases(&mut state, releases);
    harness.seed_ui_state(state);
    harness.event(egui::Event::PointerGone);
    harness.run();

    harness.get_by_label("There is nothing to roll back to.");
    assert!(
        harness
            .get_by_label("Roll back")
            .accesskit_node()
            .is_disabled()
    );
    harness.ui_harness(
        "helm_releases/helm_rollback_dialog_disables_rolling_back_to_a_pruned_revision/pruned_revision",
    );
}

#[test]
fn helm_rollback_result_from_a_previous_session_is_ignored() {
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    let stale_operation = cluster.helm_rollbacks.start(demo_release());
    cluster.reset_for_connection();
    cluster.connection = ClusterConnectionState::Connected;
    cluster.mark_loaded_for_test();
    let current_operation = cluster.helm_rollbacks.start(demo_release());
    assert_ne!(stale_operation, current_operation);
    let mut harness = application_harness::<MockWorker>();
    harness.seed_ui_state(state);

    harness.deliver_worker_result(rollback_completed(2, stale_operation));
    assert!(history(&harness).is_empty());
    harness.deliver_worker_result(rollback_completed(2, current_operation));
    assert_eq!(history(&harness).len(), 1);
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

#[test]
fn namespace_selector_shows_while_the_namespace_list_loads_or_fails() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.namespaces.clear();
    cluster.namespaces_load = crate::ui::state::ClusterLoadState::Loading;
    cluster.selected_namespaces = SelectedNamespaces::default();
    cluster.selected_api_resource = Some(pods);
    let mut harness = application_harness::<MockWorker>();
    harness.seed_ui_state(state);

    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/namespace_selector_namespace_list_states/loading",
    ));

    // A watcher failing before its first list leaves the workspace usable.
    harness.deliver_worker_result(KubernetesNamespacesReconnecting {
        cluster_key: 2,
        error: "namespaces is forbidden".into(),
    });
    harness.get_by_label("namespaces unavailable");
    harness.get_by_label("Choose a namespace");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/namespace_selector_namespace_list_states/unavailable",
    ));

    harness.deliver_worker_result(KubernetesNamespacesReplaced {
        cluster_key: 2,
        namespaces: vec![MinimalNamespace::named("default")],
    });
    assert!(harness.query_by_label("namespaces unavailable").is_none());
}

#[test]
fn helm_releases_in_all_namespaces_wait_for_the_namespace_list() {
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.namespaces.clear();
    cluster.namespaces_load = crate::ui::state::ClusterLoadState::Loading;
    cluster.selected_namespaces = SelectedNamespaces::All;
    cluster.selected_api_resource = Some(crate::api_resource::ApiResource::helm_releases());
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    // The loading state's spinner keeps repainting.
    harness.run_steps(3);

    harness.get_by_label("Loading namespaces");
    assert!(harness.query_by_label("Choose a namespace").is_none());
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "helm_releases/helm_releases_in_all_namespaces_wait_for_the_namespace_list/loading",
    ));

    harness
        .state_mut()
        .worker
        .enqueue_result(KubernetesNamespacesReconnecting {
            cluster_key: 2,
            error: "namespaces is forbidden".into(),
        });
    harness.run_steps(3);

    // The page names the failure; the toolbar notes it as well.
    harness.get_by_label("namespaces is forbidden");
    assert!(harness.query_by_label("Choose a namespace").is_none());
}

#[test]
fn clicking_a_helm_release_header_sorts_the_releases() {
    let release = |name: &str, revision: i64| HelmRelease {
        name: name.into(),
        revision,
        ..fixture_helm_release()
    };
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).unwrap();
    cluster.selected_api_resource = Some(crate::api_resource::ApiResource::helm_releases());
    cluster.helm_release_cache.insert(
        "kube-system".into(),
        HelmReleaseWatchState {
            releases: vec![release("demo", 2), release("alpha", 5), release("zeta", 1)],
            is_synced: true,
            backend_errors: BTreeMap::new(),
        },
    );
    harness.state_mut().ui_state = state;
    harness.run();

    let release_names = |harness: &Harness<MyEguiApp<MockWorker>>| {
        let mut rows = harness
            .query_all_by_role(egui::accesskit::Role::Button)
            .filter_map(|node| {
                let label = node.accesskit_node().label()?;
                let name = label.strip_prefix("Inspect Helm release ")?.to_owned();
                Some((node.rect().top(), name))
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.0.total_cmp(&right.0));
        rows.into_iter().map(|(_, name)| name).collect::<Vec<_>>()
    };
    let default_order = release_names(&harness);
    for expected in [
        ["zeta", "demo", "alpha"].map(String::from).to_vec(),
        ["alpha", "demo", "zeta"].map(String::from).to_vec(),
        default_order,
    ] {
        click_label(&mut harness, "Revision column");
        assert_eq!(release_names(&harness), expected);
    }
}
