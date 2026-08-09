use super::super::MyEguiApp;
use super::super::state::ClusterConnectionState;
use super::super::state::{PendingDelete, ResourceWatchState, UiState};
use super::fixtures::{
    application_harness, application_harness_with_terminal, fixture_api_resource, fixture_cluster,
    fixture_cluster_scoped_api_resource, oracle_resource_table_state,
};
use crate::cluster_connection_manager::Cluster;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::{MinimalResource, PodLogContainer};
use crate::resource_catalog::build_resource_navigation;
use crate::resource_detail::{
    ConfigMapDetail, ManagedResource, ManagedResourceAssociation, NodeDetail, PodConditionDetail,
    PodContainerDetail, PodDetail, PodEnvironmentVariableDetail, PodEnvironmentVariableSource,
    PodVolumeDetail, ResourceDetail, ResourceDetailPayload, ResourceEvent, SecretDataDetail,
    SecretDetail,
};
use crate::resource_table::{
    AVAILABLE_COLUMN, CONTAINERS_COLUMN, CellValue, ContainerIndicator, ContainerKind, NODE_COLUMN,
    READY_COLUMN, RESTARTS_COLUMN, STATUS_COLUMN, StatusTone, UP_TO_DATE_COLUMN,
};
use crate::terminal_launcher::{TerminalLaunchSettings, test_support::MockTerminalLauncher};
use crate::worker::{MockWorker, WorkerCommand, WorkerResult};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::collections::{BTreeMap, HashMap, HashSet};

// The verified WGPU variance at transformed blade-shadow edges has a maximum per-pixel
// YIQ-squared distance of 1.84634. This threshold accepts only that microscopic rasterization
// noise; it does not permit any count of larger differences.
const TRANSFORMED_BLADE_PIXEL_THRESHOLD: f32 = 2.1;

fn transformed_blade_snapshot_options() -> egui_kittest::SnapshotOptions {
    egui_kittest::SnapshotOptions::new().threshold(TRANSFORMED_BLADE_PIXEL_THRESHOLD)
}

fn select_namespace(harness: &mut Harness<MyEguiApp<MockWorker>>, namespace: &str) {
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run_steps(2);
    harness.get_by_label(namespace).click();
    harness.run_steps(2);
}

fn overflowing_pod_detail() -> PodDetail {
    PodDetail {
        phase: "Running".into(),
        conditions: vec![
            PodConditionDetail {
                type_: "Initialized".into(),
                status: "True".into(),
                reason: Some("PodCompleted".into()),
                message: Some("All init containers have completed.".into()),
            },
            PodConditionDetail {
                type_: "Ready".into(),
                status: "True".into(),
                reason: Some("ContainersReady".into()),
                message: Some("All containers are ready.".into()),
            },
        ],
        node_name: Some("kind-control-plane".into()),
        pod_ip: Some("10.244.0.23".into()),
        host_ip: Some("172.18.0.2".into()),
        qos_class: Some("Burstable".into()),
        restart_policy: Some("Always".into()),
        service_account_name: Some("api".into()),
        dns_policy: Some("ClusterFirst".into()),
        containers: (0..3)
            .map(|index| PodContainerDetail {
                name: format!("api-{index}"),
                image: "registry.example.com/api:v1.2.3".into(),
                ready: true,
                restart_count: 0,
                state: "Running".into(),
                reason: None,
                message: None,
                command: vec!["/app/api".into()],
                args: vec!["--serve".into(), "--metrics-address=:9090".into()],
                ports: vec!["8080/TCP".into(), "9090/TCP".into()],
                environment_variables: vec![
                    PodEnvironmentVariableDetail {
                        name: "LOG_LEVEL".into(),
                        value: Some("info".into()),
                        source: PodEnvironmentVariableSource::Literal,
                    },
                    PodEnvironmentVariableDetail {
                        name: "DATABASE_URL".into(),
                        value: Some("postgresql://database/api".into()),
                        source: PodEnvironmentVariableSource::SecretKey {
                            name: "api-database".into(),
                            key: "url".into(),
                            optional: false,
                        },
                    },
                ],
            })
            .collect(),
        log_containers: Vec::new(),
        volumes: (0..3)
            .map(|index| PodVolumeDetail {
                name: format!("config-{index}"),
                kind: "ConfigMap".into(),
                source: "api-configuration".into(),
                mount_path: Some(format!("/etc/api/config-{index}")),
                read_only: true,
            })
            .collect(),
    }
}

fn secondary_click(harness: &mut Harness<MyEguiApp<MockWorker>>, position: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(position));
    harness.event(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.event(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
}

fn primary_click(harness: &mut Harness<MyEguiApp<MockWorker>>, position: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(position));
    harness.event(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.event(egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
}

#[test]
fn namespace_selector_replaces_toggles_and_selects_all_without_stopping_watches() {
    let pods = fixture_api_resource("", "Pod", "pods");
    let mut cluster = fixture_cluster(1, "dev");
    cluster.connection = ClusterConnectionState::Connected(None);
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
    harness.input_mut().modifiers = modifiers;
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
    harness.input_mut().modifiers = egui::Modifiers::default();
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
        cluster
            .active_watchers
            .contains(&(pods.clone(), Some("kube-system".to_owned())))
    );
    assert!(
        cluster
            .active_watchers
            .contains(&(pods.clone(), Some("monitoring".to_owned())))
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, WorkerCommand::StartResourceWatch { .. }))
            .count(),
        3
    );
}

#[test]
fn cluster_scoped_resources_load_once_without_a_namespace_selection() {
    let nodes = fixture_cluster_scoped_api_resource("core", "Node", "nodes");
    let mut cluster = fixture_cluster(1, "dev");
    cluster.connection = ClusterConnectionState::Connected(None);
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
    assert!(matches!(
        harness.state().worker.commands.as_slice(),
        [WorkerCommand::StartResourceWatch {
            cluster_key: 1,
            api_resource,
            namespace: None,
        }] if api_resource == &nodes
    ));
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesResourcesReplaced {
            cluster_key: 1,
            api_resource: nodes.clone(),
            namespace: None,
            resources: vec![MinimalResource {
                uid: "node-uid".into(),
                name: "kind-control-plane".into(),
                namespace: None,
                creation_timestamp: None,
                cells: Default::default(),
                log_containers: Vec::new(),
            }],
        });
    harness.run();
    harness.get_by_label("Cluster-wide");
    harness.get_by_label("kind-control-plane");

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
            .commands
            .iter()
            .filter(|command| matches!(command, WorkerCommand::StartResourceWatch { .. }))
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
    harness.state_mut().ui_state = state;
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("m".into()));
    harness.run();
    harness.snapshot("namespace_selector_open_filtered_active_watches");
}

#[test]
fn no_current_context_leaves_cluster_selection_manual() {
    let mut harness = application_harness::<MockWorker>();
    harness.run();
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesClustersUpdated(vec![Cluster {
            name: "dev".into(),
            cluster: None,
            is_current: false,
        }]));

    harness.run();

    assert_eq!(harness.state().ui_state.selected_cluster, None);
    assert!(harness.state().worker.commands.is_empty());
}

#[test]
fn oracle_resource_table_snapshot_uses_injected_cluster_state() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.snapshot("oracle_resource_table_injected");
}

#[test]
fn resource_navigation_uses_the_persisted_expansion_state() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    for node_id in ["section:Apps & Containers"] {
        state.set_resource_navigation_node_expanded(node_id, true);
    }
    harness.state_mut().ui_state = state;

    harness.run();

    harness.get_by_label("Pods");
}

#[test]
fn pod_resource_table_shows_per_container_status_indicators() {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
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
            error: None,
        },
    );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.snapshot("pod_resource_table_container_indicators");

    harness.get_by_label("Container: sidecar").hover();
    harness.run();
    harness.snapshot("pod_resource_table_container_indicators_tooltip");
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
                    cells: Default::default(),
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

    harness.snapshot("resource_table_multiple_namespaces");
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
                    cells: BTreeMap::from([
                        (READY_COLUMN.to_owned(), CellValue::Text("3/4".to_owned())),
                        (UP_TO_DATE_COLUMN.to_owned(), CellValue::Number(3)),
                        (AVAILABLE_COLUMN.to_owned(), CellValue::Number(3)),
                    ]),
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

    harness.snapshot("deployment_resource_table_typed_columns");
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

    harness.snapshot("deployment_restart_confirmation");
    harness.get_by_label("Restart rollout").click_accesskit();
    harness.run();
    assert!(matches!(
        harness.state().worker.commands.last(),
        Some(WorkerCommand::RestartDeployment {
            cluster_key: 2,
            namespace,
            resource_name,
        }) if namespace == "kube-system" && resource_name == "coredns"
    ));
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

    harness.get_by_label("coredns-66bc5c9577-z9gt9");
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
            super::super::state::ResourceSearchState {
                query: "COREDNS.*Z9".into(),
                regex_mode: true,
            },
        );
    harness.run();
    harness.get_by_label("coredns-66bc5c9577-z9gt9");
    harness.get_by_label("7 resources hidden by search");
    harness.snapshot("resource_search_filtered");

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
            super::super::state::ResourceSearchState {
                query: "coredns[a-z".into(),
                regex_mode: true,
            },
        );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("Invalid regular expression");
    harness.snapshot("resource_search_invalid_regex");
}

#[test]
fn resource_table_more_actions_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness
        .get_by_label("More actions for coredns-66bc5c9577-ffw2s")
        .click_accesskit();
    harness.run();
    harness.snapshot("oracle_resource_table_actions");
}

#[test]
fn resource_table_row_context_menu_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let resource_name_rect = harness.get_by_label("coredns-66bc5c9577-ffw2s").rect();
    let click_position = egui::pos2(
        resource_name_rect.right() + 32.0,
        resource_name_rect.center().y,
    );
    secondary_click(&mut harness, click_position);
    harness.run();

    harness.get_by_label("Edit");
    harness.snapshot("oracle_resource_table_row_context_actions");
}

#[test]
fn resource_table_row_context_menu_opens_when_right_clicking_resource_text() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let text_position = harness
        .get_by_label("coredns-66bc5c9577-ffw2s")
        .rect()
        .center();
    secondary_click(&mut harness, text_position);
    harness.run();

    harness.get_by_label("Edit");
}

#[test]
fn shell_action_launches_the_selected_context_pod_and_application_container() {
    let mut harness = application_harness_with_terminal::<MockWorker, MockTerminalLauncher>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let resource = state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .unwrap()
        .resources
        .values_mut()
        .next()
        .unwrap();
    resource.log_containers = vec![PodLogContainer {
        name: "coredns".into(),
        kind: ContainerKind::App,
    }];
    let pod_name = resource.name.clone();
    harness.state_mut().ui_state = state;
    harness.run();

    let action_label = format!("More actions for {pod_name}");
    harness.get_by_label(&action_label).click_accesskit();
    harness.run();
    harness.get_by_label("Shell").click_accesskit();
    harness.run();

    assert_eq!(
        harness.state().terminal_launcher.requests.as_slice(),
        &[crate::terminal_launcher::PodShellRequest {
            kube_context: "kind-kind".into(),
            namespace: "kube-system".into(),
            pod_name,
            container: "coredns".into(),
        }]
    );
}

#[test]
fn shell_launch_failure_uses_the_styled_error_modal_and_opens_terminal_settings() {
    let mut harness = application_harness_with_terminal::<MockWorker, MockTerminalLauncher>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let resource = state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .unwrap()
        .resources
        .values_mut()
        .next()
        .unwrap();
    resource.log_containers = vec![PodLogContainer {
        name: "coredns".into(),
        kind: ContainerKind::App,
    }];
    let pod_name = resource.name.clone();
    harness.state_mut().ui_state = state;
    harness.state_mut().terminal_launcher.failure = Some(
        "No supported terminal launcher was found. Tried: xdg-terminal-exec (No such file or directory (os error 2))."
            .into(),
    );
    harness.run();

    let action_label = format!("More actions for {pod_name}");
    harness.get_by_label(&action_label).click_accesskit();
    harness.run();
    harness.get_by_label("Shell").click_accesskit();
    harness.run_steps(2);

    harness.get_by_label("POD SHELL");
    harness.get_by_label("Couldn’t open a terminal");
    harness.get_by_label(
        "No supported terminal launcher was found. Tried: xdg-terminal-exec (No such file or directory (os error 2)).",
    );
    harness.get_by_label("Open settings");
    harness.get_by_label("Dismiss");
    assert!(harness.state().ui_state.terminal_launch_error.is_some());
    harness.run_steps(2);
    harness.snapshot_options(
        "terminal_launch_error",
        &egui_kittest::SnapshotOptions::new().failed_pixel_count_threshold(1),
    );

    harness.get_by_label("Open settings").click_accesskit();
    harness.run_steps(2);

    assert!(harness.state().ui_state.terminal_launch_error.is_none());
    assert!(harness.state().ui_state.terminal_settings_open);
    harness.get_by_label("Terminal launcher");
}

#[test]
fn terminal_launch_error_dismisses_without_opening_settings() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_launch_error = Some("Unable to start xterm: permission denied".into());
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("Dismiss").click_accesskit();
    harness.run();

    assert!(harness.state().ui_state.terminal_launch_error.is_none());
    assert!(!harness.state().ui_state.terminal_settings_open);
}

#[test]
fn settings_button_opens_the_terminal_launcher_blade() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();

    harness.get_by_label("Settings").click_accesskit();
    harness.run();

    harness.get_by_label("Terminal launcher");
    harness.get_by_label("Save changes");
    harness.snapshot("settings_terminal_launcher");
}

#[test]
fn settings_blade_shows_custom_terminal_launcher_details() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_settings_open = true;
    state.terminal_settings_draft = TerminalLaunchSettings {
        custom_template: Some("alacritty -e {command}".into()),
    };
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "Command template");
    harness.get_by_label("Save changes");
    harness.snapshot("settings_terminal_launcher_custom");
}

#[test]
fn settings_blade_shows_invalid_custom_template_after_save() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();

    harness.get_by_label("Settings").click_accesskit();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::RadioButton, "Custom launcher")
        .click_accesskit();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Command template")
        .click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("alacritty".into()));
    harness.run();
    assert_eq!(
        harness
            .state()
            .ui_state
            .terminal_settings_draft
            .custom_template,
        Some("alacritty".into())
    );
    harness.get_by_label("Save changes").click_accesskit();
    harness.run();

    assert_eq!(
        harness.state().ui_state.terminal_settings_error.as_deref(),
        Some("The launcher template must contain exactly one {command} placeholder.")
    );
    harness.get_by_label("Command template needs attention");
    harness.snapshot("settings_terminal_launcher_invalid");
}

#[test]
fn resource_name_opens_and_closes_a_live_detail_inspector() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let name = "coredns-66bc5c9577-ffw2s";
    let resource_position = harness.get_by_label(name).rect().center();
    primary_click(&mut harness, resource_position);
    harness.run_steps(1);

    assert!(
        matches!(
            harness.state().worker.commands.last(),
            Some(WorkerCommand::StartResourceDetailWatch {
                cluster_key: 2,
                resource_name,
                resource_uid,
                history_entry_id: 1,
                ..
            }) if resource_name == name && resource_uid == "fixture-0"
        ),
        "commands: {:?}",
        harness.state().worker.commands
    );

    let pods = fixture_api_resource("core", "Pod", "pods");
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: ResourceDetail {
                api_resource: pods,
                name: name.into(),
                namespace: Some("kube-system".into()),
                uid: "fixture-0".into(),
                resource_version: "1".into(),
                creation_timestamp: None,
                owner: None,
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Generic,
            },
        });
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .as_ref()
            .and_then(|panel| panel.detail.as_ref())
            .is_some()
    );
    harness.ctx.style_mut(|style| style.animation_time = 1.0);
    harness.get_by_label("Close blade").click_accesskit();
    harness.run_steps(2);

    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_some(),
        "the inspector remains present while its close animation is in progress"
    );
    harness.ctx.style_mut(|style| style.animation_time = 0.0);
    harness.run();

    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none()
    );
    assert!(matches!(
        harness.state().worker.commands.last(),
        Some(WorkerCommand::StopResourceDetailWatch { cluster_key: 2, .. })
    ));
}

#[test]
fn clicking_a_pod_node_in_the_resource_table_opens_the_node_inspector() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    let pod = cluster
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .expect("Pod watch fixture exists")
        .resources
        .values_mut()
        .next()
        .expect("Pod fixture exists");
    pod.cells.insert(
        NODE_COLUMN.into(),
        CellValue::Text("kind-control-plane".into()),
    );
    harness.state_mut().ui_state = state;
    harness.run();

    let node_position = harness
        .get_by_label("Open details for Node kind-control-plane")
        .rect()
        .center();
    primary_click(&mut harness, node_position);
    harness.run_steps(1);

    assert!(matches!(
        harness.state().worker.commands.last(),
        Some(WorkerCommand::StartResourceDetailWatch {
            api_resource,
            namespace: None,
            resource_name,
            resource_uid,
            ..
        }) if api_resource == &crate::resource_handlers::node::api_resource()
            && resource_name == "kind-control-plane"
            && resource_uid == "kind-control-plane"
    ));
}

#[test]
fn clicking_a_pod_node_in_the_inspector_navigates_to_the_node_inspector() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut commands = Vec::new();
    harness.state_mut().ui_state.open_resource_detail(
        2,
        pods.clone(),
        "api".into(),
        Some("kube-system".into()),
        "pod-uid".into(),
        &mut commands,
    );
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: ResourceDetail {
                api_resource: pods,
                name: "api".into(),
                namespace: Some("kube-system".into()),
                uid: "pod-uid".into(),
                resource_version: "1".into(),
                creation_timestamp: None,
                owner: None,
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Pod(overflowing_pod_detail()),
            },
        });
    harness.run_steps(2);

    let node_position = harness
        .get_by_label("Open details for Node kind-control-plane")
        .rect()
        .center();
    primary_click(&mut harness, node_position);
    harness.run_steps(1);

    assert!(matches!(
        harness.state().worker.commands.last(),
        Some(WorkerCommand::StartResourceDetailWatch {
            api_resource,
            namespace: None,
            resource_name,
            resource_uid,
            history_entry_id: 2,
            ..
        }) if api_resource == &crate::resource_handlers::node::api_resource()
            && resource_name == "kind-control-plane"
            && resource_uid == "kind-control-plane"
    ));
}

#[test]
fn node_inspector_shows_its_spec() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let mut commands = Vec::new();
    harness.state_mut().ui_state.open_resource_detail(
        2,
        crate::resource_handlers::node::api_resource(),
        "kind-control-plane".into(),
        None,
        "node-uid".into(),
        &mut commands,
    );
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: ResourceDetail {
                api_resource: crate::resource_handlers::node::api_resource(),
                name: "kind-control-plane".into(),
                namespace: None,
                uid: "node-uid".into(),
                resource_version: "1".into(),
                creation_timestamp: None,
                owner: None,
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Node(NodeDetail {
                    pod_cidrs: vec!["10.244.0.0/24".into()],
                    provider_id: Some("kind://docker/kind/kind-control-plane".into()),
                    unschedulable: true,
                    taints: vec!["node-role.kubernetes.io/control-plane:NoSchedule".into()],
                }),
            },
        });
    harness.run_steps(2);

    harness.get_by_label("Spec");
    harness.get_by_label("Scheduling disabled");
    harness.get_by_label("10.244.0.0/24");
    harness.get_by_label("node-role.kubernetes.io/control-plane:NoSchedule");
}

#[test]
fn node_inspector_lists_cross_namespace_pods_in_the_shared_pod_table() {
    let mut harness = application_harness::<MockWorker>();
    harness.ctx.style_mut(|style| style.animation_time = 0.0);
    harness.state_mut().ui_state = oracle_resource_table_state();
    let nodes = crate::resource_handlers::node::api_resource();
    let mut commands = Vec::new();
    harness.state_mut().ui_state.open_resource_detail(
        2,
        nodes.clone(),
        "kind-control-plane".into(),
        None,
        "node-uid".into(),
        &mut commands,
    );
    harness.state_mut().worker.results.extend([
        WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: ResourceDetail {
                api_resource: nodes,
                name: "kind-control-plane".into(),
                namespace: None,
                uid: "node-uid".into(),
                resource_version: "1".into(),
                creation_timestamp: None,
                owner: None,
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Node(NodeDetail::default()),
            },
        },
        WorkerResult::ManagedResourcesReplaced {
            cluster_key: 2,
            history_entry_id: 1,
            resources: vec![ManagedResource {
                api_resource: fixture_api_resource("core", "Pod", "pods"),
                name: "api".into(),
                namespace: Some("monitoring".into()),
                uid: "pod-uid".into(),
                association: ManagedResourceAssociation::NodeName("kind-control-plane".into()),
                creation_timestamp: Some(
                    time::OffsetDateTime::now_utc() - time::Duration::minutes(15),
                ),
                cells: BTreeMap::from([
                    (READY_COLUMN.into(), CellValue::Text("1/1".into())),
                    (
                        STATUS_COLUMN.into(),
                        CellValue::Status {
                            label: "Running".into(),
                            tone: StatusTone::Success,
                        },
                    ),
                    (RESTARTS_COLUMN.into(), CellValue::Number(0)),
                    (
                        NODE_COLUMN.into(),
                        CellValue::Text("kind-control-plane".into()),
                    ),
                ]),
            }],
        },
    ]);
    harness.run_steps(2);

    harness.snapshot_options(
        "node_inspector_with_scheduled_pods",
        &egui_kittest::SnapshotOptions::new().failed_pixel_count_threshold(1),
    );
    harness.get_by_label("monitoring");
    harness.get_by_label("api");
}

#[test]
fn clicking_a_history_blade_returns_to_that_history_entry() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let replica_set = fixture_api_resource("apps", "ReplicaSet", "replicasets");
    let mut commands = Vec::new();
    harness.state_mut().ui_state.open_resource_detail(
        2,
        deployment.clone(),
        "api".into(),
        Some("kube-system".into()),
        "deployment-uid".into(),
        &mut commands,
    );
    harness.run_steps(2);
    harness.state_mut().ui_state.navigate_resource_detail(
        2,
        replica_set,
        "api-7b948f".into(),
        Some("kube-system".into()),
        "replicaset-uid".into(),
        &mut commands,
    );
    harness.run_steps(2);

    harness.get_by_label("Go back one blade").click();
    harness.run_steps(4);

    let panel = harness.state().ui_state.clusters[&2]
        .resource_detail_panel
        .as_ref()
        .expect("history blade click must not dismiss the detail panel");
    assert_eq!(panel.navigator.current().api_resource, deployment);
}

#[test]
fn promoted_history_blade_stays_above_its_back_history() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let mut commands = Vec::new();

    harness.state_mut().ui_state.open_resource_detail(
        2,
        deployment.clone(),
        "first".into(),
        Some("kube-system".into()),
        "first-uid".into(),
        &mut commands,
    );
    harness
        .state_mut()
        .worker
        .commands
        .extend(commands.drain(..));
    harness.run_steps(1);
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_detail_panel
        .as_mut()
        .unwrap()
        .navigator
        .clear_transition();
    harness.run_steps(1);

    for (name, uid) in [("second", "second-uid"), ("third", "third-uid")] {
        harness.state_mut().ui_state.navigate_resource_detail(
            2,
            deployment.clone(),
            name.into(),
            Some("kube-system".into()),
            uid.into(),
            &mut commands,
        );
        harness
            .state_mut()
            .worker
            .commands
            .extend(commands.drain(..));
        harness
            .state_mut()
            .ui_state
            .clusters
            .get_mut(&2)
            .unwrap()
            .resource_detail_panel
            .as_mut()
            .unwrap()
            .navigator
            .clear_transition();
        harness.run_steps(1);
    }

    for forward in [false, false, true] {
        harness
            .state_mut()
            .ui_state
            .navigate_resource_detail_history(2, forward, &mut commands);
        harness
            .state_mut()
            .worker
            .commands
            .extend(commands.drain(..));
        harness
            .state_mut()
            .ui_state
            .clusters
            .get_mut(&2)
            .unwrap()
            .resource_detail_panel
            .as_mut()
            .unwrap()
            .navigator
            .clear_transition();
        harness.run_steps(1);
    }

    let panel = harness.state().ui_state.clusters[&2]
        .resource_detail_panel
        .as_ref()
        .expect("inspector should remain open");
    assert_eq!(panel.resource_name, "second");
    assert_eq!(panel.navigator.back_stack().len(), 1);
    let active_layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("resource-detail-blade").with(("blade", panel.navigator.back_stack().len())),
    );
    let active_header = harness.get_by_label("Back").rect().center();
    assert_eq!(harness.ctx.layer_id_at(active_header), Some(active_layer));
}

#[test]
fn managed_resource_tables_navigate_with_back_and_forward_history() {
    let mut harness = application_harness::<MockWorker>();
    harness.ctx.style_mut(|style| style.animation_time = 0.0);
    harness.state_mut().ui_state = oracle_resource_table_state();
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let replica_set = fixture_api_resource("apps", "ReplicaSet", "replicasets");
    let pod = fixture_api_resource("core", "Pod", "pods");
    let detail = ResourceDetail {
        api_resource: deployment.clone(),
        name: "api".into(),
        namespace: Some("kube-system".into()),
        uid: "deployment-uid".into(),
        resource_version: "1".into(),
        creation_timestamp: None,
        owner: None,
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Generic,
    };
    open_typed_detail(&mut harness, deployment, detail);
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ManagedResourcesReplaced {
            cluster_key: 2,
            history_entry_id: 1,
            resources: vec![
                ManagedResource {
                    api_resource: replica_set.clone(),
                    name: "api-7b948f".into(),
                    namespace: Some("kube-system".into()),
                    uid: "replicaset-uid".into(),
                    association: ManagedResourceAssociation::ControllerOwnerUid(
                        "deployment-uid".into(),
                    ),
                    creation_timestamp: Some(
                        time::OffsetDateTime::now_utc() - time::Duration::hours(2),
                    ),
                    cells: BTreeMap::from([(
                        READY_COLUMN.to_owned(),
                        CellValue::Text("1/1".into()),
                    )]),
                },
                ManagedResource {
                    api_resource: pod.clone(),
                    name: "api-7b948f-pod".into(),
                    namespace: Some("kube-system".into()),
                    uid: "pod-uid".into(),
                    association: ManagedResourceAssociation::ControllerOwnerUid(
                        "replicaset-uid".into(),
                    ),
                    creation_timestamp: Some(
                        time::OffsetDateTime::now_utc() - time::Duration::minutes(15),
                    ),
                    cells: BTreeMap::from([
                        (READY_COLUMN.to_owned(), CellValue::Text("1/1".into())),
                        (
                            CONTAINERS_COLUMN.to_owned(),
                            CellValue::ContainerIndicators(vec![]),
                        ),
                        (
                            STATUS_COLUMN.to_owned(),
                            CellValue::Status {
                                label: "Running".into(),
                                tone: StatusTone::Success,
                            },
                        ),
                        (RESTARTS_COLUMN.to_owned(), CellValue::Number(0)),
                    ]),
                },
            ],
        });
    harness.run();
    harness.snapshot_options(
        "deployment_managed_resource_tables",
        &egui_kittest::SnapshotOptions::new().failed_pixel_count_threshold(1),
    );
    let replica_set_position = harness.get_by_label("api-7b948f").rect().center();
    primary_click(&mut harness, replica_set_position);
    harness.run_steps(1);

    let panel = harness.state().ui_state.clusters[&2]
        .resource_detail_panel
        .as_ref()
        .expect("inspector should remain open");
    assert_eq!(panel.api_resource, replica_set);
    assert_eq!(panel.navigator.back_stack().len(), 1);
    assert!(panel.navigator.forward_stack().is_empty());
    assert!(matches!(
        harness.state().worker.commands.last(),
        Some(WorkerCommand::StartResourceDetailWatch {
            resource_name,
            resource_uid,
            history_entry_id: 2,
            ..
        }) if resource_name == "api-7b948f" && resource_uid == "replicaset-uid"
    ));

    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 2,
            detail: ResourceDetail {
                api_resource: replica_set.clone(),
                name: "api-7b948f".into(),
                namespace: Some("kube-system".into()),
                uid: "replicaset-uid".into(),
                resource_version: "1".into(),
                creation_timestamp: Some(
                    time::OffsetDateTime::now_utc() - time::Duration::hours(2),
                ),
                owner: Some(crate::resource_detail::ResourceOwner {
                    kind: "Deployment".into(),
                    name: "api".into(),
                }),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Generic,
            },
        });
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ManagedResourcesReplaced {
            cluster_key: 2,
            history_entry_id: 2,
            resources: vec![ManagedResource {
                api_resource: pod.clone(),
                name: "api-7b948f-pod".into(),
                namespace: Some("kube-system".into()),
                uid: "pod-uid".into(),
                association: ManagedResourceAssociation::ControllerOwnerUid(
                    "replicaset-uid".into(),
                ),
                creation_timestamp: Some(
                    time::OffsetDateTime::now_utc() - time::Duration::hours(2),
                ),
                cells: BTreeMap::from([
                    (READY_COLUMN.to_owned(), CellValue::Text("1/1".into())),
                    (
                        CONTAINERS_COLUMN.to_owned(),
                        CellValue::ContainerIndicators(vec![]),
                    ),
                    (
                        STATUS_COLUMN.to_owned(),
                        CellValue::Status {
                            label: "Running".into(),
                            tone: StatusTone::Success,
                        },
                    ),
                    (RESTARTS_COLUMN.to_owned(), CellValue::Number(0)),
                ]),
            }],
        });
    harness.run();
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_detail_panel
        .as_mut()
        .unwrap()
        .navigator
        .clear_transition();
    harness.run();
    harness.snapshot_options(
        "replica_set_inspector_with_back_history",
        &transformed_blade_snapshot_options(),
    );

    harness.get_by_label("Back").click_accesskit();
    harness.run_steps(1);
    let panel = harness.state().ui_state.clusters[&2]
        .resource_detail_panel
        .as_ref()
        .expect("inspector should remain open");
    assert_eq!(panel.api_resource.kind, "Deployment");
    assert!(panel.navigator.back_stack().is_empty());
    assert_eq!(panel.navigator.forward_stack().len(), 1);
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_detail_panel
        .as_mut()
        .unwrap()
        .navigator
        .clear_transition();
    harness.run();
    harness.snapshot_options(
        "deployment_inspector_with_forward_history",
        &egui_kittest::SnapshotOptions::new().failed_pixel_count_threshold(1),
    );

    harness.get_by_label("Forward").click_accesskit();
    harness.run_steps(1);
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .as_ref()
            .expect("inspector should remain open")
            .api_resource,
        replica_set
    );

    let mut commands = Vec::new();
    harness.state_mut().ui_state.navigate_resource_detail(
        2,
        pod.clone(),
        "api-7b948f-pod".into(),
        Some("kube-system".into()),
        "pod-uid".into(),
        &mut commands,
    );
    harness.state_mut().worker.commands.extend(commands);
    let history_entry_id = harness.state().ui_state.clusters[&2]
        .resource_detail_panel
        .as_ref()
        .unwrap()
        .history_entry_id;
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id,
            detail: ResourceDetail {
                api_resource: pod.clone(),
                name: "api-7b948f-pod".into(),
                namespace: Some("kube-system".into()),
                uid: "pod-uid".into(),
                resource_version: "1".into(),
                creation_timestamp: Some(
                    time::OffsetDateTime::now_utc() - time::Duration::minutes(15),
                ),
                owner: Some(crate::resource_detail::ResourceOwner {
                    kind: "ReplicaSet".into(),
                    name: "api-7b948f".into(),
                }),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Pod(overflowing_pod_detail()),
            },
        });
    harness.run();
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_detail_panel
        .as_mut()
        .unwrap()
        .navigator
        .clear_transition();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .as_ref()
            .unwrap()
            .navigator
            .back_stack()
            .len(),
        2
    );
    harness.snapshot_options(
        "pod_inspector_with_two_back_history_blades",
        &transformed_blade_snapshot_options(),
    );

    let detail_watch_starts_before_back = harness
        .state()
        .worker
        .commands
        .iter()
        .filter(|command| matches!(command, WorkerCommand::StartResourceDetailWatch { .. }))
        .count();
    harness.get_by_label("Back").click_accesskit();
    harness.run_steps(1);
    assert_eq!(
        harness
            .state()
            .worker
            .commands
            .iter()
            .filter(|command| matches!(command, WorkerCommand::StartResourceDetailWatch { .. }))
            .count(),
        detail_watch_starts_before_back,
        "Back promotes the already-watched history entry instead of restarting it",
    );
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_detail_panel
        .as_mut()
        .unwrap()
        .navigator
        .clear_transition();
    harness.run();
    harness.get_by_label("Forward").click_accesskit();
    harness.run_steps(1);
    assert_eq!(
        harness
            .state()
            .worker
            .commands
            .iter()
            .filter(|command| matches!(command, WorkerCommand::StartResourceDetailWatch { .. }))
            .count(),
        detail_watch_starts_before_back,
        "Forward promotes the already-watched history entry instead of restarting it",
    );
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_detail_panel
        .as_mut()
        .unwrap()
        .navigator
        .clear_transition();
    harness.run();

    let mut commands = Vec::new();
    harness.state_mut().ui_state.navigate_resource_detail(
        2,
        pod.clone(),
        "api-7b948f-pod-debug".into(),
        Some("kube-system".into()),
        "pod-debug-uid".into(),
        &mut commands,
    );
    harness.state_mut().worker.commands.extend(commands);
    let history_entry_id = harness.state().ui_state.clusters[&2]
        .resource_detail_panel
        .as_ref()
        .unwrap()
        .history_entry_id;
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id,
            detail: ResourceDetail {
                api_resource: pod,
                name: "api-7b948f-pod-debug".into(),
                namespace: Some("kube-system".into()),
                uid: "pod-debug-uid".into(),
                resource_version: "1".into(),
                creation_timestamp: Some(
                    time::OffsetDateTime::now_utc() - time::Duration::minutes(10),
                ),
                owner: None,
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Generic,
            },
        });
    harness.run();
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .resource_detail_panel
        .as_mut()
        .unwrap()
        .navigator
        .clear_transition();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .as_ref()
            .unwrap()
            .navigator
            .back_stack()
            .len(),
        3
    );
    harness.snapshot_options(
        "pod_inspector_with_three_back_history_entries",
        &transformed_blade_snapshot_options(),
    );
}

#[test]
fn pod_resource_detail_inspector_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let name = "coredns-66bc5c9577-ffw2s";
    let resource_position = harness.get_by_label(name).rect().center();
    primary_click(&mut harness, resource_position);
    harness.run_steps(1);
    let pods = fixture_api_resource("core", "Pod", "pods");
    harness.state_mut().worker.results.extend([
        WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: ResourceDetail {
                api_resource: pods,
                name: name.into(),
                namespace: Some("kube-system".into()),
                uid: "fixture-0".into(),
                resource_version: "1".into(),
                creation_timestamp: None,
                owner: None,
                labels: BTreeMap::from([("k8s-app".into(), "kube-dns".into())]),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Pod(PodDetail {
                    phase: "Running".into(),
                    conditions: vec![PodConditionDetail {
                        type_: "Ready".into(),
                        status: "True".into(),
                        reason: None,
                        message: None,
                    }],
                    node_name: Some("kind-control-plane".into()),
                    pod_ip: Some("10.244.0.3".into()),
                    host_ip: Some("172.18.0.2".into()),
                    qos_class: Some("Burstable".into()),
                    restart_policy: Some("Always".into()),
                    service_account_name: Some("coredns".into()),
                    dns_policy: Some("ClusterFirst".into()),
                    containers: vec![PodContainerDetail {
                        name: "coredns".into(),
                        image: "registry.k8s.io/coredns/coredns:v1.11.1".into(),
                        ready: true,
                        restart_count: 0,
                        state: "Running".into(),
                        reason: None,
                        message: None,
                        command: vec!["/coredns".into()],
                        args: vec![
                            "-conf".into(),
                            "/etc/coredns/Corefile".into(),
                            "--cluster-domain=cluster.local --feature-gates=VeryLongFeature=true --request-timeout=60s".into(),
                        ],
                        ports: vec!["53/UDP".into()],
                        environment_variables: vec![
                            PodEnvironmentVariableDetail {
                                name: "LOG_LEVEL".into(),
                                value: Some("info".into()),
                                source: PodEnvironmentVariableSource::Literal,
                            },
                            PodEnvironmentVariableDetail {
                                name: "KUBERNETES_SERVICE_HOST".into(),
                                value: Some("10.244.0.3".into()),
                                source: PodEnvironmentVariableSource::Field {
                                    path: "status.podIP".into(),
                                },
                            },
                            PodEnvironmentVariableDetail {
                                name: "DNS_LOG_FORMAT".into(),
                                value: Some("json".into()),
                                source: PodEnvironmentVariableSource::ConfigMapKey {
                                    name: "coredns-settings".into(),
                                    key: "log-format".into(),
                                    optional: false,
                                },
                            },
                            PodEnvironmentVariableDetail {
                                name: "API_TOKEN".into(),
                                value: Some("test-token".into()),
                                source: PodEnvironmentVariableSource::SecretKey {
                                    name: "api-credentials".into(),
                                    key: "token".into(),
                                    optional: false,
                                },
                            },
                        ],
                    }],
                    log_containers: Vec::new(),
                    volumes: vec![
                        PodVolumeDetail {
                            name: "config-volume".into(),
                            kind: "ConfigMap".into(),
                            source: "coredns".into(),
                            mount_path: Some("/etc/coredns".into()),
                            read_only: false,
                        },
                        PodVolumeDetail {
                            name: "kube-api-access".into(),
                            kind: "Projected".into(),
                            source: "kube-api-access".into(),
                            mount_path: Some(
                                "/var/run/secrets/kubernetes.io/serviceaccount".into(),
                            ),
                            read_only: true,
                        },
                    ],
                }),
            },
        },
        WorkerResult::ResourceEventsReplaced {
            cluster_key: 2,
            history_entry_id: 1,
            events: vec![ResourceEvent {
                uid: "event-1".into(),
                type_: "Normal".into(),
                reason: "Started".into(),
                message: "Started container coredns".into(),
                source: Some("kubelet".into()),
                count: 1,
                last_timestamp: None,
            }],
        },
    ]);
    harness.run();

    let first_arg_top = harness.get_by_label("-conf").rect().top();
    let second_arg_top = harness.get_by_label("/etc/coredns/Corefile").rect().top();
    let long_arg_top = harness
        .get_by_label(
            "--cluster-domain=cluster.local --feature-gates=VeryLongFeature=true --request-timeout=60s",
        )
        .rect()
        .top();
    assert_eq!(first_arg_top, second_arg_top);
    assert_eq!(second_arg_top, long_arg_top);

    harness.snapshot_options(
        "pod_resource_detail_inspector",
        &egui_kittest::SnapshotOptions::new().failed_pixel_count_threshold(1),
    );
    harness.get_by_label("Reveal").click_accesskit();
    harness.run();
    harness.get_by_label("test-token");
}

fn open_typed_detail(
    harness: &mut Harness<MyEguiApp<MockWorker>>,
    api_resource: crate::api_resource::ApiResource,
    detail: ResourceDetail,
) {
    let mut commands = Vec::new();
    harness.state_mut().ui_state.open_resource_detail(
        2,
        api_resource,
        detail.name.clone(),
        detail.namespace.clone(),
        detail.uid.clone(),
        &mut commands,
    );
    harness.state_mut().worker.commands.extend(commands);
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail,
        });
    harness.run();
}

fn config_map_detail(data: BTreeMap<String, String>) -> ResourceDetail {
    ResourceDetail {
        api_resource: fixture_api_resource("core", "ConfigMap", "configmaps"),
        name: "settings".into(),
        namespace: Some("kube-system".into()),
        uid: "configmap-uid".into(),
        resource_version: "1".into(),
        creation_timestamp: None,
        owner: None,
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::ConfigMap(ConfigMapDetail {
            data,
            immutable: false,
        }),
    }
}

#[test]
fn deployment_inspector_exposes_the_shared_restart_action() {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let detail = ResourceDetail {
        api_resource: deployment.clone(),
        name: "coredns".into(),
        namespace: Some("kube-system".into()),
        uid: "deployment-uid".into(),
        resource_version: "1".into(),
        creation_timestamp: None,
        owner: None,
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Generic,
    };
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    open_typed_detail(&mut harness, deployment, detail);

    harness
        .get_by_label("More actions for coredns")
        .click_accesskit();
    harness.run();
    harness.get_by_label("Restart rollout").click_accesskit();
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&2]
            .pending_deployment_restart
            .as_ref()
            .is_some_and(|pending| {
                pending.resource_name == "coredns" && pending.namespace == "kube-system"
            })
    );
}

#[test]
fn config_map_inspector_saves_only_changed_existing_data_values() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let detail = config_map_detail(BTreeMap::from([
        ("mode".into(), "development".into()),
        ("unused".into(), "preserved".into()),
    ]));
    open_typed_detail(&mut harness, detail.api_resource.clone(), detail);

    let editor = harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .expect("fixture cluster should exist")
        .resource_detail_panel
        .as_mut()
        .and_then(|panel| panel.data_editor.as_mut())
        .expect("ConfigMap editor should initialize from the detail payload");
    editor
        .draft_values
        .insert("mode".into(), "production".into());
    harness.run();
    harness.get_by_label("Save data").click_accesskit();
    harness.run();

    assert!(matches!(
        harness.state().worker.commands.last(),
        Some(WorkerCommand::UpdateResourceData { update, .. })
            if update.expected_resource_version == "1"
                && update.expected_values == BTreeMap::from([("mode".into(), "development".into())])
                && update.updated_values == BTreeMap::from([("mode".into(), "production".into())])
    ));
}

#[test]
fn config_map_resource_detail_inspector_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let detail = config_map_detail(BTreeMap::from([
        ("app.toml".into(), "[server]\nport = 8080".into()),
        ("log-level".into(), "info".into()),
    ]));
    open_typed_detail(&mut harness, detail.api_resource.clone(), detail);

    harness.snapshot_options(
        "config_map_resource_detail_inspector",
        &egui_kittest::SnapshotOptions::new().failed_pixel_count_threshold(1),
    );
}

#[test]
fn resource_detail_more_actions_use_the_shared_resource_menu() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let detail = config_map_detail(BTreeMap::from([("mode".into(), "development".into())]));
    open_typed_detail(&mut harness, detail.api_resource.clone(), detail);

    harness
        .get_by_label("More actions for settings")
        .click_accesskit();
    harness.run();
    harness.snapshot_options(
        "config_map_resource_detail_actions",
        &egui_kittest::SnapshotOptions::new().failed_pixel_count_threshold(1),
    );
    harness.get_by_label("Edit").click_accesskit();
    harness.run();
    assert!(matches!(
        harness.state().worker.commands.iter().rev().nth(1),
        Some(WorkerCommand::GetResourceYaml { resource_name, .. }) if resource_name == "settings"
    ));
    assert!(matches!(
        harness.state().worker.commands.last(),
        Some(WorkerCommand::LoadResourceSchema { .. })
    ));

    harness
        .get_by_label("More actions for settings")
        .click_accesskit();
    harness.run();
    harness.get_by_label("Delete").click_accesskit();
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&2]
            .pending_delete
            .as_ref()
            .is_some_and(|pending| pending.resource_name == "settings")
    );
}

#[test]
fn secret_inspector_masks_values_and_prompts_for_a_real_external_change() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let secrets = fixture_api_resource("core", "Secret", "secrets");
    let detail = ResourceDetail {
        api_resource: secrets.clone(),
        name: "credentials".into(),
        namespace: Some("kube-system".into()),
        uid: "secret-uid".into(),
        resource_version: "1".into(),
        creation_timestamp: None,
        owner: None,
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Secret(SecretDetail {
            data: BTreeMap::from([
                (
                    "password".into(),
                    SecretDataDetail {
                        byte_len: 6,
                        text: Some("secret".into()),
                    },
                ),
                (
                    "binary".into(),
                    SecretDataDetail {
                        byte_len: 2,
                        text: None,
                    },
                ),
            ]),
            immutable: false,
            type_: "Opaque".into(),
        }),
    };
    open_typed_detail(&mut harness, secrets, detail);

    harness.get_by_label("Reveal").click_accesskit();
    harness.run();
    harness.get_by_label("Hide");
    harness.get_by_label("Binary data");
    harness.get_by_label("This value cannot be edited in the inspector.");

    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .expect("fixture cluster should exist")
        .resource_detail_panel
        .as_mut()
        .and_then(|panel| panel.data_editor.as_mut())
        .expect("Secret editor should initialize")
        .draft_values
        .insert("password".into(), "changed".into());
    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: ResourceDetail {
                payload: ResourceDetailPayload::Secret(SecretDetail {
                    data: BTreeMap::from([(
                        "password".into(),
                        SecretDataDetail {
                            byte_len: 7,
                            text: Some("cluster".into()),
                        },
                    )]),
                    immutable: false,
                    type_: "Opaque".into(),
                }),
                ..config_map_detail(BTreeMap::new())
            },
        });
    harness.run();
    harness.get_by_label("Data changed on cluster");
    harness.get_by_label("Keep my edits").click_accesskit();
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .as_ref()
            .and_then(|panel| panel.data_editor.as_ref())
            .and_then(|editor| editor.draft_values.get("password"))
            .map(String::as_str),
        Some("changed")
    );
}

#[test]
fn secret_resource_detail_inspector_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let secrets = fixture_api_resource("core", "Secret", "secrets");
    let detail = ResourceDetail {
        api_resource: secrets.clone(),
        name: "api-credentials".into(),
        namespace: Some("kube-system".into()),
        uid: "secret-snapshot-uid".into(),
        resource_version: "1".into(),
        creation_timestamp: None,
        owner: None,
        labels: BTreeMap::from([("app.kubernetes.io/name".into(), "api".into())]),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Secret(SecretDetail {
            data: BTreeMap::from([
                (
                    "password".into(),
                    SecretDataDetail {
                        byte_len: 24,
                        text: Some("super-secret-password-value".into()),
                    },
                ),
                (
                    "certificate".into(),
                    SecretDataDetail {
                        byte_len: 1872,
                        text: Some("-----BEGIN CERTIFICATE-----\n…".into()),
                    },
                ),
                (
                    "binary-token".into(),
                    SecretDataDetail {
                        byte_len: 32,
                        text: None,
                    },
                ),
            ]),
            immutable: false,
            type_: "Opaque".into(),
        }),
    };
    open_typed_detail(&mut harness, secrets, detail);

    harness.snapshot_options(
        "secret_resource_detail_inspector",
        &egui_kittest::SnapshotOptions::new().failed_pixel_count_threshold(1),
    );
}

#[test]
fn resource_table_reflows_after_viewport_resize() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    harness.set_size(egui::vec2(1024.0, 1024.0));
    harness.run();
    harness.snapshot("resource_table_narrow");

    components::test_support::setup_egui(&mut harness);
    harness.run();
    harness.snapshot("resource_table_resized");
}

#[test]
fn cluster_rail_shows_connection_status_marker_and_tooltip() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state
        .clusters
        .get_mut(&1)
        .expect("dev fixture exists")
        .connection = ClusterConnectionState::Connecting;
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_label("dev").hover();
    harness.run();
    harness.snapshot_options(
        "cluster_rail_connection_status",
        &egui_kittest::SnapshotOptions::new().failed_pixel_count_threshold(1),
    );
}

#[test]
fn delete_confirmation_can_be_cancelled_without_sending_a_command() {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.selected_api_resource = Some(fixture_api_resource("", "ConfigMap", "configmaps"));
    cluster.pending_delete = Some(PendingDelete {
        api_resource: fixture_api_resource("", "ConfigMap", "configmaps"),
        resource_name: "important-config".into(),
        namespace: Some("default".into()),
        confirmation_available_at: std::time::Instant::now(),
    });
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    };

    harness.run();
    harness.get_by_label("Cancel").click_accesskit();
    harness.run();

    assert!(harness.state().worker.commands.is_empty());
    assert!(
        harness.state().ui_state.clusters[&1]
            .pending_delete
            .is_none()
    );
}

#[test]
fn delete_confirmation_waits_before_enabling_the_destructive_action() {
    let api_resource = fixture_api_resource("", "ConfigMap", "configmaps");
    let mut cluster = fixture_cluster(1, "dev");
    cluster.pending_delete = Some(PendingDelete {
        api_resource,
        resource_name: "important-config".into(),
        namespace: Some("default".into()),
        confirmation_available_at: std::time::Instant::now() + std::time::Duration::from_secs(3),
    });
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    };

    harness.run_steps(1);
    harness
        .get_by_label("Delete important-config")
        .click_accesskit();
    harness.run_steps(1);
    assert!(harness.state().worker.commands.is_empty());
    assert!(
        harness.state().ui_state.clusters[&1]
            .pending_delete
            .is_some()
    );

    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&1)
        .unwrap()
        .pending_delete = Some(PendingDelete {
        api_resource: fixture_api_resource("", "ConfigMap", "configmaps"),
        resource_name: "important-config".into(),
        namespace: Some("default".into()),
        confirmation_available_at: std::time::Instant::now(),
    });
    harness.run();
    harness
        .get_by_label("Delete important-config")
        .click_accesskit();
    harness.run();
    assert!(matches!(
        harness.state().worker.commands.as_slice(),
        [WorkerCommand::DeleteResource { resource_name, .. }] if resource_name == "important-config"
    ));
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
            .filter_map(|command| match command {
                WorkerCommand::StartResourceWatch { api_resource, .. } =>
                    Some(api_resource.name.as_str()),
                _ => None,
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
    harness.snapshot("01_empty_state");

    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesClustersUpdated(vec![
            Cluster {
                name: "dev".into(),
                cluster: None,
                is_current: true,
            },
            Cluster {
                name: "prod".into(),
                cluster: Some("production".into()),
                is_current: false,
            },
        ]));
    harness.run_steps(1);
    assert_eq!(harness.state().ui_state.selected_cluster, Some(1));
    assert!(matches!(
        harness.state().ui_state.clusters[&1].connection,
        ClusterConnectionState::Connecting
    ));
    assert!(matches!(
        harness.state().worker.commands.as_slice(),
        [WorkerCommand::ConnectToCluster {
            cluster_key: 1,
            cluster,
        }] if cluster == "dev"
    ));
    harness.snapshot("current_context_connecting");

    harness.state_mut().worker.results.push_back(
        WorkerResult::KubernetesClusterConnectionCreated {
            cluster_key: 1,
            runner: None,
        },
    );

    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesNamespacesReplaced {
            cluster_key: 1,
            namespaces: vec![
                MinimalNamespace {
                    name: "default".into(),
                    display_name: None,
                },
                MinimalNamespace {
                    name: "kube-system".into(),
                    display_name: None,
                },
                MinimalNamespace {
                    name: "monitoring".into(),
                    display_name: Some("Monitoring Stack".into()),
                },
            ],
        });
    harness.run_steps(1);

    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesApisLoaded {
            cluster_key: 1,
            api_resources: vec![
                fixture_api_resource("", "Pod", "pods"),
                fixture_api_resource("", "Service", "services"),
                fixture_api_resource("", "ConfigMap", "configmaps"),
                fixture_api_resource("apps", "Deployment", "deployments"),
                fixture_api_resource("apps", "StatefulSet", "statefulsets"),
                fixture_api_resource("networking.k8s.io", "Ingress", "ingresses"),
            ],
        });
    harness.run();

    select_namespace(&mut harness, "default");
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness.get_by_label("Pods").click_accesskit();
    harness.run_steps(1);
    harness.run_steps(1);

    let pods = fixture_api_resource("", "Pod", "pods");
    assert!(
        harness
            .state()
            .worker
            .commands
            .iter()
            .any(|command| matches!(
                command,
                WorkerCommand::StartResourceWatch {
                    cluster_key: 1,
                    api_resource,
                    namespace,
                } if api_resource == &pods && namespace.as_deref() == Some("default")
            ))
    );
    harness.get_by_label("Loading resources");
    harness.snapshot("resource_watch_loading");

    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesResourcesReplaced {
            cluster_key: 1,
            api_resource: pods.clone(),
            namespace: Some("default".into()),
            resources: Vec::new(),
        });
    harness.run();
    harness.get_by_label("No resources found");

    harness
        .state_mut()
        .worker
        .results
        .push_back(WorkerResult::KubernetesResourceWatchFailed {
            cluster_key: 1,
            api_resource: pods,
            namespace: Some("default".into()),
            error: watch_error.into(),
        });
    harness.run();
    harness.get_by_label("Unable to load resources");
    harness.get_by_label(watch_error);
    harness.snapshot("resource_watch_error");
    harness.get_by_label("Retry").click_accesskit();
    harness.run_steps(1);
    assert_eq!(
        harness
            .state()
            .worker
            .commands
            .iter()
            .filter(|command| matches!(command, WorkerCommand::StartResourceWatch { .. }))
            .count(),
        2
    );
}
