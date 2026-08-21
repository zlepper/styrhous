use super::*;

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
