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
        acknowledgement: "wrong-name".into(),
        confirmation_available_at: std::time::Instant::now() + std::time::Duration::from_secs(3),
    });
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    };

    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_actions/force_delete_confirmation_requires_the_resource_name_before_removing_finalizers/force_delete_confirmation",
    ));
    harness.get_by_label("Remove finalizers").click_accesskit();
    harness.run();
    assert!(harness.state().worker.commands.is_empty());

    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&1)
        .and_then(|cluster| cluster.pending_force_delete.as_mut())
        .expect("force deletion should still be pending")
        .confirmation_available_at = std::time::Instant::now();
    harness.run();
    // The delay has elapsed, but a non-empty wrong acknowledgement is still rejected.
    harness.get_by_label("Remove finalizers").click_accesskit();
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
    harness.get_by_label("Remove finalizers").click_accesskit();
    harness.run();

    assert!(matches!(
        harness.state().worker.commands.as_slice(),
        [command] if command.as_ref().as_any().downcast_ref::<ForceDeleteResource>()
            .is_some_and(|command| command.resource_name == "important-config")
    ));
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
fn force_delete_failure_is_shown_and_can_be_dismissed() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceForceDeleteFailed {
            cluster_key: 2,
            error: "Resource was replaced while awaiting confirmation".into(),
        }) as WorkerResultBox);

    harness.run();
    harness.get_by_label("Couldn’t remove finalizers");
    harness.get_by_label("Dismiss").click_accesskit();
    harness.run();

    assert!(
        harness.state().ui_state.clusters[&2]
            .force_delete_error
            .is_none()
    );
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
            },
            Cluster {
                name: "prod".into(),
                is_current: false,
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
