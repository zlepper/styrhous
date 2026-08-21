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
