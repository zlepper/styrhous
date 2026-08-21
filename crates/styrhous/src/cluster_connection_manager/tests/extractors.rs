use super::*;

#[test]
fn pod_extractor_populates_ready_status_and_restarts() {
    let pod = Pod {
        metadata: ObjectMeta {
            name: Some("api".to_owned()),
            namespace: Some("default".to_owned()),
            uid: Some("pod-uid".to_owned()),
            ..Default::default()
        },
        status: Some(PodStatus {
            phase: Some("Running".to_owned()),
            container_statuses: Some(vec![
                ContainerStatus {
                    ready: true,
                    restart_count: 2,
                    ..Default::default()
                },
                ContainerStatus {
                    ready: false,
                    restart_count: 3,
                    ..Default::default()
                },
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resource = crate::resource_handlers::pod::extract(&pod);

    assert_eq!(resource.uid, "pod-uid");
    assert_eq!(
        resource.cells.get(READY_COLUMN),
        Some(&CellValue::Text("1/2".to_owned()))
    );
    assert_eq!(
        resource.cells.get(STATUS_COLUMN),
        Some(&CellValue::Status {
            label: "Running".to_owned(),
            tone: crate::resource_table::StatusTone::Success,
        })
    );
    assert_eq!(
        resource.cells.get(RESTARTS_COLUMN),
        Some(&CellValue::Number(5))
    );
}

#[test]
fn deployment_extractor_populates_replica_columns() {
    let deployment = Deployment {
        metadata: ObjectMeta {
            name: Some("api".to_owned()),
            namespace: Some("default".to_owned()),
            ..Default::default()
        },
        status: Some(DeploymentStatus {
            replicas: Some(4),
            ready_replicas: Some(3),
            updated_replicas: Some(2),
            available_replicas: Some(3),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resource = crate::resource_handlers::deployment::extract(&deployment);

    assert_eq!(
        resource.cells.get(READY_COLUMN),
        Some(&CellValue::Text("3/4".to_owned()))
    );
    assert_eq!(
        resource.cells.get(UP_TO_DATE_COLUMN),
        Some(&CellValue::Number(2))
    );
    assert_eq!(
        resource.cells.get(AVAILABLE_COLUMN),
        Some(&CellValue::Number(3))
    );
}

#[test]
fn dynamic_resource_extractor_evaluates_custom_printer_columns_locally() {
    let columns = vec![CustomResourceColumn {
        id: "crd-0".to_owned(),
        label: "State".to_owned(),
        json_path: ".status.conditions[*].type".to_owned(),
        type_: "string".to_owned(),
        format: None,
    }];

    let cells = extract_custom_cells(
        &k8s_openapi::serde_json::json!({
            "status": { "conditions": [{ "type": "Ready" }, { "type": "Synced" }] }
        }),
        &columns,
    );

    assert_eq!(
        cells.get("crd-0"),
        Some(&CellValue::List(vec![
            "Ready".to_owned(),
            "Synced".to_owned()
        ]))
    );

    let resource = extract_minimal_resource(
        &DynamicObject {
            types: None,
            metadata: ObjectMeta {
                name: Some("widget".into()),
                labels: Some(BTreeMap::from([("app".into(), "api".into())])),
                annotations: Some(BTreeMap::from([(
                    "example.com/team".into(),
                    "platform".into(),
                )])),
                ..Default::default()
            },
            data: k8s_openapi::serde_json::json!({}),
        },
        &[],
    );

    assert_eq!(resource.labels["app"], "api");
    assert_eq!(resource.annotations["example.com/team"], "platform");
}

#[test]
fn managed_resource_tree_keeps_only_supported_controller_descendants() {
    let replica_set = ManagedResource {
        api_resource: api_resource_for::<ReplicaSet>(),
        name: "api-7b948f".into(),
        namespace: Some("default".into()),
        uid: "replicaset-uid".into(),
        association: ManagedResourceAssociation::ControllerOwnerUid("deployment-uid".into()),
        creation_timestamp: None,
        cells: BTreeMap::new(),
    };
    let pod = ManagedResource {
        api_resource: api_resource_for::<Pod>(),
        name: "api-7b948f-pod".into(),
        namespace: Some("default".into()),
        uid: "pod-uid".into(),
        association: ManagedResourceAssociation::ControllerOwnerUid("replicaset-uid".into()),
        creation_timestamp: None,
        cells: BTreeMap::new(),
    };
    let unrelated_pod = ManagedResource {
        uid: "other-pod-uid".into(),
        association: ManagedResourceAssociation::ControllerOwnerUid("other-uid".into()),
        ..pod.clone()
    };
    let resources = BTreeMap::from([
        (replica_set.api_resource.clone(), vec![replica_set.clone()]),
        (
            pod.api_resource.clone(),
            vec![pod.clone(), unrelated_pod.clone()],
        ),
    ]);

    assert!(belongs_to_workload_tree(
        &replica_set,
        "deployment-uid",
        &api_resource_for::<Deployment>(),
        &resources
    ));
    assert!(belongs_to_workload_tree(
        &pod,
        "deployment-uid",
        &api_resource_for::<Deployment>(),
        &resources
    ));
    assert!(!belongs_to_workload_tree(
        &unrelated_pod,
        "deployment-uid",
        &api_resource_for::<Deployment>(),
        &resources
    ));
    let directly_owned_pod = ManagedResource {
        association: ManagedResourceAssociation::ControllerOwnerUid("deployment-uid".into()),
        ..pod.clone()
    };
    assert!(!belongs_to_workload_tree(
        &directly_owned_pod,
        "deployment-uid",
        &api_resource_for::<Deployment>(),
        &resources
    ));
}

#[test]
fn node_association_keeps_only_pods_scheduled_to_the_inspected_node() {
    let scheduled = ManagedResource {
        api_resource: api_resource_for::<Pod>(),
        name: "api".into(),
        namespace: Some("default".into()),
        uid: "pod-uid".into(),
        association: ManagedResourceAssociation::NodeName("kind-control-plane".into()),
        creation_timestamp: None,
        cells: BTreeMap::new(),
    };
    let elsewhere = ManagedResource {
        association: ManagedResourceAssociation::NodeName("kind-worker".into()),
        ..scheduled.clone()
    };

    assert!(belongs_to_node(&scheduled, "kind-control-plane"));
    assert!(!belongs_to_node(&elsewhere, "kind-control-plane"));
}
