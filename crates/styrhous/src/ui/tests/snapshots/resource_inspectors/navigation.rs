use super::*;

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
