//! Application-level navigation and destructive-action scenarios.

use super::*;

#[test]
fn force_delete_confirmation_requires_the_resource_name_before_removing_finalizers() {
    let api_resource = fixture_api_resource("", "ConfigMap", "configmaps");
    let mut cluster = fixture_cluster(1, "dev");
    cluster.pending_force_delete = Some(PendingForceDelete {
        api_resource,
        resource_name: "important-config".into(),
        resource_uid: "important-config-uid".into(),
        namespace: Some("default".into()),
        finalizers: vec!["example.com/cleanup".into()],
        acknowledgement: "important-config".into(),
        confirmation_available_at: std::time::Instant::now() + std::time::Duration::from_secs(3600),
    });
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    };

    // A correct acknowledgement must still respect the confirmation delay.
    harness.run_steps(4);
    harness.get_by_label("Remove finalizers").click();
    harness.step();
    assert!(harness.state().worker.commands.is_empty());

    let pending = harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&1)
        .and_then(|cluster| cluster.pending_force_delete.as_mut())
        .expect("force deletion should still be pending");
    pending.confirmation_available_at = std::time::Instant::now();
    pending.acknowledgement = "wrong-name".into();
    harness.run();
    // Snapshot the stable state after the wall-clock countdown has elapsed.
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_actions/force_delete_confirmation_requires_the_resource_name_before_removing_finalizers/force_delete_confirmation",
    ));
    // The delay has elapsed, but a non-empty wrong acknowledgement is still rejected.
    harness.get_by_label("Remove finalizers").click();
    harness.run();
    assert!(harness.state().worker.commands.is_empty());
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&1)
        .and_then(|cluster| cluster.pending_force_delete.as_mut())
        .expect("force deletion should still be pending")
        .acknowledgement = "important-config".into();
    harness.run();
    harness.get_by_label("Remove finalizers").click();
    harness.run();

    assert!(matches!(
        harness.state().worker.commands.as_slice(),
        [command] if command.as_ref().as_any().downcast_ref::<ForceDeleteResource>()
            .is_some_and(|command| command.resource_name == "important-config")
    ));
    assert!(
        harness.cluster(1).pending_force_delete.is_none(),
        "confirming closes the dialog immediately, so completions never need to"
    );
}

#[test]
fn force_delete_is_available_from_a_deleting_resource_inspector() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let mut detail = config_map_detail(BTreeMap::new());
    detail.is_deleting = true;
    detail.finalizers = vec!["example.com/cleanup".into()];
    open_typed_detail(&mut harness, detail.api_resource.clone(), detail);

    harness
        .get_by_label("More actions for settings")
        .click_accesskit();
    harness.run();
    harness
        .get_by_label("Force delete (remove finalizers)")
        .click_accesskit();
    harness.run();

    assert!(
        harness.state().ui_state.clusters[&2]
            .pending_force_delete
            .as_ref()
            .is_some_and(|pending| {
                pending.resource_name == "settings"
                    && pending.resource_uid == "configmap-uid"
                    && pending.finalizers == ["example.com/cleanup"]
            })
    );
}

#[test]
fn force_delete_failure_is_recorded_in_operation_history() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceForceDeleteFailed {
            cluster_key: 2,
            api_resource: fixture_api_resource("core", "ConfigMap", "configmaps"),
            namespace: Some("kube-system".into()),
            resource_name: "settings".into(),
            error: "Resource was replaced while awaiting confirmation".into(),
        }) as WorkerResultBox);

    harness.run();
    let entry = harness.state().ui_state.clusters[&2]
        .operation_history
        .last()
        .expect("force deletion failure is recorded");
    assert_eq!(entry.title, "Couldn’t remove finalizers from ConfigMap");
    assert_eq!(entry.target, "settings • kube-system");
}

#[test]
fn resource_navigation_selects_primary_curated_gateway_and_other_resources() {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.selected_namespaces.insert("default".into());
    cluster.resource_navigation = build_resource_navigation(vec![
        fixture_api_resource("core", "Node", "nodes"),
        fixture_api_resource("core", "Namespace", "namespaces"),
        fixture_api_resource("core", "Event", "events"),
        fixture_api_resource("core", "Pod", "pods"),
        fixture_api_resource("apps", "Deployment", "deployments"),
        fixture_api_resource("gateway.networking.k8s.io", "HTTPRoute", "httproutes"),
        fixture_api_resource("apps", "ControllerRevision", "controllerrevisions"),
    ]);
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    };
    harness.run();

    harness.get_by_label("Nodes").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1]
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("nodes")
    );

    harness.get_by_label("Namespaces").click_accesskit();
    harness.run();
    harness.get_by_label("Events").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1]
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("events")
    );

    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.get_by_label("Pods").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1]
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("pods")
    );

    harness.get_by_label("Gateway API").click_accesskit();
    harness.run();
    harness.get_by_label("HTTP Routes").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1]
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("httproutes")
    );

    harness.get_by_label("Other Resources").click_accesskit();
    harness.run();
    harness.get_by_label("apps").click_accesskit();
    harness.run();
    harness
        .get_by_label("Controller Revisions")
        .click_accesskit();
    harness.run();

    let app = harness.state();
    assert_eq!(
        app.ui_state.clusters[&1]
            .selected_api_resource
            .as_ref()
            .map(|resource| resource.name.as_str()),
        Some("controllerrevisions")
    );
    assert_eq!(
        app.worker
            .commands
            .iter()
            .filter_map(|command| {
                command
                    .as_ref()
                    .as_any()
                    .downcast_ref::<ReconcileResourceWatches>()
                    .map(|command| command.api_resource.name.as_str())
            })
            .collect::<Vec<_>>(),
        vec![
            "nodes",
            "namespaces",
            "events",
            "pods",
            "httproutes",
            "controllerrevisions",
        ]
    );
}

#[test]
fn test_ui_flow() {
    let watch_error = r#"InitialListFailed(
    Service(
        hyper_util::client::legacy::Error(
            Connect,
            ConnectError(
                "dns error",
                Custom {
                    kind: Uncategorized,
                    error: "failed to lookup address information: Name or service not known",
                },
            ),
        ),
    ),
)"#;
    let mut harness = application_harness::<MockWorker>();
    harness.run();
    harness.ui_harness("cluster_connection/test_ui_flow/01_empty_state");

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesClustersUpdated(vec![
            Cluster {
                name: "dev".into(),
                is_current: true,
                presentation: Default::default(),
            },
            Cluster {
                name: "prod".into(),
                is_current: false,
                presentation: Default::default(),
            },
        ])) as WorkerResultBox);
    harness.run_steps(1);
    assert_eq!(harness.state().ui_state.selected_cluster, Some(1));
    assert!(matches!(
        harness.state().ui_state.clusters[&1].connection,
        ClusterConnectionState::Connecting
    ));
    assert!(matches!(
        harness.state().worker.commands.as_slice(),
        [command] if command.as_ref().as_any().downcast_ref::<ConnectToCluster>()
            .is_some_and(|command| command.cluster_key == 1 && command.cluster == "dev")
    ));
    harness.ui_harness("cluster_connection/test_ui_flow/current_context_connecting");

    harness
        .state_mut()
        .worker
        .results
        .push_back(
            Box::new(KubernetesClusterConnectionCreated { cluster_key: 1 }) as WorkerResultBox,
        );

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesNamespacesReplaced {
            cluster_key: 1,
            namespaces: vec![
                MinimalNamespace {
                    name: "default".into(),
                    labels: Default::default(),
                    annotations: Default::default(),
                },
                MinimalNamespace {
                    name: "kube-system".into(),
                    labels: Default::default(),
                    annotations: Default::default(),
                },
                MinimalNamespace {
                    name: "monitoring".into(),
                    labels: Default::default(),
                    annotations: Default::default(),
                },
            ],
        }) as WorkerResultBox);
    harness.run_steps(1);

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesApisLoaded {
            cluster_key: 1,
            api_resources: vec![
                fixture_api_resource("", "Pod", "pods"),
                fixture_api_resource("", "Service", "services"),
                fixture_api_resource("", "ConfigMap", "configmaps"),
                fixture_api_resource("apps", "Deployment", "deployments"),
                fixture_api_resource("apps", "StatefulSet", "statefulsets"),
                fixture_api_resource("networking.k8s.io", "Ingress", "ingresses"),
            ],
            scalable_api_resources: Default::default(),
            failed_api_groups: Vec::new(),
            pod_metrics_api_available: true,
            node_metrics_api_available: true,
        }) as WorkerResultBox);
    harness.run();

    select_namespace(&mut harness, "default");
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.get_by_label("Pods").click_accesskit();
    harness.run_steps(1);
    harness.run_steps(1);

    let pods = fixture_api_resource("", "Pod", "pods");
    assert!(harness.state().worker.commands.iter().any(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<ReconcileResourceWatches>()
            .is_some_and(|command| {
                command.cluster_key == 1
                    && command.api_resource == pods
                    && matches!(
                        command.sources.as_slice(),
                        [ResourceWatchSource::Namespace(namespace)] if namespace == "default"
                    )
            })
    }));
    harness.get_by_label("Loading resources");
    harness.ui_harness("cluster_connection/test_ui_flow/resource_watch_loading");

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesResourcesReplaced {
            cluster_key: 1,
            api_resource: pods.clone(),
            namespace: Some("default".into()),
            resources: Vec::new(),
        }) as WorkerResultBox);
    harness.run();
    harness.get_by_label("No resources found");

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesResourceWatchFailed {
            cluster_key: 1,
            api_resource: pods,
            namespace: Some("default".into()),
            error: watch_error.into(),
        }) as WorkerResultBox);
    harness.run();
    harness.get_by_label("Unable to load resources");
    harness.get_by_label(watch_error);
    harness.ui_harness("cluster_connection/test_ui_flow/resource_watch_error");
    harness.get_by_label("Retry").click_accesskit();
    harness.run_steps(1);
    assert_eq!(
        harness
            .state()
            .worker
            .commands
            .iter()
            .filter(|command| command_is::<ReconcileResourceWatches>(command).is_some())
            .count(),
        2
    );
}

fn pending_delete_for(name: &str) -> PendingDelete {
    PendingDelete {
        api_resource: fixture_api_resource("", "ConfigMap", "configmaps"),
        resource_name: name.into(),
        namespace: Some("default".into()),
        confirmation_available_at: std::time::Instant::now(),
    }
}

fn config_map_delete_completed(name: &str, bulk_delete_id: Option<u64>) -> ResourceDeleteCompleted {
    ResourceDeleteCompleted {
        cluster_key: 1,
        api_resource: fixture_api_resource("", "ConfigMap", "configmaps"),
        namespace: Some("default".into()),
        resource_name: name.into(),
        bulk_delete_id,
    }
}

#[test]
fn delete_completion_keeps_the_confirmation_opened_from_a_row_action() {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.selected_api_resource = Some(deployment.clone());
    cluster.resource_cache.insert(
        (deployment.clone(), Some("kube-system".to_owned())),
        ResourceWatchState {
            resources: ["alpha", "bravo"]
                .into_iter()
                .map(|name| {
                    (
                        format!("{name}-uid"),
                        MinimalResource {
                            uid: format!("{name}-uid"),
                            name: name.to_owned(),
                            namespace: Some("kube-system".to_owned()),
                            creation_timestamp: None,
                            controller_owner: None,
                            labels: Default::default(),
                            annotations: Default::default(),
                            cells: BTreeMap::new(),
                            log_containers: Vec::new(),
                        },
                    )
                })
                .collect(),
            is_synced: true,
            ..Default::default()
        },
    );
    let mut harness = application_harness_with_state(state);

    harness.get_by_label("More actions for bravo").click();
    harness.run();
    harness.get_by_label("Delete").click();
    harness.run();
    harness.get_by_label("Delete bravo");

    harness.deliver_worker_result(ResourceDeleteCompleted {
        cluster_key: 2,
        api_resource: deployment,
        namespace: Some("kube-system".into()),
        resource_name: "alpha".into(),
        bulk_delete_id: None,
    });

    harness.get_by_label("Delete bravo");
    assert!(harness.cluster(2).pending_delete.is_some());
}

#[test]
fn delete_completion_for_another_resource_keeps_the_open_delete_confirmation() {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.pending_delete = Some(pending_delete_for("resource-b"));
    let mut harness = application_harness_with_cluster(cluster);

    harness.deliver_worker_result(config_map_delete_completed("resource-a", None));

    assert!(harness.cluster(1).pending_delete.is_some());
    harness.get_by_label("Delete resource-b");
}

#[test]
fn bulk_delete_completion_keeps_the_open_delete_confirmation() {
    let target = BulkDeleteTarget {
        uid: "a-uid".into(),
        name: "resource-a".into(),
        namespace: Some("default".into()),
    };
    let mut cluster = fixture_cluster(1, "dev");
    cluster.pending_delete = Some(pending_delete_for("resource-b"));
    cluster.bulk_delete_progress = Some(BulkDeleteProgress::new(
        7,
        fixture_api_resource("", "ConfigMap", "configmaps"),
        vec![target],
    ));
    let mut harness = application_harness_with_cluster(cluster);

    harness.deliver_worker_result(config_map_delete_completed("resource-a", Some(7)));

    assert!(harness.cluster(1).pending_delete.is_some());
    harness.get_by_label("Delete resource-b");
}

#[test]
fn force_delete_completion_for_another_resource_keeps_the_open_confirmation() {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.pending_force_delete = Some(PendingForceDelete {
        api_resource: fixture_api_resource("", "ConfigMap", "configmaps"),
        resource_name: "resource-b".into(),
        resource_uid: "b-uid".into(),
        namespace: Some("default".into()),
        finalizers: vec!["example.com/cleanup".into()],
        acknowledgement: "resource-".into(),
        confirmation_available_at: std::time::Instant::now(),
    });
    let mut harness = application_harness_with_cluster(cluster);

    harness.deliver_worker_result(ResourceForceDeleteCompleted {
        cluster_key: 1,
        api_resource: fixture_api_resource("", "ConfigMap", "configmaps"),
        namespace: Some("default".into()),
        resource_name: "resource-a".into(),
    });

    let pending = harness
        .cluster(1)
        .pending_force_delete
        .as_ref()
        .expect("resource-b's confirmation must stay open");
    assert_eq!(pending.acknowledgement, "resource-");
    harness.get_by_label("Remove finalizers");
}

fn pending_scale_for(name: &str, desired: &str) -> crate::ui::state::PendingScale {
    crate::ui::state::PendingScale {
        api_resource: fixture_api_resource("apps", "Deployment", "deployments"),
        resource_name: name.into(),
        namespace: Some("default".into()),
        current_replicas: 2,
        desired_replicas: desired.into(),
    }
}

fn request_scale(harness: &mut Harness<'static, MyEguiApp<MockWorker>>, name: &str) -> u64 {
    harness
        .state_mut()
        .ui_state
        .request_resource_scale(
            1,
            fixture_api_resource("apps", "Deployment", "deployments"),
            Some("default".into()),
            name.into(),
        )
        .request_id
}

fn scale_fetched(name: &str, request_id: u64, replicas: i32) -> ResourceScaleFetched {
    ResourceScaleFetched {
        cluster_key: 1,
        request_id,
        api_resource: fixture_api_resource("apps", "Deployment", "deployments"),
        namespace: Some("default".into()),
        resource_name: name.into(),
        replicas,
    }
}

fn scale_fetch_failed(name: &str, request_id: u64) -> ResourceScaleFetchFailed {
    ResourceScaleFetchFailed {
        cluster_key: 1,
        request_id,
        api_resource: fixture_api_resource("apps", "Deployment", "deployments"),
        namespace: Some("default".into()),
        resource_name: name.into(),
        error: "boom".into(),
    }
}

#[test]
fn scale_update_for_another_resource_keeps_the_scale_dialog_the_user_is_editing() {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.pending_scale = Some(pending_scale_for("resource-b", "7"));
    let mut harness = application_harness_with_cluster(cluster);

    harness.deliver_worker_result(ResourceScaleUpdated {
        cluster_key: 1,
        api_resource: fixture_api_resource("apps", "Deployment", "deployments"),
        namespace: Some("default".into()),
        resource_name: "resource-a".into(),
        replicas: 3,
    });

    let pending = harness
        .cluster(1)
        .pending_scale
        .as_ref()
        .expect("resource-b's scale dialog must stay open");
    assert_eq!(pending.resource_name, "resource-b");
    assert_eq!(pending.desired_replicas, "7");
    harness.get_by_label("Scale resource-b");
}

#[test]
fn late_scale_fetch_does_not_replace_the_open_scale_dialog() {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.pending_scale = Some(pending_scale_for("resource-b", "7"));
    let mut harness = application_harness_with_cluster(cluster);

    harness.deliver_worker_result(scale_fetched("resource-a", 1, 3));

    let pending = harness
        .cluster(1)
        .pending_scale
        .as_ref()
        .expect("resource-b's scale dialog must stay open");
    assert_eq!(pending.resource_name, "resource-b");
    assert_eq!(pending.desired_replicas, "7");
    harness.get_by_label("Scale resource-b");
}

#[test]
fn only_the_latest_scale_fetch_opens_the_scale_dialog() {
    let mut harness = application_harness_with_cluster(fixture_cluster(1, "dev"));
    let first = request_scale(&mut harness, "resource-a");
    let second = request_scale(&mut harness, "resource-b");
    assert_ne!(first, second);

    harness.deliver_worker_result(scale_fetched("resource-a", first, 3));
    assert!(harness.cluster(1).pending_scale.is_none());

    harness.deliver_worker_result(scale_fetched("resource-b", second, 5));
    harness.get_by_label("Scale resource-b");
    assert_eq!(
        harness
            .cluster(1)
            .pending_scale
            .as_ref()
            .map(|pending| pending.desired_replicas.as_str()),
        Some("5")
    );

    harness.deliver_worker_result(scale_fetched("resource-b", second, 9));
    assert_eq!(
        harness
            .cluster(1)
            .pending_scale
            .as_ref()
            .map(|pending| pending.desired_replicas.as_str()),
        Some("5"),
        "a duplicate reply must not reset what the user is editing"
    );
}

#[test]
fn scale_fetch_arriving_after_navigation_does_not_open_the_dialog() {
    type Navigate = fn(&mut UiState);
    let navigations: [(&str, Navigate); 3] = [
        ("namespace change", |ui| {
            ui.clusters
                .get_mut(&1)
                .unwrap()
                .selected_namespaces
                .insert("other".into());
        }),
        ("resource type change", |ui| {
            ui.clusters.get_mut(&1).unwrap().selected_api_resource =
                Some(fixture_api_resource("", "Pod", "pods"));
        }),
        ("cluster switch", |ui| {
            ui.clusters.insert(2, fixture_cluster(2, "staging"));
            ui.selected_cluster = Some(2);
        }),
    ];
    for (label, navigate) in navigations {
        let mut harness = application_harness_with_cluster(fixture_cluster(1, "dev"));
        let request_id = request_scale(&mut harness, "resource-a");
        navigate(&mut harness.state_mut().ui_state);
        harness.run();

        harness.deliver_worker_result(scale_fetched("resource-a", request_id, 3));

        assert!(
            harness.cluster(1).pending_scale.is_none(),
            "a late scale reply after a {label} must not open the dialog"
        );
    }
}

#[test]
fn scale_fetch_failures_are_recorded_and_only_the_awaited_one_cancels_the_lookup() {
    let mut harness = application_harness_with_cluster(connected_cluster());
    let stale = request_scale(&mut harness, "resource-a");
    let awaited = request_scale(&mut harness, "resource-b");

    harness.deliver_worker_result(scale_fetch_failed("resource-a", stale));
    assert_eq!(harness.cluster(1).operation_history.len(), 1);
    harness.deliver_worker_result(scale_fetched("resource-b", awaited, 4));
    harness.get_by_label("Scale resource-b");

    let mut harness = application_harness_with_cluster(connected_cluster());
    let awaited = request_scale(&mut harness, "resource-b");
    harness.deliver_worker_result(scale_fetch_failed("resource-b", awaited));
    assert_eq!(harness.cluster(1).operation_history.len(), 1);
    harness.deliver_worker_result(scale_fetched("resource-b", awaited, 4));
    assert!(
        harness.cluster(1).pending_scale.is_none(),
        "a reply for a failed lookup must not open the dialog"
    );
}

fn connected_cluster() -> crate::ui::state::ClusterState {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.connection = ClusterConnectionState::Connected;
    cluster
}
