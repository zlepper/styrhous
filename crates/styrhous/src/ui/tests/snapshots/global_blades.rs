//! Global-blade ownership and settings scenarios.

use super::*;

#[test]
fn settings_shows_that_application_updates_are_being_checked() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness
        .state_mut()
        .updater
        .set_status_for_test(UpdateStatus::Checking);
    harness.run();

    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(&mut harness, settings_position);
    harness.run();
    let application_settings_position = harness
        .get_by_label(OPEN_APPLICATION_SETTINGS)
        .rect()
        .center();
    primary_click(&mut harness, application_settings_position);
    harness.run();

    harness.get_by_label("Application updates");
    harness.get_by_label("Checking for updates…");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "updater/settings_shows_that_application_updates_are_being_checked/checking",
    ));
}

#[test]
fn settings_shows_a_staged_application_update() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness
        .state_mut()
        .updater
        .set_status_for_test(UpdateStatus::Staged {
            version: "0.2.0".into(),
        });
    harness.run();

    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(&mut harness, settings_position);
    harness.run();
    let application_settings_position = harness
        .get_by_label(OPEN_APPLICATION_SETTINGS)
        .rect()
        .center();
    primary_click(&mut harness, application_settings_position);
    harness.run();

    harness.get_by_label("Application updates");
    harness.get_by_label("Version 0.2.0 is ready and will be installed on the next launch.");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "updater/settings_shows_a_staged_application_update/staged_update",
    ));
}

#[test]
fn settings_replaces_an_inspector_owned_child_blade_without_resurrecting_history() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    open_inspector_with_column_settings(&mut state, &mut commands);
    assert_eq!(
        state.global_blades.navigator().unwrap().back_stack().len(),
        1
    );
    state.open_terminal_settings(&TerminalLaunchSettings::default(), &mut commands);
    assert!(state.clusters[&2].resource_detail_panel.is_none());
    assert!(state.terminal_settings_blade().is_some());
    assert!(commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
    harness.state_mut().ui_state = state;
    harness.state_mut().worker.commands.append(&mut commands);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run_steps(2);

    harness.get_by_label("Close blade").click();
    harness.run_steps(2);

    assert!(harness.state().ui_state.global_blades.navigator().is_none());
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none()
    );
}

#[test]
fn opening_an_inspector_in_another_cluster_replaces_all_panel_state() {
    let mut state = oracle_resource_table_state();
    let deployment = fixture_api_resource("apps", "Deployment", "deployments");
    let mut commands = Vec::new();
    state.open_resource_detail(
        1,
        deployment.clone(),
        "dev-api".into(),
        Some("default".into()),
        "dev-deployment-uid".into(),
        &mut commands,
    );
    state.open_resource_detail(
        2,
        deployment,
        "kind-api".into(),
        Some("kube-system".into()),
        "kind-deployment-uid".into(),
        &mut commands,
    );

    assert!(state.clusters[&1].resource_detail_panel.is_none());
    assert!(state.clusters[&2].resource_detail_panel.is_some());
    assert_eq!(
        state
            .global_blades
            .navigator()
            .unwrap()
            .current()
            .resource_detail()
            .unwrap()
            .cluster_key,
        2
    );
    assert!(commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 1 && command.history_entry_id == 1)
    }));
}

#[test]
fn closing_an_inspector_owned_column_settings_blade_cleans_up_the_inspector() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    open_inspector_with_column_settings(&mut state, &mut commands);
    harness.state_mut().ui_state = state;
    harness.state_mut().worker.commands.append(&mut commands);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run_steps(2);

    harness.get_by_label("Configure columns");
    harness.get_by_label("Close blade").click();
    harness.run_steps(2);

    assert!(harness.state().ui_state.global_blades.navigator().is_none());
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none()
    );
    assert!(harness.state().worker.commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
}

#[test]
fn escape_closes_an_inspector_owned_column_settings_blade_and_cleans_up() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    open_inspector_with_column_settings(&mut state, &mut commands);
    harness.state_mut().ui_state = state;
    harness.state_mut().worker.commands.append(&mut commands);
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run_steps(2);

    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);

    assert!(harness.state().ui_state.global_blades.navigator().is_none());
    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none()
    );
    assert!(harness.state().worker.commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
}

#[test]
fn deleting_an_inspector_removes_its_foreground_column_settings_child() {
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    open_inspector_with_column_settings(&mut state, &mut commands);

    ResourceDetailDeleted {
        cluster_key: 2,
        history_entry_id: 1,
    }
    .apply(&mut state, &mut commands);

    assert!(state.global_blades.navigator().is_none());
    assert!(state.clusters[&2].resource_detail_panel.is_none());
    assert!(commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
}

#[test]
fn refreshing_clusters_discards_open_inspectors_and_stops_their_watches() {
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    state.open_resource_detail(
        2,
        fixture_api_resource("apps", "Deployment", "deployments"),
        "api".into(),
        Some("kube-system".into()),
        "deployment-uid".into(),
        &mut commands,
    );

    KubernetesClustersUpdated(vec![Cluster {
        name: "refreshed".into(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);

    assert!(state.global_blades.navigator().is_none());
    assert!(commands.iter().any(|command| {
        command_is::<StopResourceDetailWatch>(command)
            .is_some_and(|command| command.cluster_key == 2 && command.history_entry_id == 1)
    }));
}
