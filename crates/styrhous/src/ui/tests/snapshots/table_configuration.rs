//! Table configuration and confirmation-dialog scenarios.

use super::*;

#[test]
fn resource_table_header_menu_opens_the_column_settings_blade() {
    let mut harness = application_harness::<MockWorker>();
    show_apps_resource_table(&mut harness);

    let header = resource_table_name_header_context_position(&harness);
    secondary_click(&mut harness, header);
    harness.run_steps(2);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/resource_table_column_configuration/header_context_menu",
    ));

    let configure = harness.get_by_label("Configure columns").rect().center();
    primary_click(&mut harness, configure);
    harness.run_steps(2);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/resource_table_column_configuration/settings_blade",
    ));
}

#[test]
fn resource_table_column_settings_hide_and_reorder_columns() {
    let mut harness = application_harness::<MockWorker>();
    show_apps_resource_table(&mut harness);
    open_workspace_column_settings(&mut harness);

    let owner = harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Owner")
        .rect();
    primary_click(&mut harness, owner.center());
    harness.run();
    let age_checkbox = harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Age")
        .rect();
    let age_handle = egui::pos2(age_checkbox.left() - 38.0, age_checkbox.center().y);
    let name_handle = harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Name")
        .rect();
    drag(
        &mut harness,
        age_handle,
        egui::pos2(age_handle.x, name_handle.top()),
    );
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/resource_table_column_configuration/reordered_settings",
    ));

    harness.state_mut().ui_state.global_blades.clear();
    harness.run_steps(2);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/resource_table_column_configuration/configured_table",
    ));
}

#[test]
fn resource_table_custom_metadata_column_can_be_added_from_column_settings() {
    let mut harness = application_harness::<MockWorker>();
    show_apps_resource_table(&mut harness);
    open_workspace_column_settings(&mut harness);

    harness.get_by_label("Add custom column").click();
    harness.run_steps(2);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/resource_table_column_configuration/custom_column_form",
    ));

    let metadata_key = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Metadata key")
        .rect()
        .center();
    primary_click(&mut harness, metadata_key);
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("app.kubernetes.io/name".into()));
    harness.run();
    let column_header = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Column header")
        .rect()
        .center();
    primary_click(&mut harness, column_header);
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("Application".into()));
    harness.run();
    let add_column = harness.get_by_label("Add column").rect().center();
    primary_click(&mut harness, add_column);
    harness.run_steps(2);

    let pods = fixture_api_resource("core", "Pod", "pods");
    let column = harness
        .state_mut()
        .resource_table_preferences
        .custom_columns(&ResourceTableKey::workspace(&pods));
    assert_eq!(column[0].key, "app.kubernetes.io/name");
    assert_eq!(column[0].label, "Application");

    harness.state_mut().ui_state.global_blades.clear();
    harness.run_steps(2);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/resource_table_column_configuration/table_with_custom_metadata_column",
    ));

    open_workspace_column_settings(&mut harness);
    let remove_column = harness
        .get_by_label("Remove Application column")
        .rect()
        .center();
    primary_click(&mut harness, remove_column);
    harness.run_steps(2);
    assert!(
        harness
            .state_mut()
            .resource_table_preferences
            .custom_columns(&ResourceTableKey::workspace(&pods))
            .is_empty()
    );
}

#[test]
fn resource_table_resizes_and_horizontally_scrolls() {
    let mut harness = application_harness::<MockWorker>();
    show_apps_resource_table(&mut harness);
    harness.set_size(egui::vec2(850.0, 1024.0));
    harness.run_steps(2);

    let original_name_width = resource_table_name_width(&mut harness);
    let resize_handle = resource_table_name_resize_handle(&harness);
    let drag_start = resize_handle.center();
    let drag_target = drag_start + egui::vec2(120.0, 0.0);
    harness.event(egui::Event::PointerMoved(drag_start));
    harness.event(egui::Event::PointerButton {
        pos: drag_start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.run();
    harness.event(egui::Event::PointerMoved(drag_target));
    harness.run_steps(2);

    assert_eq!(
        resource_table_name_width(&mut harness),
        original_name_width + 120.0
    );
    assert!(
        (resource_table_name_resize_handle(&harness).center().x - drag_target.x).abs() < 0.1,
        "the resize handle should remain beneath the pointer while dragging"
    );
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/resource_table_column_configuration/resizing_column",
    ));

    harness.event(egui::Event::PointerButton {
        pos: drag_target,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(2);
    assert_eq!(
        resource_table_name_width(&mut harness),
        original_name_width + 120.0
    );
    assert!(
        (resource_table_name_resize_handle(&harness).center().x - drag_target.x).abs() < 0.1,
        "the resize handle should stay in place after releasing it"
    );
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/resource_table_column_configuration/resized_column",
    ));

    let table_header = resource_table_name_header_left(&harness);
    harness.event(egui::Event::PointerMoved(table_header));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(-1_000.0, 0.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/resource_table_column_configuration/resized_and_horizontally_scrolled",
    ));
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
fn cluster_rail_settings_item_shows_its_update_status() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness
        .state_mut()
        .updater
        .set_status_for_test(UpdateStatus::Checking);
    harness.run();

    harness.get_by_label("Settings").hover();
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/cluster_rail_settings_item_shows_its_update_status/settings_update_status",
    ));
}

#[test]
fn cluster_rail_settings_footer_stays_clear_of_overflowing_clusters() {
    let mut harness = application_harness::<MockWorker>();
    let clusters = (1..=24)
        .map(|cluster_key| {
            (
                cluster_key,
                fixture_cluster(cluster_key, &format!("cluster-{cluster_key}")),
            )
        })
        .collect();
    harness.state_mut().ui_state = UiState {
        clusters,
        next_cluster_key: 24,
        selected_cluster: Some(1),
        ..Default::default()
    };
    harness.run();

    harness.get_by_label("Settings");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "cluster_connection/cluster_rail_settings_footer_stays_clear_of_overflowing_clusters/settings_footer",
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
fn cron_job_run_confirmation_can_be_cancelled_without_sending_a_command() {
    let mut cluster = fixture_cluster(1, "dev");
    cluster.pending_cron_job_run = Some(PendingCronJobRun {
        resource_name: "nightly-report".into(),
        namespace: "default".into(),
    });
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        next_cluster_key: 1,
        selected_cluster: Some(1),
        ..Default::default()
    };

    harness.run();
    harness.get_by_label("Cancel").click();
    harness.run();

    assert!(harness.state().worker.commands.is_empty());
    assert!(
        harness.state().ui_state.clusters[&1]
            .pending_cron_job_run
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
