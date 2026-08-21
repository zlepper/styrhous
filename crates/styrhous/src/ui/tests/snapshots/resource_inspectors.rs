//! Resource-inspector and editor scenarios.

use super::*;

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
                    allocatable: PodResourceThresholds {
                        cpu_nanocores: Some(2_000_000_000),
                        memory_bytes: Some(4 * 1024 * 1024 * 1024),
                    },
                }),
            }),
        }) as WorkerResultBox);
    // Model the Metrics API's 15-second cadence across the full retained window. Offset the
    // series slightly into the future so all samples survive the rolling history prune while
    // the harness renders; chart positions are clamped to the window's current edge.
    let now = OffsetDateTime::now_utc();
    let last_sample = now + time::Duration::seconds(10);
    let first_sample = last_sample - POD_USAGE_HISTORY_WINDOW;
    for sample_index in 0_i64..=40 {
        harness
            .state_mut()
            .worker
            .results
            .push_back(Box::new(ResourceDetailNodeUsageUpdated {
                cluster_key: 2,
                history_entry_id: 1,
                usage: NodeUsage {
                    timestamp: first_sample + time::Duration::seconds(sample_index * 15),
                    cpu_nanocores: 350_000_000 + sample_index * 3_750_000,
                    memory_bytes: 768 * 1024 * 1024 + (256 * 1024 * 1024 * sample_index / 40),
                },
            }) as WorkerResultBox);
    }
    harness.run_steps(42);
    let usage_history = &harness
        .state_mut()
        .ui_state
        .resource_detail_entry_mut(1)
        .expect("node inspector should remain open")
        .node_usage_history;
    assert_eq!(usage_history.len(), 41);
    assert_eq!(
        usage_history.first().map(|sample| sample.timestamp),
        Some(first_sample)
    );
    assert_eq!(
        usage_history.last().map(|sample| sample.timestamp),
        Some(last_sample)
    );
    assert_eq!(
        usage_history.last().map(|sample| sample.cpu_nanocores),
        Some(500_000_000)
    );
    assert_eq!(
        usage_history.last().map(|sample| sample.memory_bytes),
        Some(1024 * 1024 * 1024)
    );

    harness.get_by_label("Spec");
    harness.get_by_label("Scheduling disabled");
    harness.get_by_label("10.244.0.0/24");
    harness.get_by_label("node-role.kubernetes.io/control-plane:NoSchedule");
    harness.get_by_label("Resource usage");
    harness.get_by_label("500m");
    harness.get_by_label(
        "Node CPU usage chart; usage history available; 10-minute history; scale from 0 to 2; Allocatable 2",
    );
    harness.get_by_label(
        "Node memory usage chart; usage history available; 10-minute history; scale from 0 to 4Gi; Allocatable 4Gi",
    );
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/node_inspector_shows_its_spec/populated_node_spec",
    ));
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(NodeMetricsApiUnavailable { cluster_key: 2 }) as WorkerResultBox);
    harness.run();
    harness.get_by_label("Metrics API unavailable");
    harness.get_by_label("CPU allocatable");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/node_inspector_shows_its_spec/node_usage_unavailable",
    ));
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
            preset: DebugImagePreset {
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
        "d1f2c3a4-b5e6-47f8-9a0b-1c2d3e4f5a6b".into(),
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
                uid: "d1f2c3a4-b5e6-47f8-9a0b-1c2d3e4f5a6b".into(),
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
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);

    let name_resize_handle = harness
        .get_all_by_label("Resize Name column")
        .max_by(|left, right| left.rect().left().total_cmp(&right.rect().left()))
        .expect("the inspector managed-resource table has a Name resize handle")
        .rect();
    secondary_click(
        &mut harness,
        egui::pos2(
            name_resize_handle.left() - 80.0,
            name_resize_handle.center().y,
        ),
    );
    harness.run_steps(2);
    let configure_columns = harness.get_by_label("Configure columns").rect().center();
    primary_click(&mut harness, configure_columns);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run_steps(5);
    harness.ui_harness(
        "resource_inspectors/node_inspector_lists_cross_namespace_pods_in_the_shared_pod_table/column_settings_in_shared_blade_stack",
    );

    harness.get_by_label("Go back one blade").click();
    harness.run_steps(4);
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

    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_some()
    );
    assert_eq!(
        harness
            .state()
            .ui_state
            .global_blades
            .navigator()
            .unwrap()
            .current()
            .resource_detail()
            .unwrap()
            .api_resource,
        deployment
    );
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
        .global_blades
        .navigator_mut()
        .unwrap()
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
            .global_blades
            .navigator_mut()
            .unwrap()
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
            .global_blades
            .navigator_mut()
            .unwrap()
            .clear_transition();
        harness.run_steps(1);
    }

    let navigator = harness.state().ui_state.global_blades.navigator().unwrap();
    assert_eq!(
        navigator.current().resource_detail().unwrap().resource_name,
        "second"
    );
    assert_eq!(navigator.back_stack().len(), 1);
    let active_layer = egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("global-blade-stack").with(("blade", navigator.back_stack().len())),
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
    let replica_set_name = "digizuitecore-configurationmanagementservice-6558ccd787";
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
                    name: replica_set_name.into(),
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
    let replica_set_label = format!("Open details for {replica_set_name}");
    let replica_set_position = harness.get_by_label(&replica_set_label).rect().center();
    primary_click(&mut harness, replica_set_position);
    harness.run_steps(1);

    let navigator = harness.state().ui_state.global_blades.navigator().unwrap();
    assert_eq!(
        navigator.current().resource_detail().unwrap().api_resource,
        replica_set
    );
    assert_eq!(navigator.back_stack().len(), 1);
    assert!(navigator.forward_stack().is_empty());
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
                    command.resource_name == replica_set_name
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
                name: replica_set_name.into(),
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
        .global_blades
        .navigator_mut()
        .unwrap()
        .clear_transition();
    harness.run();
    harness.ui_harness("resource_inspectors/managed_resource_tables_navigate_with_back_and_forward_history/replica_set_inspector_with_back_history");

    harness.get_by_label("Back").click_accesskit();
    harness.run_steps(1);
    let navigator = harness.state().ui_state.global_blades.navigator().unwrap();
    assert_eq!(
        navigator
            .current()
            .resource_detail()
            .unwrap()
            .api_resource
            .kind,
        "Deployment"
    );
    assert!(navigator.back_stack().is_empty());
    assert_eq!(navigator.forward_stack().len(), 1);
    harness
        .state_mut()
        .ui_state
        .global_blades
        .navigator_mut()
        .unwrap()
        .clear_transition();
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/managed_resource_tables_navigate_with_back_and_forward_history/deployment_inspector_with_forward_history",
    ));

    harness.get_by_label("Forward").click_accesskit();
    harness.run_steps(1);
    assert_eq!(
        harness
            .state()
            .ui_state
            .global_blades
            .navigator()
            .unwrap()
            .current()
            .resource_detail()
            .unwrap()
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
    let history_entry_id = harness
        .state()
        .ui_state
        .global_blades
        .navigator()
        .unwrap()
        .current()
        .resource_detail()
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
        .global_blades
        .navigator_mut()
        .unwrap()
        .clear_transition();
    harness.run();
    assert_eq!(
        harness
            .state()
            .ui_state
            .global_blades
            .navigator()
            .unwrap()
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
        .global_blades
        .navigator_mut()
        .unwrap()
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
        .global_blades
        .navigator_mut()
        .unwrap()
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
    let history_entry_id = harness
        .state()
        .ui_state
        .global_blades
        .navigator()
        .unwrap()
        .current()
        .resource_detail()
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
        .global_blades
        .navigator_mut()
        .unwrap()
        .clear_transition();
    harness.run();
    assert_eq!(
        harness
            .state()
            .ui_state
            .global_blades
            .navigator()
            .unwrap()
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
                        resource_requests: PodResourceThresholds {
                            cpu_nanocores: Some(10_000_000),
                            memory_bytes: Some(16 * 1024 * 1024),
                        },
                        resource_limits: PodResourceThresholds {
                            cpu_nanocores: Some(100_000_000),
                            memory_bytes: Some(128 * 1024 * 1024),
                        },
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
    let now = OffsetDateTime::now_utc();
    for (seconds_ago, cpu_nanocores, memory_bytes) in [
        (90, 12_000_000, 20 * 1024 * 1024),
        (45, 18_000_000, 24 * 1024 * 1024),
        (0, 15_000_000, 22 * 1024 * 1024),
    ] {
        harness
            .state_mut()
            .worker
            .results
            .push_back(Box::new(ResourceDetailPodUsageUpdated {
                cluster_key: 2,
                history_entry_id: 1,
                usage: PodUsage {
                    timestamp: now - time::Duration::seconds(seconds_ago),
                    cpu_nanocores,
                    memory_bytes,
                    containers: BTreeMap::from([(
                        "coredns".into(),
                        ContainerUsage {
                            cpu_nanocores,
                            memory_bytes,
                        },
                    )]),
                },
            }) as WorkerResultBox);
    }
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

    harness.event(egui::Event::PointerGone);
    harness.run();
    // Chart points are positioned against the live clock, so allow their anti-aliased edges to
    // move by a fraction of a pixel between snapshot runs.
    harness.ui_harness(
        HarnessSnapshotOptions::strict(
            "resource_inspectors/pod_resource_detail_inspector_snapshot/pod_resource_detail_inspector",
        )
        .max_failed_pixels(100),
    );
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(PodMetricsApiUnavailable { cluster_key: 2 }) as WorkerResultBox);
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();
    harness.ui_harness(
        HarnessSnapshotOptions::strict(
            "resource_inspectors/pod_resource_detail_inspector_snapshot/pod_resource_detail_inspector_usage_unavailable",
        )
        .max_failed_pixels(100),
    );
    harness.get_by_label("Reveal").click_accesskit();
    harness.run();
    harness.get_by_label("test-token");
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
            super::super::super::yaml_editor::show_editor_window(
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
