//! Resource selection and context-menu scenarios.

use super::*;

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
    let click_position = resource_name_rect.center();
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
