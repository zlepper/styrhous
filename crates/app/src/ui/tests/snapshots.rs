use super::super::MyEguiApp;
use super::super::state::ClusterConnectionState;
use super::super::state::{
    BulkDeleteProgress, BulkDeleteTarget, PendingDelete, PendingForceDelete, ResourceWatchState,
    UiState, ValidationState, YamlEditorWindowState,
};
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
    PodVolumeDetail, ResourceDetail, ResourceDetailPayload, ResourceEvent, ResourceOwner,
    SecretDataDetail, SecretDetail,
};
use crate::resource_schema::ResourceSchema;
use crate::resource_table::{
    AVAILABLE_COLUMN, CONTAINERS_COLUMN, CellValue, ContainerIndicator, ContainerKind, NODE_COLUMN,
    READY_COLUMN, RESTARTS_COLUMN, STATUS_COLUMN, StatusTone, UP_TO_DATE_COLUMN,
};
use crate::terminal_launcher::{
    DebugProfile, NodeShellPreset, ShellRequest, TerminalLaunchSettings, TerminalLauncher,
    test_support::MockTerminalLauncher,
};
use crate::worker::*;
use components::test_support::{HarnessSnapshotOptions, UiHarnessSnapshot};
use egui::text::{CCursor, CCursorRange};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use k8s_openapi::serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};

struct YamlEditorSnapshotState {
    editor: YamlEditorWindowState,
    commands: Vec<WorkerCommandBox>,
}

fn command_is<T: WorkerCommand + 'static>(command: &WorkerCommandBox) -> Option<&T> {
    command.as_ref().as_any().downcast_ref()
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

fn primary_click<L: TerminalLauncher>(
    harness: &mut Harness<MyEguiApp<MockWorker, L>>,
    position: egui::Pos2,
) {
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

fn drag<L: TerminalLauncher>(
    harness: &mut Harness<MyEguiApp<MockWorker, L>>,
    from: egui::Pos2,
    to: egui::Pos2,
) {
    harness.event(egui::Event::PointerMoved(from));
    harness.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
    harness.event(egui::Event::PointerMoved(to));
    harness.run();
    harness.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
}

fn type_text<L: TerminalLauncher>(
    harness: &mut Harness<MyEguiApp<MockWorker, L>>,
    accessibility_label: &str,
    value: &str,
) {
    let position = harness.get_by_label(accessibility_label).rect().center();
    primary_click(harness, position);
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text(value.into()));
    harness.run();
}

#[test]
fn namespace_selector_replaces_toggles_and_selects_all_without_stopping_watches() {
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
            .filter(|command| command_is::<StartResourceWatch>(command).is_some())
            .count(),
        3
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
    assert_eq!(harness.state().worker.commands.len(), 1);
    assert!(
        harness.state().worker.commands[0]
            .as_ref()
            .as_any()
            .downcast_ref::<StartResourceWatch>()
            .is_some_and(|command| command.cluster_key == 1
                && command.api_resource == nodes
                && command.namespace.is_none())
    );
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
                cells: Default::default(),
                log_containers: Vec::new(),
            }],
        }) as WorkerResultBox);
    harness.run();
    harness.get_by_label("Cluster-wide");
    harness.get_by_label("Open details for kind-control-plane");

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
            .filter(|command| command_is::<StartResourceWatch>(command).is_some())
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
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/namespace_selector_search_snapshot_shows_active_watches/namespace_selector_open_filtered_active_watches",
    ));
}

#[test]
fn no_current_context_leaves_cluster_selection_manual() {
    let mut harness = application_harness::<MockWorker>();
    harness.run();
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(KubernetesClustersUpdated(vec![Cluster {
            name: "dev".into(),
            is_current: false,
        }])) as WorkerResultBox);

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
    harness.ui_harness("resource_tables/oracle_resource_table_snapshot_uses_injected_cluster_state/oracle_resource_table_injected");
}

#[test]
fn resource_navigation_uses_the_persisted_expansion_state() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.set_resource_navigation_node_expanded("section:Apps & Containers", true);
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
                    controller_owner: None,
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
            .commands
            .last()
            .and_then(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<RestartDeployment>())
            .is_some_and(|command| command.cluster_key == 2
                && command.namespace == "kube-system"
                && command.resource_name == "coredns")
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
                        cells: BTreeMap::new(),
                        log_containers: Vec::new(),
                    },
                )]),
                is_synced: true,
                error: None,
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
        .pending_scale = Some(super::super::state::PendingScale {
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

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceScaleFetched {
            cluster_key: 2,
            api_resource: deployment.clone(),
            namespace: Some("kube-system".into()),
            resource_name: "coredns".into(),
            replicas: 3,
        }) as WorkerResultBox);
    harness.run();
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
            super::super::state::ResourceSearchState {
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
            super::super::state::ResourceSearchState {
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
    harness.ui_harness(
        "resource_tables/resource_table_more_actions_snapshot/oracle_resource_table_actions",
    );
}

fn multi_container_pod_table_state() -> (UiState, String) {
    let pods = fixture_api_resource("core", "Pod", "pods");
    let mut state = oracle_resource_table_state();
    let resource = state
        .clusters
        .get_mut(&2)
        .expect("kind fixture exists")
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .expect("pod fixture exists")
        .resources
        .values_mut()
        .next()
        .expect("pod resource exists");
    resource.log_containers = vec![
        PodLogContainer {
            name: "coredns".into(),
            kind: ContainerKind::App,
        },
        PodLogContainer {
            name: "dns-autoscaler".into(),
            kind: ContainerKind::App,
        },
    ];
    let pod_name = resource.name.clone();
    (state, pod_name)
}

#[test]
fn pod_resource_table_multi_container_logs_menu_snapshot() {
    let (state, pod_name) = multi_container_pod_table_state();

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click();
    harness.run();
    harness
        .get_by_label(&format!("More actions for {pod_name}"))
        .click();
    harness.run();

    harness.get_by_label("View logs ⏵");
    harness.ui_harness(
        "resource_tables/pod_resource_table_multi_container_logs_menu_snapshot/multi_container_logs_menu",
    );

    harness.get_by_label("View logs ⏵").click();
    harness.run();
    harness.get_by_label("coredns — Container");
    harness.get_by_label("dns-autoscaler — Container");
    harness.ui_harness(
        "resource_tables/pod_resource_table_multi_container_logs_menu_snapshot/multi_container_logs_submenu",
    );
}

#[test]
fn pod_resource_table_multi_container_logs_context_submenu_snapshot() {
    let (state, pod_name) = multi_container_pod_table_state();
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click();
    harness.run();

    let row_position = harness
        .get_by_label(&format!("Open details for {pod_name}"))
        .rect()
        .center();
    secondary_click(&mut harness, row_position);
    harness.run();
    harness.get_by_label("View logs ⏵").click();
    harness.run();

    harness.get_by_label("coredns — Container");
    harness.get_by_label("dns-autoscaler — Container");
    harness.ui_harness(
        "resource_tables/pod_resource_table_multi_container_logs_context_submenu_snapshot/multi_container_logs_context_submenu",
    );
}

#[test]
fn resource_table_multi_selection_confirms_and_dispatches_bulk_delete() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    harness.get_by_label("Select row 1").click_accesskit();
    harness.run();
    harness.get_by_label("Select row 2").click_accesskit();
    harness.run();
    harness.get_by_label("2 selected");
    harness.ui_harness(
        "resource_tables/resource_table_multi_selection_confirms_and_dispatches_bulk_delete/selected",
    );

    harness.get_by_label("Delete selected").click_accesskit();
    harness.run();
    harness.get_by_label("Delete 2 resources?");
    harness.ui_harness(
        "resource_tables/resource_table_multi_selection_confirms_and_dispatches_bulk_delete/confirmation",
    );

    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&2)
        .unwrap()
        .pending_bulk_delete
        .as_mut()
        .unwrap()
        .confirmation_available_at = std::time::Instant::now();
    harness.run();
    harness.get_by_label("Delete 2 resources").click_accesskit();
    harness.run();

    let commands = &harness.state().worker.commands;
    assert_eq!(commands.len(), 2);
    assert!(
        commands[0]
            .as_ref()
            .as_any()
            .downcast_ref::<DeleteResource>()
            .is_some_and(|command| {
                command.cluster_key == 2
                    && command.namespace.as_deref() == Some("kube-system")
                    && command.resource_name == "coredns-66bc5c9577-ffw2s"
            })
    );
    assert!(
        commands[1]
            .as_ref()
            .as_any()
            .downcast_ref::<DeleteResource>()
            .is_some_and(|command| {
                command.cluster_key == 2
                    && command.namespace.as_deref() == Some("kube-system")
                    && command.resource_name == "coredns-66bc5c9577-z9gt9"
            })
    );

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDeleteCompleted {
            cluster_key: 2,
            api_resource: fixture_api_resource("core", "Pod", "pods"),
            namespace: Some("kube-system".into()),
            resource_name: "coredns-66bc5c9577-ffw2s".into(),
            bulk_delete_id: Some(1),
        }) as WorkerResultBox);
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&2].resource_selections
            [&fixture_api_resource("core", "Pod", "pods")],
        HashSet::from(["fixture-1".into()])
    );

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDeleteCompleted {
            cluster_key: 2,
            api_resource: fixture_api_resource("core", "Pod", "pods"),
            namespace: Some("kube-system".into()),
            resource_name: "coredns-66bc5c9577-z9gt9".into(),
            bulk_delete_id: Some(1),
        }) as WorkerResultBox);
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&2].resource_selections
            [&fixture_api_resource("core", "Pod", "pods")]
            .is_empty()
    );
}

#[test]
fn bulk_delete_keeps_failed_resources_selected_and_reports_them_together() {
    let api_resource = fixture_api_resource("core", "Pod", "pods");
    let failed = BulkDeleteTarget {
        uid: "failed-uid".into(),
        name: "failed-pod".into(),
        namespace: Some("default".into()),
    };
    let succeeded = BulkDeleteTarget {
        uid: "succeeded-uid".into(),
        name: "succeeded-pod".into(),
        namespace: Some("default".into()),
    };
    let mut cluster = fixture_cluster(1, "dev");
    cluster.resource_selections.insert(
        api_resource.clone(),
        HashSet::from([failed.uid.clone(), succeeded.uid.clone()]),
    );
    cluster.bulk_delete_progress = Some(BulkDeleteProgress::new(
        42,
        api_resource.clone(),
        vec![failed.clone(), succeeded.clone()],
    ));
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    };
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDeleteCompleted {
            cluster_key: 1,
            api_resource: fixture_api_resource("core", "ConfigMap", "configmaps"),
            namespace: succeeded.namespace.clone(),
            resource_name: succeeded.name.clone(),
            bulk_delete_id: None,
        }) as WorkerResultBox);
    harness.run();
    assert_eq!(
        harness.state().ui_state.clusters[&1].resource_selections[&api_resource],
        HashSet::from([failed.uid.clone(), succeeded.uid.clone()])
    );
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDeleteFailed {
            cluster_key: 1,
            api_resource: api_resource.clone(),
            namespace: failed.namespace.clone(),
            resource_name: failed.name.clone(),
            bulk_delete_id: Some(42),
            error: "forbidden".into(),
        }) as WorkerResultBox);
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDeleteCompleted {
            cluster_key: 1,
            api_resource: api_resource.clone(),
            namespace: succeeded.namespace.clone(),
            resource_name: succeeded.name.clone(),
            bulk_delete_id: Some(42),
        }) as WorkerResultBox);

    harness.run();
    harness.get_by_label("Some resources could not be deleted");
    assert_eq!(
        harness.state().ui_state.clusters[&1].resource_selections[&api_resource],
        HashSet::from([failed.uid])
    );
    harness.get_by_label("Dismiss").click_accesskit();
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&1]
            .bulk_delete_error
            .is_none()
    );
}

#[test]
fn resource_table_row_context_menu_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let resource_name_rect = harness
        .get_by_label("Open details for coredns-66bc5c9577-ffw2s")
        .rect();
    let click_position = egui::pos2(
        resource_name_rect.right() + 32.0,
        resource_name_rect.center().y,
    );
    secondary_click(&mut harness, click_position);
    harness.run();

    harness.get_by_label("Edit");
    harness.ui_harness(
        HarnessSnapshotOptions::strict("resource_tables/resource_table_row_context_menu_snapshot/oracle_resource_table_row_context_actions")
            .max_failed_pixels(2),
    );
}

#[test]
fn resource_table_row_context_menu_opens_when_right_clicking_resource_text() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let text_position = harness
        .get_by_label("Open details for coredns-66bc5c9577-ffw2s")
        .rect()
        .center();
    secondary_click(&mut harness, text_position);
    harness.run();

    harness.get_by_label("Edit");
}

#[test]
fn context_menu_action_does_not_activate_the_overlapped_resource_button() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let resource_name = "coredns-66bc5c9577-ffw2s";
    let resource_position = harness
        .get_by_label(&format!("Open details for {resource_name}"))
        .rect()
        .center();
    secondary_click(&mut harness, resource_position);
    harness.run();

    let edit = harness.get_by_label("Edit");
    let overlapped_resource = harness
        .get_by_label("Open details for coredns-66bc5c9577-z9gt9")
        .rect();
    assert!(
        edit.rect().intersects(overlapped_resource),
        "the menu action must overlap a resource button to exercise popup input ownership"
    );

    edit.click();
    harness.run_steps(1);

    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none(),
        "the context-menu click must not open the overlapped resource"
    );
    assert!(
        !harness
            .state()
            .worker
            .commands
            .iter()
            .any(|command| command_is::<StartResourceDetailWatch>(command).is_some()),
        "the context-menu click must not start an underlying detail watch"
    );
}

#[test]
fn namespace_popup_option_does_not_activate_the_overlapped_resource_button() {
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    for namespace in ["default", "monitoring"] {
        cluster.namespaces.insert(
            namespace.into(),
            MinimalNamespace {
                name: namespace.into(),
                display_name: None,
            },
        );
    }

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = state;
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run();

    let namespace_option = harness.get_by_label("monitoring");
    let overlapped_resource = harness
        .get_by_label("Open details for coredns-66bc5c9577-z9gt9")
        .rect();
    assert!(
        namespace_option.rect().intersects(overlapped_resource),
        "the namespace option must overlap a resource button to exercise popup input ownership"
    );

    namespace_option.click();
    harness.run_steps(2);

    assert_eq!(
        harness.state().ui_state.clusters[&2].selected_namespaces,
        HashSet::from(["monitoring".to_owned()])
    );
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none(),
        "the namespace click must not open the overlapped resource"
    );
    assert!(
        !harness
            .state()
            .worker
            .commands
            .iter()
            .any(|command| command_is::<StartResourceDetailWatch>(command).is_some()),
        "the namespace click must not start an underlying detail watch"
    );
}

#[test]
fn namespace_popup_filters_to_an_offscreen_option_before_pointer_click() {
    let generated_namespaces = (0..12)
        .map(|index| MinimalNamespace {
            name: format!("kdui-it-concurrent-{index:02}"),
            display_name: None,
        })
        .collect::<Vec<_>>();
    let target_namespace = "kube-system";
    let mut namespaces = vec![MinimalNamespace {
        name: "default".into(),
        display_name: None,
    }];
    namespaces.extend(generated_namespaces);
    namespaces.extend([
        MinimalNamespace {
            name: "kube-node-lease".into(),
            display_name: None,
        },
        MinimalNamespace {
            name: "kube-public".into(),
            display_name: None,
        },
    ]);
    namespaces.push(MinimalNamespace {
        name: target_namespace.into(),
        display_name: None,
    });
    let setup_state = || {
        let mut state = oracle_resource_table_state();
        let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
        cluster.selected_namespaces.clear();
        cluster.selected_api_resource = None;
        cluster.namespaces = namespaces
            .iter()
            .cloned()
            .map(|namespace| (namespace.name.clone().into(), namespace))
            .collect();
        state
    };

    let mut offscreen_harness = application_harness::<MockWorker>();
    offscreen_harness.state_mut().ui_state = setup_state();
    offscreen_harness.run();
    offscreen_harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    offscreen_harness.run();
    offscreen_harness.get_by_label(target_namespace).click();
    offscreen_harness.run_steps(1);
    assert!(
        offscreen_harness.state().ui_state.clusters[&2]
            .selected_namespaces
            .is_empty(),
        "the unfiltered target is off-screen, so its accessibility node must not be used for a pointer click"
    );

    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = setup_state();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run();
    let search_input = harness
        .query_by_role_and_label(egui::accesskit::Role::TextInput, "Search Namespace")
        .expect("the namespace popup search input should be present");
    assert!(
        search_input.is_focused(),
        "the popup search field must receive focus before typing"
    );
    search_input.type_text(target_namespace);
    harness.run_steps(1);
    assert!(
        harness
            .query_by_role_and_label(egui::accesskit::Role::TextInput, "Search Namespace")
            .is_some_and(|input| input.value().as_deref() == Some(target_namespace)),
        "the text input must contain the namespace filter before the test clicks its option"
    );

    harness.state_mut().worker.results.extend((0..32).map(|_| {
        Box::new(KubernetesNamespacesReplaced {
            cluster_key: 2,
            namespaces: namespaces.clone(),
        }) as WorkerResultBox
    }));
    harness.get_by_label(target_namespace).click();
    harness.run_steps(1);

    assert_eq!(
        harness.state().ui_state.clusters[&2].selected_namespaces,
        HashSet::from([target_namespace.to_owned()]),
        "the filtered namespace option must receive a pointer click even while the worker delivers a burst of discovery results"
    );
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
        &[ShellRequest::Pod {
            kube_context: "kind-kind".into(),
            namespace: "kube-system".into(),
            pod_name,
            container: "coredns".into(),
        }]
    );
}

#[test]
fn node_shell_action_launches_the_selected_context_node_and_preset() {
    let mut harness = application_harness_with_terminal::<MockWorker, MockTerminalLauncher>();
    let nodes = fixture_cluster_scoped_api_resource("core", "Node", "nodes");
    let mut state = oracle_resource_table_state();
    let cluster = state.clusters.get_mut(&2).unwrap();
    cluster.selected_api_resource = Some(nodes.clone());
    cluster.resource_cache.insert(
        (nodes, None),
        ResourceWatchState {
            resources: BTreeMap::from([(
                "node-uid".into(),
                MinimalResource {
                    uid: "node-uid".into(),
                    name: "kind-control-plane".into(),
                    namespace: None,
                    creation_timestamp: None,
                    controller_owner: None,
                    cells: BTreeMap::new(),
                    log_containers: Vec::new(),
                },
            )]),
            is_synced: true,
            error: None,
        },
    );
    harness.state_mut().ui_state = state;
    harness.run();

    let more_actions_position = harness
        .get_by_label("More actions for kind-control-plane")
        .rect()
        .center();
    primary_click(&mut harness, more_actions_position);
    harness.run();
    let shell_position = harness.get_by_label("Shell ⏵").rect().center();
    primary_click(&mut harness, shell_position);
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/node_shell_action_launches_the_selected_context_node_and_preset/node_shell_presets",
    ));
    let busybox_position = harness.get_by_label("Busybox — General").rect().center();
    primary_click(&mut harness, busybox_position);
    harness.run();

    assert_eq!(
        harness.state().terminal_launcher.requests.as_slice(),
        &[ShellRequest::Node {
            kube_context: "kind-kind".into(),
            node_name: "kind-control-plane".into(),
            preset: NodeShellPreset {
                name: "Busybox".into(),
                image: "busybox".into(),
                profile: DebugProfile::General,
            },
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

    harness.get_by_label("SHELL");
    harness.get_by_label("Couldn’t open a terminal");
    harness.get_by_label(
        "No supported terminal launcher was found. Tried: xdg-terminal-exec (No such file or directory (os error 2)).",
    );
    harness.get_by_label("Open settings");
    harness.get_by_label("Dismiss");
    assert!(harness.state().ui_state.terminal_launch_error.is_some());
    harness.run_steps(2);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel("terminal/shell_launch_failure_uses_the_styled_error_modal_and_opens_terminal_settings/terminal_launch_error"));

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

    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(&mut harness, settings_position);
    harness.run();

    harness.get_by_label("Terminal launcher");
    harness.get_by_label("Save changes");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/settings_button_opens_the_terminal_launcher_blade/settings_terminal_launcher",
    ));
}

#[test]
fn settings_blade_shows_custom_terminal_launcher_details() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_settings_open = true;
    state.terminal_settings_draft = TerminalLaunchSettings {
        custom_template: Some("alacritty -e {command}".into()),
        ..Default::default()
    };
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "Command template");
    harness.get_by_label("Save changes");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/settings_blade_shows_custom_terminal_launcher_details/settings_terminal_launcher_custom",
    ));
}

#[test]
fn saving_node_shell_presets_applies_the_settings_draft() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_settings_open = true;
    state.terminal_settings_draft = TerminalLaunchSettings {
        custom_template: None,
        node_shell_presets: vec![NodeShellPreset {
            name: "Operations".into(),
            image: "registry.example/debug-tools:v1".into(),
            profile: DebugProfile::Sysadmin,
        }],
    };
    harness.state_mut().ui_state = state;
    harness.run();

    let save_position = harness.get_by_label("Save changes").rect().center();
    primary_click(&mut harness, save_position);
    harness.run_steps(2);

    assert_eq!(
        harness.state().terminal_launch_settings.node_shell_presets,
        vec![NodeShellPreset {
            name: "Operations".into(),
            image: "registry.example/debug-tools:v1".into(),
            profile: DebugProfile::Sysadmin,
        }]
    );
    assert!(!harness.state().ui_state.terminal_settings_open);
}

#[test]
fn node_shell_preset_table_adds_and_removes_rows() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_settings_open = true;
    state.terminal_settings_draft = TerminalLaunchSettings {
        custom_template: None,
        node_shell_presets: vec![NodeShellPreset {
            name: "Operations".into(),
            image: "registry.example/debug-tools:v1".into(),
            profile: DebugProfile::Sysadmin,
        }],
    };
    harness.state_mut().ui_state = state;
    harness.run();

    let remove_position = harness.get_by_label("Remove Operations").rect().center();
    primary_click(&mut harness, remove_position);
    harness.run();
    assert!(
        harness
            .state()
            .ui_state
            .terminal_settings_draft
            .node_shell_presets
            .is_empty()
    );

    let add_position = harness.get_by_label("Add node shell").rect().center();
    primary_click(&mut harness, add_position);
    harness.run();
    assert_eq!(
        harness
            .state()
            .ui_state
            .terminal_settings_draft
            .node_shell_presets,
        vec![NodeShellPreset {
            name: String::new(),
            image: String::new(),
            profile: DebugProfile::General,
        }]
    );
}

#[test]
fn node_shell_preset_table_reorders_rows_by_dragging_the_handle() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_settings_open = true;
    state.terminal_settings_draft = TerminalLaunchSettings::default();
    harness.state_mut().ui_state = state;
    harness.run();

    let busybox_position = harness.get_by_label("Reorder Busybox").rect().center();
    let first_visible_row = harness.get_by_label("Reorder Ubuntu").rect();
    // Busybox overlaps the first visible destination row by exactly half here,
    // so it takes the next slot without requiring a full-row movement.
    let half_overlap_position = egui::pos2(first_visible_row.center().x, first_visible_row.top());
    drag(&mut harness, busybox_position, half_overlap_position);

    assert_eq!(
        harness
            .state()
            .ui_state
            .terminal_settings_draft
            .node_shell_presets
            .iter()
            .map(|preset| preset.name.as_str())
            .collect::<Vec<_>>(),
        ["Ubuntu", "Busybox", "Netshoot"]
    );
}

#[test]
fn node_shell_preset_table_moves_the_dragged_row() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_settings_open = true;
    state.terminal_settings_draft = TerminalLaunchSettings::default();
    harness.state_mut().ui_state = state;
    harness.run();

    let busybox_position = harness.get_by_label("Reorder Busybox").rect().center();
    let netshoot_rect = harness.get_by_label("Reorder Netshoot").rect();
    let target_position = egui::pos2(netshoot_rect.center().x, netshoot_rect.bottom() - 4.0);
    harness.event(egui::Event::PointerMoved(busybox_position));
    harness.event(egui::Event::PointerButton {
        pos: busybox_position,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
    harness.event(egui::Event::PointerMoved(target_position));
    harness.run_steps(2);

    // The preview is intentionally rendered above the destination row on egui's
    // tooltip layer while a drag is active.
    harness.ui_harness(
        HarnessSnapshotOptions::one_pixel(
            "terminal/node_shell_preset_table_moves_the_dragged_row/dragging_row",
        )
        .check_illegal_overlaps(false),
    );

    harness.event(egui::Event::PointerButton {
        pos: target_position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
    assert_eq!(
        harness
            .state()
            .ui_state
            .terminal_settings_draft
            .node_shell_presets
            .iter()
            .map(|preset| preset.name.as_str())
            .collect::<Vec<_>>(),
        ["Ubuntu", "Netshoot", "Busybox"]
    );
}

#[test]
fn node_shell_preset_table_edits_and_saves_a_profile() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_settings_open = true;
    state.terminal_settings_draft = TerminalLaunchSettings {
        custom_template: None,
        node_shell_presets: vec![NodeShellPreset {
            name: String::new(),
            image: String::new(),
            profile: DebugProfile::General,
        }],
    };
    harness.state_mut().ui_state = state;
    harness.run();

    type_text(&mut harness, "Node shell 1 name", "Operations");
    type_text(
        &mut harness,
        "Node shell 1 image",
        "registry.example/debug-tools:v1",
    );
    let profile_position = harness
        .get_by_role_and_label(
            egui::accesskit::Role::ComboBox,
            "Node shell 1 debug profile",
        )
        .rect()
        .center();
    primary_click(&mut harness, profile_position);
    harness.run();
    let profile_position = harness.get_by_label("System admin").rect().center();
    primary_click(&mut harness, profile_position);
    harness.run();
    let save_position = harness.get_by_label("Save changes").rect().center();
    primary_click(&mut harness, save_position);
    harness.run_steps(2);

    assert_eq!(
        harness.state().terminal_launch_settings.node_shell_presets,
        vec![NodeShellPreset {
            name: "Operations".into(),
            image: "registry.example/debug-tools:v1".into(),
            profile: DebugProfile::Sysadmin,
        }]
    );
}

#[test]
fn node_shell_preset_profile_menu_stays_within_the_settings_blade() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    state.terminal_settings_open = true;
    state.terminal_settings_draft = TerminalLaunchSettings {
        custom_template: Some("alacritty -e {command}".into()),
        ..Default::default()
    };
    harness.state_mut().ui_state = state;
    harness.run();

    let profile_position = harness
        .get_by_role_and_label(
            egui::accesskit::Role::ComboBox,
            "Node shell 1 debug profile",
        )
        .rect()
        .center();
    primary_click(&mut harness, profile_position);
    harness.run();

    harness.get_by_label("System admin");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/node_shell_preset_profile_menu_stays_within_the_settings_blade/profile_menu",
    ));
}

#[test]
fn settings_blade_shows_invalid_custom_template_after_save() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();

    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(&mut harness, settings_position);
    harness.run();
    let custom_launcher_position = harness
        .get_by_role_and_label(egui::accesskit::Role::RadioButton, "Custom launcher")
        .rect()
        .center();
    primary_click(&mut harness, custom_launcher_position);
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
    let save_position = harness.get_by_label("Save changes").rect().center();
    primary_click(&mut harness, save_position);
    harness.run();

    assert_eq!(
        harness.state().ui_state.terminal_settings_error.as_deref(),
        Some("The launcher template must contain exactly one {command} placeholder.")
    );
    harness.get_by_label("Command template needs attention");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/settings_blade_shows_invalid_custom_template_after_save/settings_terminal_launcher_invalid",
    ));
}

#[test]
fn resource_name_opens_and_closes_a_live_detail_inspector() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let name = "coredns-66bc5c9577-ffw2s";
    let resource_position = harness
        .get_by_label(&format!("Open details for {name}"))
        .rect()
        .center();
    primary_click(&mut harness, resource_position);
    harness.run_steps(1);

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .is_some_and(|command| {
                    command.cluster_key == 2
                        && command.resource_name == name
                        && command.resource_uid == "fixture-0"
                        && command.history_entry_id == 1
                })),
        "commands: {:?}",
        harness.state().worker.commands
    );

    let pods = fixture_api_resource("core", "Pod", "pods");
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                api_resource: pods,
                name: name.into(),
                namespace: Some("kube-system".into()),
                uid: "fixture-0".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: None,
                owners: Vec::new(),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Generic,
            }),
        }) as WorkerResultBox);
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .as_ref()
            .and_then(|panel| panel.detail.as_ref())
            .is_some()
    );
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 1.0);
    harness.get_by_label("Close blade").click_accesskit();
    harness.run_steps(2);

    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_some(),
        "the inspector remains present while its close animation is in progress"
    );
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run();

    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none()
    );
    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StopResourceDetailWatch>()
                .is_some_and(|command| command.cluster_key == 2))
    );
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

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .is_some_and(|command| {
                    command.api_resource == crate::resource_handlers::node::api_resource()
                        && command.namespace.is_none()
                        && command.resource_name == "kind-control-plane"
                        && command.resource_uid == "kind-control-plane"
                }))
    );
}

#[test]
fn clicking_a_controller_owner_in_the_resource_table_opens_its_inspector() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let replica_set = fixture_api_resource("apps", "ReplicaSet", "replicasets");
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.resource_navigation =
        build_resource_navigation(vec![pods.clone(), replica_set.clone()]);
    let pod = cluster
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .expect("Pod watch fixture exists")
        .resources
        .values_mut()
        .next()
        .expect("Pod fixture exists");
    pod.controller_owner = Some(ResourceOwner {
        api_version: "apps/v1".into(),
        kind: "ReplicaSet".into(),
        name: "api-7b948f".into(),
        uid: "replicaset-uid".into(),
        controller: true,
    });
    harness.state_mut().ui_state = state;
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/controller_owner_link_opens_its_inspector/controller_owner_link",
    ));

    let owner_position = harness
        .get_by_label("Open details for ReplicaSet / api-7b948f")
        .rect()
        .center();
    primary_click(&mut harness, owner_position);
    harness.run_steps(1);

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .is_some_and(|command| {
                    command.api_resource == replica_set
                        && command.namespace.as_deref() == Some("kube-system")
                        && command.resource_name == "api-7b948f"
                        && command.resource_uid == "replicaset-uid"
                }))
    );
}

#[test]
fn owner_reference_card_lists_every_owner_and_navigates_resolved_links() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let pod = fixture_api_resource("core", "Pod", "pods");
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    open_typed_detail(
        &mut harness,
        pod.clone(),
        ResourceDetail {
            api_resource: pod,
            name: "api-pod".into(),
            namespace: Some("kube-system".into()),
            uid: "pod-uid".into(),
            resource_version: "1".into(),
            is_deleting: false,
            finalizers: Vec::new(),
            creation_timestamp: None,
            owners: vec![
                ResourceOwner {
                    api_version: "apps/v1".into(),
                    kind: "Deployment".into(),
                    name: "api".into(),
                    uid: "deployment-uid".into(),
                    controller: true,
                },
                ResourceOwner {
                    api_version: "example.dev/v1".into(),
                    kind: "Widget".into(),
                    name: "api-widget".into(),
                    uid: "widget-uid".into(),
                    controller: false,
                },
            ],
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            payload: ResourceDetailPayload::Generic,
        },
    );

    harness.get_by_label("Owner references").click_accesskit();
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/owner_reference_card_lists_every_owner_and_navigates_resolved_links/owner_references",
    ));
    harness.get_by_label("Widget / api-widget");
    let owner_position = harness
        .get_by_label("Open details for Deployment / api")
        .rect()
        .center();
    primary_click(&mut harness, owner_position);
    harness.run_steps(1);

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .is_some_and(|command| {
                    command.api_resource == deployment
                        && command.namespace.as_deref() == Some("kube-system")
                        && command.resource_name == "api"
                        && command.resource_uid == "deployment-uid"
                        && command.history_entry_id == 2
                }))
    );
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
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                api_resource: pods,
                name: "api".into(),
                namespace: Some("kube-system".into()),
                uid: "pod-uid".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: None,
                owners: Vec::new(),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Pod(Box::new(overflowing_pod_detail())),
            }),
        }) as WorkerResultBox);
    harness.run_steps(2);

    let node_position = harness
        .get_by_label("Open details for Node kind-control-plane")
        .rect()
        .center();
    primary_click(&mut harness, node_position);
    harness.run_steps(1);

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .is_some_and(|command| {
                    command.api_resource == crate::resource_handlers::node::api_resource()
                        && command.namespace.is_none()
                        && command.resource_name == "kind-control-plane"
                        && command.resource_uid == "kind-control-plane"
                        && command.history_entry_id == 2
                }))
    );
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
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                api_resource: crate::resource_handlers::node::api_resource(),
                name: "kind-control-plane".into(),
                namespace: None,
                uid: "node-uid".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: None,
                owners: Vec::new(),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Node(NodeDetail {
                    pod_cidrs: vec!["10.244.0.0/24".into()],
                    provider_id: Some("kind://docker/kind/kind-control-plane".into()),
                    unschedulable: true,
                    taints: vec!["node-role.kubernetes.io/control-plane:NoSchedule".into()],
                }),
            }),
        }) as WorkerResultBox);
    harness.run_steps(2);

    harness.get_by_label("Spec");
    harness.get_by_label("Scheduling disabled");
    harness.get_by_label("10.244.0.0/24");
    harness.get_by_label("node-role.kubernetes.io/control-plane:NoSchedule");
}

#[test]
fn node_inspector_shell_action_launches_the_selected_preset() {
    let mut harness = application_harness_with_terminal::<MockWorker, MockTerminalLauncher>();
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
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                api_resource: crate::resource_handlers::node::api_resource(),
                name: "kind-control-plane".into(),
                namespace: None,
                uid: "node-uid".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: None,
                owners: Vec::new(),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Node(NodeDetail::default()),
            }),
        }) as WorkerResultBox);
    harness.run_steps(2);

    let more_actions_position = harness
        .get_by_label("More actions for kind-control-plane")
        .rect()
        .center();
    primary_click(&mut harness, more_actions_position);
    harness.run();
    let shell_position = harness.get_by_label("Shell ⏵").rect().center();
    primary_click(&mut harness, shell_position);
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/node_inspector_shell_action_launches_the_selected_preset/node_shell_presets",
    ));
    let ubuntu_position = harness.get_by_label("Ubuntu — General").rect().center();
    primary_click(&mut harness, ubuntu_position);
    harness.run();

    assert_eq!(
        harness.state().terminal_launcher.requests.as_slice(),
        &[ShellRequest::Node {
            kube_context: "kind-kind".into(),
            node_name: "kind-control-plane".into(),
            preset: NodeShellPreset {
                name: "Ubuntu".into(),
                image: "ubuntu".into(),
                profile: DebugProfile::General,
            },
        }]
    );
}

#[test]
fn node_inspector_lists_cross_namespace_pods_in_the_shared_pod_table() {
    let mut harness = application_harness::<MockWorker>();
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
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
        Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                api_resource: nodes,
                name: "kind-control-plane".into(),
                namespace: None,
                uid: "node-uid".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: None,
                owners: Vec::new(),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Node(NodeDetail::default()),
            }),
        }) as WorkerResultBox,
        Box::new(ManagedResourcesReplaced {
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
        }) as WorkerResultBox,
    ]);
    harness.run_steps(2);

    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/node_inspector_lists_cross_namespace_pods_in_the_shared_pod_table/node_inspector_with_scheduled_pods",
    ));
    harness.get_by_label("monitoring");
    harness.get_by_label("Open details for api");
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
    harness.state_mut().worker.commands.append(&mut commands);
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
        harness.state_mut().worker.commands.append(&mut commands);
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
        harness.state_mut().worker.commands.append(&mut commands);
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
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
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
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Generic,
    };
    open_typed_detail(&mut harness, deployment, detail);
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ManagedResourcesReplaced {
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
        }) as WorkerResultBox);
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/managed_resource_tables_navigate_with_back_and_forward_history/deployment_managed_resource_tables",
    ));
    let replica_set_position = harness
        .get_by_label("Open details for api-7b948f")
        .rect()
        .center();
    primary_click(&mut harness, replica_set_position);
    harness.run_steps(1);

    let panel = harness.state().ui_state.clusters[&2]
        .resource_detail_panel
        .as_ref()
        .expect("inspector should remain open");
    assert_eq!(panel.api_resource, replica_set);
    assert_eq!(panel.navigator.back_stack().len(), 1);
    assert!(panel.navigator.forward_stack().is_empty());
    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .is_some_and(|command| {
                    command.resource_name == "api-7b948f"
                        && command.resource_uid == "replicaset-uid"
                        && command.history_entry_id == 2
                }))
    );

    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 2,
            detail: Box::new(ResourceDetail {
                api_resource: replica_set.clone(),
                name: "api-7b948f".into(),
                namespace: Some("kube-system".into()),
                uid: "replicaset-uid".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: Some(
                    time::OffsetDateTime::now_utc() - time::Duration::hours(2),
                ),
                owners: vec![crate::resource_detail::ResourceOwner {
                    api_version: "apps/v1".into(),
                    kind: "Deployment".into(),
                    name: "api".into(),
                    uid: "deployment-uid".into(),
                    controller: true,
                }],
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Generic,
            }),
        }) as WorkerResultBox);
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ManagedResourcesReplaced {
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
        }) as WorkerResultBox);
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
    harness.ui_harness("resource_inspectors/managed_resource_tables_navigate_with_back_and_forward_history/replica_set_inspector_with_back_history");

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
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/managed_resource_tables_navigate_with_back_and_forward_history/deployment_inspector_with_forward_history",
    ));

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
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id,
            detail: Box::new(ResourceDetail {
                api_resource: pod.clone(),
                name: "api-7b948f-pod".into(),
                namespace: Some("kube-system".into()),
                uid: "pod-uid".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: Some(
                    time::OffsetDateTime::now_utc() - time::Duration::minutes(15),
                ),
                owners: vec![crate::resource_detail::ResourceOwner {
                    api_version: "apps/v1".into(),
                    kind: "ReplicaSet".into(),
                    name: "api-7b948f".into(),
                    uid: "replicaset-uid".into(),
                    controller: true,
                }],
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Pod(Box::new(overflowing_pod_detail())),
            }),
        }) as WorkerResultBox);
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
    harness.ui_harness("resource_inspectors/managed_resource_tables_navigate_with_back_and_forward_history/pod_inspector_with_two_back_history_blades");

    let detail_watch_starts_before_back = harness
        .state()
        .worker
        .commands
        .iter()
        .filter(|command| command_is::<StartResourceDetailWatch>(command).is_some())
        .count();
    harness.get_by_label("Back").click_accesskit();
    harness.run_steps(1);
    assert_eq!(
        harness
            .state()
            .worker
            .commands
            .iter()
            .filter(|command| command_is::<StartResourceDetailWatch>(command).is_some())
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
            .filter(|command| command_is::<StartResourceDetailWatch>(command).is_some())
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
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id,
            detail: Box::new(ResourceDetail {
                api_resource: pod,
                name: "api-7b948f-pod-debug".into(),
                namespace: Some("kube-system".into()),
                uid: "pod-debug-uid".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: Some(
                    time::OffsetDateTime::now_utc() - time::Duration::minutes(10),
                ),
                owners: Vec::new(),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Generic,
            }),
        }) as WorkerResultBox);
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
    harness.ui_harness("resource_inspectors/managed_resource_tables_navigate_with_back_and_forward_history/pod_inspector_with_three_back_history_entries");
}

#[test]
fn pod_resource_detail_inspector_snapshot() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let name = "coredns-66bc5c9577-ffw2s";
    let resource_position = harness
        .get_by_label(&format!("Open details for {name}"))
        .rect()
        .center();
    primary_click(&mut harness, resource_position);
    harness.run_steps(1);
    let pods = fixture_api_resource("core", "Pod", "pods");
    harness.state_mut().worker.results.extend([
        Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                api_resource: pods,
                name: name.into(),
                namespace: Some("kube-system".into()),
                uid: "fixture-0".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: None,
                owners: Vec::new(),
                labels: BTreeMap::from([("k8s-app".into(), "kube-dns".into())]),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Pod(Box::new(PodDetail {
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
                })),
            }),
        }) as WorkerResultBox,
        Box::new(ResourceEventsReplaced {
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
        }) as WorkerResultBox,
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

    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/pod_resource_detail_inspector_snapshot/pod_resource_detail_inspector",
    ));
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
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(detail),
        }) as WorkerResultBox);
    harness.run();
}

fn config_map_detail(data: BTreeMap<String, String>) -> ResourceDetail {
    ResourceDetail {
        api_resource: fixture_api_resource("core", "ConfigMap", "configmaps"),
        name: "settings".into(),
        namespace: Some("kube-system".into()),
        uid: "configmap-uid".into(),
        resource_version: "1".into(),
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::ConfigMap(ConfigMapDetail {
            data,
            immutable: false,
        }),
    }
}

#[test]
fn deployment_editor_completes_match_labels_in_a_selector() {
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let detail = ResourceDetail {
        api_resource: deployment.clone(),
        name: "coredns".into(),
        namespace: Some("kube-system".into()),
        uid: "deployment-match-labels-uid".into(),
        resource_version: "1".into(),
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        payload: ResourceDetailPayload::Generic,
    };
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    open_typed_detail(&mut harness, deployment.clone(), detail);

    harness
        .get_by_label("More actions for coredns")
        .click_accesskit();
    harness.run();
    harness.get_by_label("Edit").click_accesskit();
    harness.run();

    let editor_id = harness
        .state()
        .worker
        .commands
        .iter()
        .find_map(|command| {
            command
                .as_ref()
                .as_any()
                .downcast_ref::<GetResourceYaml>()
                .map(|command| command.editor_id)
        })
        .expect("editing the deployment fetches its YAML");
    let original_yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: coredns\nspec:\n  selector:\n    matchLabels:\n      app: coredns";
    let partial_yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: coredns\nspec:\n  selector:\n    match";
    assert!(
        deployment_selector_schema()
            .completion_at(partial_yaml, partial_yaml.chars().count())
            .suggestions
            .iter()
            .any(|suggestion| suggestion.label == "matchLabels"),
        "the deployment schema should complete the partial selector key"
    );
    harness.state_mut().worker.results.extend([
        Box::new(ResourceYamlFetched {
            editor_id,
            cluster_key: 2,
            api_resource: deployment.clone(),
            namespace: Some("kube-system".into()),
            resource_name: "coredns".into(),
            yaml: original_yaml.into(),
        }) as WorkerResultBox,
        Box::new(ResourceSchemaLoaded {
            editor_id,
            cluster_key: 2,
            api_resource: deployment.clone(),
            schema: deployment_selector_schema(),
        }) as WorkerResultBox,
    ]);
    harness.run();

    let mut editor = harness
        .state()
        .ui_state
        .yaml_editors
        .get(&editor_id)
        .expect("deployment editor remains open")
        .clone();
    editor.edited_yaml = partial_yaml.into();
    editor.validation_revision = 0;
    editor.validation_due = None;
    editor.diagnostics.clear();
    editor.retained_diagnostics.clear();
    editor.server_validation = ValidationState::Idle;
    let mut snapshot_harness = Harness::builder().build_ui_state(
        |ctx, state: &mut YamlEditorSnapshotState| {
            super::super::yaml_editor::show_editor_window(
                ctx,
                &mut state.editor,
                &mut state.commands,
            );
        },
        YamlEditorSnapshotState {
            editor,
            commands: Vec::new(),
        },
    );
    components::test_support::setup_egui(&mut snapshot_harness);
    snapshot_harness.run();

    let text_edit_id = egui::Id::new(("yaml-editor-text", editor_id));
    let mut text_edit_state =
        egui::widgets::text_edit::TextEditState::load(&snapshot_harness.ctx, text_edit_id)
            .expect("the deployment YAML editor has rendered");
    let cursor = partial_yaml.chars().count();
    text_edit_state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(cursor))));
    text_edit_state.store(&snapshot_harness.ctx, text_edit_id);
    snapshot_harness
        .ctx
        .memory_mut(|memory| memory.request_focus(text_edit_id));

    snapshot_harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Space);
    snapshot_harness.run();

    let editor = &snapshot_harness.state().editor;
    assert!(editor.suggestions_visible, "editor state: {editor:#?}");
    assert_eq!(editor.suggestions[0].label, "matchLabels");
    snapshot_harness.ui_harness("resource_editor/deployment_editor_completes_match_labels_in_a_selector/deployment_editor_match_labels_completion");
}

fn deployment_selector_schema() -> ResourceSchema {
    ResourceSchema::new(json!({
        "type": "object",
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"type": "object"},
            "spec": {
                "type": "object",
                "properties": {
                    "selector": {
                        "description": "Label selector for the Pods managed by this Deployment.",
                        "allOf": [{"$ref": "#/components/schemas/LabelSelector"}]
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "LabelSelector": {
                    "type": "object",
                    "properties": {
                        "matchLabels": {
                            "type": "object",
                            "description": "Map of label keys and values that must match the selected Pods.",
                            "additionalProperties": {"type": "string"}
                        },
                        "matchExpressions": {
                            "type": "array",
                            "description": "Requirements for selecting Pods by label.",
                            "items": {"type": "object"}
                        }
                    }
                }
            }
        }
    }))
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
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
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

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<UpdateResourceData>()
                .is_some_and(|command| {
                    command.history_entry_id > 0
                        && command.request_id > 0
                        && command.update.expected_resource_version == "1"
                        && command.update.expected_values
                            == BTreeMap::from([("mode".into(), "development".into())])
                        && command.update.updated_values
                            == BTreeMap::from([("mode".into(), "production".into())])
                }))
    );
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

    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/config_map_resource_detail_inspector_snapshot/config_map_resource_detail_inspector",
    ));
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
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_actions/resource_detail_more_actions_use_the_shared_resource_menu/config_map_resource_detail_actions",
    ));
    harness.get_by_label("Edit").click_accesskit();
    harness.run();
    assert!(
        harness
            .state()
            .worker
            .commands
            .iter()
            .rev()
            .nth(1)
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<GetResourceYaml>()
                .is_some_and(|command| command.resource_name == "settings"))
    );
    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<LoadResourceSchema>()
                .is_some())
    );

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
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
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
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
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
            }),
        }) as WorkerResultBox);
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
        is_deleting: false,
        finalizers: Vec::new(),
        creation_timestamp: None,
        owners: Vec::new(),
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

    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/secret_resource_detail_inspector_snapshot/secret_resource_detail_inspector",
    ));
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
    harness.ui_harness(
        "resource_tables/resource_table_reflows_after_viewport_resize/resource_table_narrow",
    );

    components::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness(
        "resource_tables/resource_table_reflows_after_viewport_resize/resource_table_resized",
    );
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
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/cluster_rail_shows_connection_status_marker_and_tooltip/cluster_rail_connection_status",
    ));
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
        [command] if command.as_ref().as_any().downcast_ref::<DeleteResource>()
            .is_some_and(|command| command.resource_name == "important-config")
    ));
}

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
                    .downcast_ref::<StartResourceWatch>()
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
            .downcast_ref::<StartResourceWatch>()
            .is_some_and(|command| {
                command.cluster_key == 1
                    && command.api_resource == pods
                    && command.namespace.as_deref() == Some("default")
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
            .filter(|command| command_is::<StartResourceWatch>(command).is_some())
            .count(),
        2
    );
}
