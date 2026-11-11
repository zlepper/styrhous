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
fn owner_reference_with_a_very_long_name_does_not_widen_the_inspector_blade() {
    let long_name = "a-deployment-with-an-extraordinarily-long-name-".repeat(3)
        + "that-cannot-possibly-fit-within-the-inspector-blade";
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let pod = fixture_api_resource("core", "Pod", "pods");
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
            owners: vec![ResourceOwner {
                api_version: "apps/v1".into(),
                kind: "Deployment".into(),
                name: long_name.clone(),
                uid: "deployment-uid".into(),
                controller: true,
            }],
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            payload: ResourceDetailPayload::Generic,
        },
    );

    harness.get_by_label("Owner references").click_accesskit();
    harness.run();
    harness.run();

    let viewport = harness.ctx.content_rect();
    let blade_left = viewport.right() - components::BLADE_WIDTH - 8.0;
    let close = harness.get_by_label("Close blade").rect();
    assert_eq!(
        close.right(),
        blade_left + components::BLADE_WIDTH - 1.0 - 24.0,
        "the inspector must keep its fixed width and position"
    );
    let link = harness.get_by_label_contains("Open details for Deployment / a-deployment-with");
    let link_position = link.rect().center();
    assert!(
        egui_kittest::kittest::NodeT::accesskit_node(&link)
            .label()
            .is_some_and(
                |label| label.ends_with("that-cannot-possibly-fit-within-the-inspector-blade")
            ),
        "the truncated link keeps the full owner name as its accessible label"
    );
    assert!(
        link.rect().right() < blade_left + components::BLADE_WIDTH - 24.0,
        "the truncated link stays inside the card"
    );
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_inspectors/owner_reference_with_a_very_long_name_does_not_widen_the_inspector_blade/long_owner",
    ));

    let full_label = format!("Deployment / {long_name}");
    assert_eq!(harness.query_all_by_label(&full_label).count(), 0);
    harness.event(egui::Event::PointerMoved(link_position));
    harness.run_steps(3);
    assert!(
        harness.query_all_by_label(&full_label).count() > 0,
        "hovering the truncated link shows the full owner name in a tooltip"
    );

    primary_click(&mut harness, link_position);
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
                .is_some_and(|command| command.resource_name == long_name
                    && command.resource_uid == "deployment-uid"))
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
fn unresolvable_owner_explains_why_its_details_are_unavailable_on_hover() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    let pod = fixture_api_resource("core", "Pod", "pods");
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
            owners: vec![ResourceOwner {
                api_version: "unknown.dev/v1".into(),
                kind: "Gadget".into(),
                name: "gadget".into(),
                uid: "gadget-uid".into(),
                controller: false,
            }],
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            payload: ResourceDetailPayload::Generic,
        },
    );

    harness.get_by_label("Owner references").click_accesskit();
    harness.run();
    let position = harness.get_by_label("Gadget / gadget").rect().center();
    harness.event(egui::Event::PointerMoved(position));
    harness.run_steps(3);
    harness.get_by_label(
        "Details for Gadget / gadget are unavailable because this resource type is not available on the cluster.",
    );
}
