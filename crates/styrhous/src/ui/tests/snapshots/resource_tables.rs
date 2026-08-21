//! Resource-table and table-action scenarios.

use super::*;

#[test]
fn pod_resource_table_shows_per_container_status_indicators() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.pod_metrics.insert(
        "kube-system".into(),
        PodMetricsNamespaceState {
            usages: BTreeMap::from([(
                "api-pod".into(),
                PodUsage {
                    timestamp: OffsetDateTime::now_utc(),
                    cpu_nanocores: 125_000_000,
                    memory_bytes: 96 * 1024 * 1024,
                    containers: BTreeMap::new(),
                },
            )]),
            error: None,
        },
    );
    cluster.resource_cache.insert(
        (pods, Some("kube-system".into())),
        ResourceWatchState {
            resources: BTreeMap::from([(
                "api-pod".into(),
                MinimalResource {
                    uid: "api-pod".into(),
                    name: "api-pod".into(),
                    namespace: Some("kube-system".into()),
                    creation_timestamp: None,
                    controller_owner: None,
                    labels: Default::default(),
                    annotations: Default::default(),
                    cells: BTreeMap::from([
                        (READY_COLUMN.to_owned(), CellValue::Text("1/2".to_owned())),
                        (
                            CONTAINERS_COLUMN.to_owned(),
                            CellValue::ContainerIndicators(vec![
                                ContainerIndicator {
                                    name: "setup".into(),
                                    kind: ContainerKind::Init,
                                    state: "Terminated".into(),
                                    reason: Some("Completed".into()),
                                    message: None,
                                    ready: false,
                                    restart_count: 0,
                                    tone: StatusTone::Success,
                                },
                                ContainerIndicator {
                                    name: "api".into(),
                                    kind: ContainerKind::App,
                                    state: "Running".into(),
                                    reason: None,
                                    message: None,
                                    ready: true,
                                    restart_count: 2,
                                    tone: StatusTone::Success,
                                },
                                ContainerIndicator {
                                    name: "sidecar".into(),
                                    kind: ContainerKind::App,
                                    state: "Waiting".into(),
                                    reason: Some("ContainerCreating".into()),
                                    message: Some("Waiting for volume mount".into()),
                                    ready: false,
                                    restart_count: 3,
                                    tone: StatusTone::Warning,
                                },
                                ContainerIndicator {
                                    name: "debugger".into(),
                                    kind: ContainerKind::Ephemeral,
                                    state: "Terminated".into(),
                                    reason: Some("Error".into()),
                                    message: None,
                                    ready: false,
                                    restart_count: 1,
                                    tone: StatusTone::Danger,
                                },
                            ]),
                        ),
                        (
                            STATUS_COLUMN.to_owned(),
                            CellValue::Status {
                                label: "Running".into(),
                                tone: StatusTone::Success,
                            },
                        ),
                        (RESTARTS_COLUMN.to_owned(), CellValue::Number(5)),
                    ]),
                    log_containers: Vec::new(),
                },
            )]),
            is_synced: true,
            ..Default::default()
        },
    );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.ui_harness("resource_tables/pod_resource_table_shows_per_container_status_indicators/pod_resource_table_container_indicators");

    harness.get_by_label("Container: sidecar").hover();
    harness.run();
    harness.ui_harness("resource_tables/pod_resource_table_shows_per_container_status_indicators/pod_resource_table_container_indicators_tooltip");
}

#[test]
fn resource_table_snapshot_keeps_namespace_column_readable() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.namespaces.insert(
        "default".into(),
        MinimalNamespace {
            name: "default".into(),
            display_name: None,
        },
    );
    cluster.selected_namespaces = HashSet::from(["default".into(), "kube-system".into()]);
    cluster.resource_cache.insert(
        (pods, Some("default".into())),
        ResourceWatchState {
            resources: BTreeMap::from([(
                "default-pod".into(),
                MinimalResource {
                    uid: "default-pod".into(),
                    name: "default-pod".into(),
                    namespace: Some("default".into()),
                    creation_timestamp: None,
                    controller_owner: None,
                    labels: Default::default(),
                    annotations: Default::default(),
                    cells: Default::default(),
                    log_containers: Vec::new(),
                },
            )]),
            is_synced: true,
            ..Default::default()
        },
    );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    harness.ui_harness("resource_tables/resource_table_snapshot_keeps_namespace_column_readable/resource_table_multiple_namespaces");
}

#[test]
fn deployment_resource_table_snapshot_uses_typed_columns() {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.selected_api_resource = Some(deployment.clone());
    cluster.resource_cache.insert(
        (deployment, Some("kube-system".to_owned())),
        ResourceWatchState {
            resources: BTreeMap::from([(
                "deployment-uid".to_owned(),
                MinimalResource {
                    uid: "deployment-uid".to_owned(),
                    name: "coredns".to_owned(),
                    namespace: Some("kube-system".to_owned()),
                    creation_timestamp: Some(
                        time::OffsetDateTime::now_utc() - time::Duration::days(220),
                    ),
                    controller_owner: None,
                    labels: Default::default(),
                    annotations: Default::default(),
                    cells: BTreeMap::from([
                        (READY_COLUMN.to_owned(), CellValue::Text("3/4".to_owned())),
                        (UP_TO_DATE_COLUMN.to_owned(), CellValue::Number(3)),
                        (AVAILABLE_COLUMN.to_owned(), CellValue::Number(3)),
                    ]),
                    log_containers: Vec::new(),
                },
            )]),
            is_synced: true,
            ..Default::default()
        },
    );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.get_by_label("Deployments").click_accesskit();
    harness.run();

    harness.ui_harness("resource_tables/deployment_resource_table_snapshot_uses_typed_columns/deployment_resource_table_typed_columns");
}

#[test]
fn deployment_restart_action_opens_a_confirmation_and_sends_a_worker_command() {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.selected_api_resource = Some(deployment.clone());
    cluster.resource_cache.insert(
        (deployment, Some("kube-system".to_owned())),
        ResourceWatchState {
            resources: BTreeMap::from([(
                "deployment-uid".to_owned(),
                MinimalResource {
                    uid: "deployment-uid".to_owned(),
                    name: "coredns".to_owned(),
                    namespace: Some("kube-system".to_owned()),
                    creation_timestamp: None,
                    controller_owner: None,
                    labels: Default::default(),
                    annotations: Default::default(),
                    cells: BTreeMap::new(),
                    log_containers: Vec::new(),
                },
            )]),
            is_synced: true,
            error: None,
        },
    );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.get_by_label("Deployments").click_accesskit();
    harness.run();
    harness
        .get_by_label("More actions for coredns")
        .click_accesskit();
    harness.run();
    harness.get_by_label("Restart rollout").click_accesskit();
    harness.run();

    harness.ui_harness("resource_actions/deployment_restart_action_opens_a_confirmation_and_sends_a_worker_command/deployment_restart_confirmation");
    harness.get_by_label("Restart rollout").click_accesskit();
    harness.run();
    assert!(
        harness
            .state()
            .worker
            .last_command::<RestartDeployment>()
            .is_some_and(|command| command.cluster_key == 2
                && command.namespace == "kube-system"
                && command.resource_name == "coredns")
    );
}

#[test]
fn cron_job_run_action_opens_a_confirmation_and_sends_a_worker_command() {
    let cron_job = fixture_api_resource("batch", "CronJob", "cronjobs");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.selected_api_resource = Some(cron_job.clone());
    cluster.resource_navigation = build_resource_navigation(vec![cron_job.clone()]);
    cluster.resource_cache.insert(
        (cron_job, Some("kube-system".to_owned())),
        ResourceWatchState {
            resources: BTreeMap::from([(
                "cron-job-uid".to_owned(),
                MinimalResource {
                    uid: "cron-job-uid".to_owned(),
                    name: "nightly-report".to_owned(),
                    namespace: Some("kube-system".to_owned()),
                    creation_timestamp: None,
                    controller_owner: None,
                    labels: Default::default(),
                    annotations: Default::default(),
                    cells: BTreeMap::new(),
                    log_containers: Vec::new(),
                },
            )]),
            is_synced: true,
            error: None,
        },
    );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click();
    harness.run();
    harness.get_by_label("Cron Jobs").click();
    harness.run();
    harness
        .get_by_label("More actions for nightly-report")
        .click();
    harness.run();
    harness.get_by_label("Run now").click();
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();

    harness.ui_harness("resource_actions/cron_job_run_action_opens_a_confirmation_and_sends_a_worker_command/cron_job_run_confirmation");
    harness.get_by_label("Run now").click();
    harness.run();
    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .and_then(|command| command.as_ref().as_any().downcast_ref::<RunCronJob>())
            .is_some_and(|command| command.cluster_key == 2
                && command.namespace == "kube-system"
                && command.resource_name == "nightly-report")
    );
}

#[test]
fn scalable_resource_action_fetches_and_updates_the_scale() {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let setup_state = || {
        let mut state = oracle_resource_table_state();
        let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
        cluster.selected_api_resource = Some(deployment.clone());
        cluster.scalable_api_resources.insert(deployment.clone());
        cluster.resource_cache.insert(
            (deployment.clone(), Some("kube-system".to_owned())),
            ResourceWatchState {
                resources: BTreeMap::from([(
                    "deployment-uid".to_owned(),
                    MinimalResource {
                        uid: "deployment-uid".to_owned(),
                        name: "coredns".to_owned(),
                        namespace: Some("kube-system".to_owned()),
                        creation_timestamp: None,
                        controller_owner: None,
                        labels: Default::default(),
                        annotations: Default::default(),
                        cells: BTreeMap::new(),
                        log_containers: Vec::new(),
                    },
                )]),
                is_synced: true,
                ..Default::default()
            },
        );
        state
    };

    let mut snapshot_harness = application_harness::<MockWorker>();
    snapshot_harness.state_mut().ui_state = setup_state();
    snapshot_harness.run();
    snapshot_harness
        .get_by_label("Apps & Containers")
        .click_accesskit();
    snapshot_harness.run();
    snapshot_harness
        .get_by_label("Deployments")
        .click_accesskit();
    snapshot_harness.run();
    snapshot_harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .pending_scale = Some(super::super::super::state::PendingScale {
        api_resource: deployment.clone(),
        resource_name: "coredns".into(),
        namespace: Some("kube-system".into()),
        current_replicas: 3,
        desired_replicas: "3".into(),
    });
    snapshot_harness.run();
    snapshot_harness.ui_harness("resource_actions/scalable_resource_action_fetches_and_updates_the_scale/resource_scale_dialog");

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = setup_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click();
    harness.run();
    harness.get_by_label("Deployments").click();
    harness.run();
    harness.get_by_label("More actions for coredns").click();
    harness.run();
    harness.get_by_label("Scale").click();
    harness.run();

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .and_then(|command| command.as_ref().as_any().downcast_ref::<GetResourceScale>())
            .is_some_and(|command| command.cluster_key == 2
                && command.api_resource == deployment
                && command.namespace.as_deref() == Some("kube-system")
                && command.resource_name == "coredns")
    );

    harness.deliver_worker_result(ResourceScaleFetched {
        cluster_key: 2,
        api_resource: deployment.clone(),
        namespace: Some("kube-system".into()),
        resource_name: "coredns".into(),
        replicas: 3,
    });
    harness.get_by_label("Increase desired replicas").click();
    harness.run();

    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .pending_scale
            .as_ref()
            .map(|pending| pending.desired_replicas.as_str()),
        Some("4"),
        "the pointer click must update the visible scale dialog before submitting it"
    );

    harness.get_by_label("Update scale").click();
    harness.run();

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .and_then(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<UpdateResourceScale>())
            .is_some_and(|command| command.cluster_key == 2
                && command.api_resource == deployment
                && command.namespace.as_deref() == Some("kube-system")
                && command.resource_name == "coredns"
                && command.replicas == 4)
    );
}

#[test]
fn resource_search_filters_rows_and_restores_its_mode_per_resource_type() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let nodes = fixture_api_resource("core", "Node", "nodes");
    let mut state = oracle_resource_table_state();
    state.clusters.get_mut(&2).unwrap().resource_cache.insert(
        (nodes, Some("kube-system".into())),
        ResourceWatchState {
            is_synced: true,
            ..Default::default()
        },
    );
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("Search resources").click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("c66z".into()));
    harness.run();

    harness.get_by_label("Open details for coredns-66bc5c9577-z9gt9");
    harness.get_by_label("7 resources hidden by search");
    assert_eq!(
        harness.state().ui_state.clusters[&2].resource_searches[&pods].query,
        "c66z"
    );

    harness.get_by_label("Use regex search").click();
    harness.run();
    assert!(harness.state().ui_state.clusters[&2].resource_searches[&pods].regex_mode);
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_searches
        .insert(
            pods.clone(),
            super::super::super::state::ResourceSearchState {
                query: "COREDNS.*Z9".into(),
                regex_mode: true,
            },
        );
    harness.run();
    harness.get_by_label("Open details for coredns-66bc5c9577-z9gt9");
    harness.get_by_label("7 resources hidden by search");
    harness.ui_harness("resource_tables/resource_search_filters_rows_and_restores_its_mode_per_resource_type/resource_search_filtered");

    harness.get_by_label("Nodes").click_accesskit();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.get_by_label("Pods").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&2].resource_searches[&pods].query,
        "COREDNS.*Z9"
    );
    assert!(harness.state().ui_state.clusters[&2].resource_searches[&pods].regex_mode);
}

#[test]
fn invalid_resource_search_regex_is_shown_in_workspace() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_searches
        .insert(
            pods,
            super::super::super::state::ResourceSearchState {
                query: "coredns[a-z".into(),
                regex_mode: true,
            },
        );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("Invalid regular expression");
    harness.ui_harness("resource_tables/invalid_resource_search_regex_is_shown_in_workspace/resource_search_invalid_regex");
}
