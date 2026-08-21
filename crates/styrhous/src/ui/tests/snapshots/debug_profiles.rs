//! Debug-image profile and launcher-setting scenarios.

use super::*;

#[test]
fn refreshing_clusters_preserves_a_non_resource_global_blade() {
    let mut state = oracle_resource_table_state();
    let mut commands = Vec::new();
    open_terminal_settings(&mut state, TerminalLaunchSettings::default());

    KubernetesClustersUpdated(vec![Cluster {
        name: "refreshed".into(),
        is_current: true,
    }])
    .apply(&mut state, &mut commands);

    assert!(state.terminal_settings_blade().is_some());
    assert!(
        !commands
            .iter()
            .any(|command| { command_is::<StopResourceDetailWatch>(command).is_some() })
    );
}

#[test]
fn settings_blade_shows_custom_terminal_launcher_details() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    open_terminal_settings(
        &mut state,
        TerminalLaunchSettings {
            custom_template: Some("alacritty -e {command}".into()),
            ..Default::default()
        },
    );
    harness.state_mut().ui_state = state;
    harness.run();

    harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "Command template");
    harness.get_by_label("Save changes");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/settings_blade_shows_custom_terminal_launcher_details/settings_terminal_launcher_custom",
    ));
}

#[test]
fn saving_debug_image_presets_applies_the_settings_draft() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    open_terminal_settings(
        &mut state,
        TerminalLaunchSettings {
            custom_template: None,
            debug_image_presets: vec![DebugImagePreset {
                name: "Operations".into(),
                image: "registry.example/debug-tools:v1".into(),
                profile: DebugProfile::Sysadmin,
            }],
        },
    );
    harness.state_mut().ui_state = state;
    harness.run();

    let save_position = harness.get_by_label("Save changes").rect().center();
    primary_click(&mut harness, save_position);
    harness.run_steps(2);

    assert_eq!(
        harness.state().terminal_launch_settings.debug_image_presets,
        vec![DebugImagePreset {
            name: "Operations".into(),
            image: "registry.example/debug-tools:v1".into(),
            profile: DebugProfile::Sysadmin,
        }]
    );
    assert!(harness.state().ui_state.global_blades.navigator().is_none());
}

#[test]
fn debug_image_preset_table_adds_and_removes_rows() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    open_terminal_settings(
        &mut state,
        TerminalLaunchSettings {
            custom_template: None,
            debug_image_presets: vec![DebugImagePreset {
                name: "Operations".into(),
                image: "registry.example/debug-tools:v1".into(),
                profile: DebugProfile::Sysadmin,
            }],
        },
    );
    harness.state_mut().ui_state = state;
    harness.run();

    let remove_position = harness.get_by_label("Remove Operations").rect().center();
    primary_click(&mut harness, remove_position);
    harness.run();
    assert!(
        harness
            .state()
            .ui_state
            .terminal_settings_blade()
            .unwrap()
            .draft
            .debug_image_presets
            .is_empty()
    );

    let add_position = harness.get_by_label("Add debug image").rect().center();
    primary_click(&mut harness, add_position);
    harness.run();
    assert_eq!(
        harness
            .state()
            .ui_state
            .terminal_settings_blade()
            .unwrap()
            .draft
            .debug_image_presets,
        vec![DebugImagePreset {
            name: String::new(),
            image: String::new(),
            profile: DebugProfile::General,
        }]
    );
}

#[test]
fn debug_image_preset_table_reorders_rows_by_dragging_the_handle() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    open_terminal_settings(&mut state, TerminalLaunchSettings::default());
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
            .terminal_settings_blade()
            .unwrap()
            .draft
            .debug_image_presets
            .iter()
            .map(|preset| preset.name.as_str())
            .collect::<Vec<_>>(),
        ["Ubuntu", "Busybox", "Netshoot"]
    );
}

#[test]
fn debug_image_preset_table_moves_the_dragged_row() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    open_terminal_settings(&mut state, TerminalLaunchSettings::default());
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
            .terminal_settings_blade()
            .unwrap()
            .draft
            .debug_image_presets
            .iter()
            .map(|preset| preset.name.as_str())
            .collect::<Vec<_>>(),
        ["Ubuntu", "Netshoot", "Busybox"]
    );
}

#[test]
fn debug_image_preset_table_edits_and_saves_a_profile() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    open_terminal_settings(
        &mut state,
        TerminalLaunchSettings {
            custom_template: None,
            debug_image_presets: vec![DebugImagePreset {
                name: String::new(),
                image: String::new(),
                profile: DebugProfile::General,
            }],
        },
    );
    harness.state_mut().ui_state = state;
    harness.run();

    type_text(&mut harness, "Debug image 1 name", "Operations");
    type_text(
        &mut harness,
        "Debug image 1 image",
        "registry.example/debug-tools:v1",
    );
    let profile_position = harness
        .get_by_role_and_label(
            egui::accesskit::Role::ComboBox,
            "Debug image 1 debug profile",
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
        harness.state().terminal_launch_settings.debug_image_presets,
        vec![DebugImagePreset {
            name: "Operations".into(),
            image: "registry.example/debug-tools:v1".into(),
            profile: DebugProfile::Sysadmin,
        }]
    );
}

#[test]
fn debug_image_preset_profile_menu_stays_within_the_settings_blade() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    open_terminal_settings(
        &mut state,
        TerminalLaunchSettings {
            custom_template: Some("alacritty -e {command}".into()),
            ..Default::default()
        },
    );
    harness.state_mut().ui_state = state;
    harness.run();

    let profile_position = harness
        .get_by_role_and_label(
            egui::accesskit::Role::ComboBox,
            "Debug image 1 debug profile",
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
