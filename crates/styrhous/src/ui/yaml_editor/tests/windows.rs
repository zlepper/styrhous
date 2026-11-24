use super::*;

#[test]
fn yaml_editor_clean_snapshot() {
    snapshot_editor(
        editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings"),
        "yaml_editor/yaml_editor_clean_snapshot/clean",
    );
}

#[test]
fn yaml_editor_search_interaction_snapshot() {
    // Local validation arms a real-time server-validation debounce on the first frame.
    // The harness advances simulated time, so under load the debounce can elapse mid-test,
    // run extra frames, and move the snapshot past the transient scroll fade after a match jump.
    let mut search_editor = editor("kind: ConfigMap\nmetadata:\n  name: map-config");
    search_editor.validation_revision = 1;
    let mut harness = editor_harness(search_editor);

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.step();
    harness.event(egui::Event::Text("map".into()));
    harness.step();
    harness.key_press(egui::Key::Enter);
    harness.step();

    assert_eq!(harness.state().editor.search.query, "map");
    assert_eq!(harness.state().editor.search.active_match, Some(0));
    assert_eq!(
        find_matches(&harness.state().editor.edited_yaml, "map", false)
            .expect("literal search is valid")
            .len(),
        2
    );
    harness.ui_harness(
        "yaml_editor/yaml_editor_search_interaction_snapshot/searches_and_highlights_matches",
    );
}

#[test]
fn stale_yaml_validation_results_do_not_overwrite_a_newer_revision() {
    let mut editor = editor("kind: ConfigMap");
    editor.validation_revision = 8;
    editor.server_validation = ValidationState::Idle;
    let editor_id = editor.id;
    let cluster_key = editor.cluster_key;
    let api_resource = editor.api_resource.clone();
    let namespace = editor.namespace.clone();
    let resource_name = editor.resource_name.clone();
    let mut ui = UiState::default();
    ui.yaml_editors.insert(editor_id, editor);
    let mut commands = Vec::new();

    ResourceYamlValidated {
        editor_id,
        revision: 7,
        cluster_key,
        api_resource: api_resource.clone(),
        namespace: namespace.clone(),
        resource_name: resource_name.clone(),
    }
    .apply(&mut ui, &mut commands);
    ResourceYamlValidationFailed {
        editor_id,
        revision: 7,
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        error: ResourceApiError {
            status_code: 422,
            message: "stale validation error".into(),
            causes: Vec::new(),
        },
    }
    .apply(&mut ui, &mut commands);
    ResourceYamlValidationCommandFailed {
        editor_id,
        revision: 7,
        error: "stale command error".into(),
    }
    .apply(&mut ui, &mut commands);

    let editor = ui.yaml_editors.get(&editor_id).expect("editor is retained");
    assert_eq!(editor.server_validation, ValidationState::Idle);
    assert!(editor.diagnostics.is_empty());
    assert!(editor.retained_diagnostics.is_empty());
}

#[test]
fn yaml_editor_modified_snapshot() {
    let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    snapshot_editor(editor, "yaml_editor/yaml_editor_modified_snapshot/modified");
}

#[test]
fn yaml_editor_saving_snapshot() {
    let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    editor.saving = true;
    snapshot_editor(editor, "yaml_editor/yaml_editor_saving_snapshot/saving");
}

#[test]
fn command_enter_does_not_apply_yaml_changes() {
    let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    let mut harness = editor_harness(editor);

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
    harness.run();

    assert!(!harness.state().editor.saving);
    assert!(harness.state().commands.iter().all(|command| {
        command
            .as_ref()
            .as_any()
            .downcast_ref::<ApplyResourceYaml>()
            .is_none()
    }));
}

#[test]
fn apply_editor_sends_the_fetched_resource_version() {
    let mut editor = editor("kind: ConfigMap");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    editor.resource_version = "42".into();
    let mut commands = Vec::new();

    apply_editor(&mut editor, &mut commands);

    let apply = commands
        .first()
        .and_then(|command| {
            command
                .as_ref()
                .as_any()
                .downcast_ref::<ApplyResourceYaml>()
        })
        .expect("applying YAML should send an apply command");
    assert_eq!(apply.resource_version, "42");
    assert_eq!(apply.resource_uid, "uid-1");
    assert_eq!(apply.original_yaml, "kind: ConfigMap");
}

#[test]
fn server_validation_sends_the_fetched_identity_and_original_document() {
    let mut editor = editor("kind: ConfigMap");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    editor.resource_version = "42".into();
    editor.resource_uid = "uid-42".into();
    editor.validation_due = Some(ValidationDue::At(1.0));
    let mut commands = Vec::new();

    maybe_request_server_validation(&mut editor, 1.0, &mut commands);

    let validation = commands
        .first()
        .and_then(|command| {
            command
                .as_ref()
                .as_any()
                .downcast_ref::<ValidateResourceYaml>()
        })
        .expect("validation should send a validation command");
    assert_eq!(validation.original_yaml, "kind: ConfigMap");
    assert_eq!(validation.resource_version, "42");
    assert_eq!(validation.resource_uid, "uid-42");
}

#[test]
fn completed_replacement_refreshes_the_saved_document_and_version() {
    let mut editor = editor("kind: ConfigMap\nmetadata:\n  name: settings");
    editor.edited_yaml.push_str("\ndata:\n  mode: stale");
    editor.saving = true;
    let editor_id = editor.id;
    let cluster_key = editor.cluster_key;
    let api_resource = editor.api_resource.clone();
    let namespace = editor.namespace.clone();
    let resource_name = editor.resource_name.clone();
    let persisted_yaml = "kind: ConfigMap\nmetadata:\n  name: settings\ndata:\n  mode: persisted";
    let mut ui = UiState::default();
    ui.yaml_editors.insert(editor_id, editor);
    let mut commands = Vec::new();

    ResourceApplyCompleted {
        editor_id,
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        yaml: persisted_yaml.into(),
        resource_version: "43".into(),
        resource_uid: "uid-1".into(),
    }
    .apply(&mut ui, &mut commands);

    let editor = ui.yaml_editors.get(&editor_id).expect("editor is retained");
    assert_eq!(editor.original_yaml.as_deref(), Some(persisted_yaml));
    assert_eq!(editor.edited_yaml, persisted_yaml);
    assert_eq!(editor.resource_version, "43");
    assert_eq!(editor.resource_uid, "uid-1");
    assert!(!editor.saving);
}

#[test]
fn conflict_failure_keeps_the_editor_document_and_identity_for_recovery() {
    let original_yaml = "kind: ConfigMap\nmetadata:\n  name: settings";
    let edited_yaml = format!("{original_yaml}\ndata:\n  mode: development");
    let mut editor = editor(original_yaml);
    editor.edited_yaml = edited_yaml.clone();
    editor.resource_version = "42".into();
    editor.resource_uid = "uid-42".into();
    editor.saving = true;
    let editor_id = editor.id;
    let cluster_key = editor.cluster_key;
    let api_resource = editor.api_resource.clone();
    let namespace = editor.namespace.clone();
    let resource_name = editor.resource_name.clone();
    let mut ui = UiState::default();
    ui.yaml_editors.insert(editor_id, editor);
    let mut commands = Vec::new();

    ResourceApplyFailed {
        current_resource_version: Some("43".into()),
        editor_id,
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        error: ResourceApiError {
            status_code: 409,
            message: "the object has been modified".into(),
            causes: Vec::new(),
        },
    }
    .apply(&mut ui, &mut commands);

    let editor = ui.yaml_editors.get(&editor_id).expect("editor is retained");
    assert!(!editor.saving);
    assert_eq!(editor.original_yaml.as_deref(), Some(original_yaml));
    assert_eq!(editor.edited_yaml, edited_yaml);
    assert_eq!(editor.resource_version, "42");
    assert_eq!(editor.resource_uid, "uid-42");
    assert_eq!(
        editor.error, None,
        "a conflict has its banner, not an error strip"
    );
    assert_eq!(
        editor.conflict,
        Some(EditorConflict {
            current_resource_version: Some("43".into())
        })
    );
}

#[test]
fn yaml_editor_apply_error_snapshot() {
    let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    editor.error = Some("The Kubernetes API rejected this resource".into());
    snapshot_editor(
        editor,
        "yaml_editor/yaml_editor_apply_error_snapshot/apply_error",
    );
}

#[test]
fn yaml_editor_apply_conflict_snapshot() {
    let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    editor.conflict = Some(EditorConflict {
        current_resource_version: None,
    });
    snapshot_editor(
        editor,
        "yaml_editor/yaml_editor_apply_conflict_snapshot/apply_conflict",
    );
}

#[test]
fn yaml_editor_discard_confirmation_snapshot() {
    let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    editor.confirm_discard = true;
    snapshot_editor(
        editor,
        "yaml_editor/yaml_editor_discard_confirmation_snapshot/discard_confirmation",
    );
}

#[test]
fn yaml_apply_is_disabled_with_a_reason_until_the_yaml_changes() {
    use egui_kittest::kittest::NodeT as _;
    let mut harness = editor_harness(editor("kind: ConfigMap"));
    assert!(
        harness
            .get_by_label("Apply changes")
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label("Apply changes").hover();
    harness.run();
    assert!(harness.query_by_label("No changes to apply").is_some());

    harness.state_mut().editor.edited_yaml = "kind: Secret".into();
    harness.run();
    assert!(
        !harness
            .get_by_label("Apply changes")
            .accesskit_node()
            .is_disabled(),
        "an edit enables Apply"
    );

    harness.state_mut().editor.saving = true;
    harness.run();
    harness.get_by_label("Apply changes").hover();
    harness.run();
    assert!(harness.query_by_label("Applying the changes…").is_some());
}

/// An editor whose apply was rejected because the cluster's copy changed: the banner offers the
/// two ways out.
fn conflicted_editor(current_resource_version: Option<&str>) -> YamlEditorWindowState {
    let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    editor.resource_version = "1834".into();
    editor.validation_revision = 1;
    editor.conflict = Some(EditorConflict {
        current_resource_version: current_resource_version.map(Into::into),
    });
    editor
}

#[test]
fn the_conflict_banner_names_both_resource_versions_and_blocks_apply() {
    use egui_kittest::kittest::NodeT as _;
    let mut harness = editor_harness(conflicted_editor(Some("1902")));

    harness.get_by_label(
        "This ConfigMap changed on the cluster after you opened it (resource version 1834 → 1902). Your edits can’t be applied.",
    );
    harness.get_by_label("Conflict");
    assert!(
        harness
            .get_by_label("Apply changes")
            .accesskit_node()
            .is_disabled(),
        "the edits cannot be applied over the newer version"
    );
    harness.get_by_label("Apply changes").hover();
    harness.run();
    assert!(
        harness
            .query_by_label("The resource changed on the cluster")
            .is_some()
    );

    // The worker could not say which version it found: only the opened one is named.
    let harness = editor_harness(conflicted_editor(None));
    harness.get_by_label(
        "This ConfigMap changed on the cluster after you opened it (resource version 1834). Your edits can’t be applied.",
    );
}

#[test]
fn copy_my_edits_copies_the_buffer_without_leaving_the_conflict() {
    let mut harness = editor_harness(conflicted_editor(Some("1902")));
    let edited = harness.state().editor.edited_yaml.clone();
    let position = harness.get_by_label("Copy my edits").rect().center();

    let copied = components::test_support::click_and_collect_copies(&mut harness, position);

    assert_eq!(copied, vec![edited.clone()]);
    assert_eq!(harness.state().editor.edited_yaml, edited);
    assert!(harness.state().editor.conflict.is_some());
}

#[test]
fn discard_and_reload_asks_first_then_fetches_the_cluster_version() {
    use components::test_support::primary_click;
    let mut harness = editor_harness(conflicted_editor(Some("1902")));
    let click = |harness: &mut Harness<'_, SnapshotState>, label: &str| {
        let position = harness.get_by_label(label).rect().center();
        primary_click(harness, position);
        harness.run();
    };

    click(&mut harness, "Discard and reload");
    harness.get_by_label("Discard your edits?");
    assert!(harness.state().commands.is_empty(), "nothing is lost yet");

    click(&mut harness, "Keep editing");
    assert!(harness.query_by_label("Discard your edits?").is_none());
    assert!(harness.state().editor.conflict.is_some());
    assert!(harness.state().editor.is_modified());

    click(&mut harness, "Discard and reload");
    // The dialog's own button repeats the banner's label.
    let confirm = harness
        .get_all_by_label("Discard and reload")
        .last()
        .expect("the dialog has a confirm button")
        .rect()
        .center();
    primary_click(&mut harness, confirm);
    harness.run();

    let editor = &harness.state().editor;
    assert!(editor.loading && editor.conflict.is_some() && !editor.confirm_reload);
    let fetch = harness
        .state()
        .commands
        .iter()
        .find_map(|command| command.as_ref().as_any().downcast_ref::<GetResourceYaml>())
        .expect("the cluster's version is fetched");
    assert_eq!(
        (fetch.editor_id, fetch.resource_name.as_str()),
        (1, "settings")
    );

    // The fetched document replaces the edits and is validated like a freshly opened one.
    let mut ui = UiState::default();
    ui.yaml_editors.insert(1, harness.state().editor.clone());
    ResourceYamlFetched {
        editor_id: 1,
        cluster_key: 7,
        api_resource: ui.yaml_editors[&1].api_resource.clone(),
        namespace: Some("kube-system".into()),
        resource_name: "settings".into(),
        yaml: "kind: ConfigMap\ndata:\n  mode: cluster".into(),
        resource_version: "1902".into(),
        resource_uid: "uid-1".into(),
    }
    .apply(&mut ui, &mut Vec::new());
    let fetched = &ui.yaml_editors[&1];
    assert_eq!(
        fetched.edited_yaml,
        "kind: ConfigMap\ndata:\n  mode: cluster"
    );
    assert_eq!(fetched.resource_version, "1902");
    assert!(!fetched.is_modified() && !fetched.loading);
    assert_eq!(
        fetched.conflict, None,
        "the fetched version ends the conflict"
    );
}

#[test]
fn server_validation_waits_for_egui_frame_time_not_the_wall_clock() {
    let mut editor = editor("kind: ConfigMap");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    editor.validation_revision = 1;
    editor.validation_due = Some(ValidationDue::At(1_000.0));
    let mut harness = editor_harness(editor);
    let validations = |harness: &Harness<'_, SnapshotState>| {
        harness
            .state()
            .commands
            .iter()
            .filter(|command| {
                command
                    .as_ref()
                    .as_any()
                    .downcast_ref::<ValidateResourceYaml>()
                    .is_some()
            })
            .count()
    };
    let frame_time = |harness: &Harness<'_, SnapshotState>| harness.ctx.input(|input| input.time);

    // A deadline far ahead is not reached by frames, whatever the wall clock says.
    harness.step();
    assert_eq!(validations(&harness), 0);

    // Frames advance the harness's clock by a fixed step; it is due two steps (500 ms) on.
    let due = frame_time(&harness) + VALIDATION_DEBOUNCE.as_secs_f64();
    harness.state_mut().editor.validation_due = Some(ValidationDue::At(due));
    harness.step();
    assert_eq!(validations(&harness), 0, "{} < {due}", frame_time(&harness));
    harness.step();
    assert_eq!(
        validations(&harness),
        1,
        "{} >= {due}",
        frame_time(&harness)
    );
    assert_eq!(harness.state().editor.validation_due, None);
    assert_eq!(
        harness.state().editor.server_validation,
        ValidationState::Pending
    );

    // An edit arms the debounce from the time of the next frame.
    let before = frame_time(&harness);
    harness.state_mut().editor.validation_due = Some(ValidationDue::Debounce);
    harness.step();
    let Some(ValidationDue::At(at)) = harness.state().editor.validation_due else {
        panic!("the debounce is armed");
    };
    let armed_at = at - VALIDATION_DEBOUNCE.as_secs_f64();
    assert!(before < armed_at && armed_at <= frame_time(&harness));
}

/// The state of the editor that `apply` is applied to, via the worker result as the UI receives it.
fn apply_failure(editor: YamlEditorWindowState, status_code: u16) -> YamlEditorWindowState {
    let (editor_id, cluster_key) = (editor.id, editor.cluster_key);
    let api_resource = editor.api_resource.clone();
    let mut ui = UiState::default();
    ui.yaml_editors.insert(editor_id, editor);
    ResourceApplyFailed {
        editor_id,
        cluster_key,
        api_resource,
        namespace: Some("kube-system".into()),
        resource_name: "settings".into(),
        error: ResourceApiError {
            status_code,
            message: "the API server said no".into(),
            causes: Vec::new(),
        },
        current_resource_version: None,
    }
    .apply(&mut ui, &mut Vec::new());
    ui.yaml_editors.remove(&editor_id).expect("editor is kept")
}

#[test]
fn an_apply_failure_other_than_a_conflict_keeps_its_error_and_sets_no_conflict() {
    let mut editor = editor("kind: ConfigMap");
    editor.saving = true;
    let editor = apply_failure(editor, 422);
    assert_eq!(editor.conflict, None);
    assert!(editor.error.is_some());
    assert!(!editor.saving);
}

#[test]
fn a_rejected_409_without_a_version_still_shows_the_conflict() {
    let editor = apply_failure(editor("kind: ConfigMap"), 409);
    assert_eq!(
        editor.conflict,
        Some(EditorConflict {
            current_resource_version: None
        })
    );
    assert_eq!(editor.error, None);
}

#[test]
fn a_completed_apply_clears_the_conflict() {
    let mut editor = conflicted_editor(Some("1902"));
    editor.saving = true;
    let (id, key, api_resource) = (editor.id, editor.cluster_key, editor.api_resource.clone());
    let mut ui = UiState::default();
    ui.yaml_editors.insert(id, editor);
    ResourceApplyCompleted {
        editor_id: id,
        cluster_key: key,
        api_resource,
        namespace: Some("kube-system".into()),
        resource_name: "settings".into(),
        yaml: "kind: ConfigMap".into(),
        resource_version: "1903".into(),
        resource_uid: "uid-1".into(),
    }
    .apply(&mut ui, &mut Vec::new());
    assert_eq!(ui.yaml_editors[&id].conflict, None);
}

#[test]
fn a_conflict_found_by_the_dry_run_blocks_apply_like_a_rejected_apply() {
    let mut editor = editor("kind: ConfigMap");
    editor.edited_yaml.push_str("\ndata:\n  mode: development");
    editor.validation_revision = 3;
    editor.server_validation = ValidationState::Pending;
    let (id, key, api_resource) = (editor.id, editor.cluster_key, editor.api_resource.clone());
    let mut ui = UiState::default();
    ui.yaml_editors.insert(id, editor);
    ResourceYamlValidationFailed {
        editor_id: id,
        revision: 3,
        cluster_key: key,
        api_resource,
        namespace: Some("kube-system".into()),
        resource_name: "settings".into(),
        error: ResourceApiError {
            status_code: 409,
            message: "The resource changed on the cluster".into(),
            causes: Vec::new(),
        },
    }
    .apply(&mut ui, &mut Vec::new());
    let editor = ui.yaml_editors.remove(&id).expect("editor is kept");
    assert!(editor.conflict.is_some());
    assert_eq!(editor.server_validation, ValidationState::Idle);

    let harness = editor_harness(editor);
    harness.get_by_label("Conflict");
    use egui_kittest::kittest::NodeT as _;
    assert!(
        harness
            .get_by_label("Apply changes")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn a_validation_result_from_before_a_reload_does_not_validate_the_fetched_document() {
    let mut editor = conflicted_editor(Some("1902"));
    editor.server_validation = ValidationState::Pending;
    let revision = editor.validation_revision;
    let (id, key, api_resource) = (editor.id, editor.cluster_key, editor.api_resource.clone());
    reload_editor(&mut editor, &mut Vec::new());
    assert!(editor.validation_revision > revision);
    let mut ui = UiState::default();
    ui.yaml_editors.insert(id, editor);
    ResourceYamlValidated {
        editor_id: id,
        revision,
        cluster_key: key,
        api_resource,
        namespace: Some("kube-system".into()),
        resource_name: "settings".into(),
    }
    .apply(&mut ui, &mut Vec::new());
    assert_eq!(
        ui.yaml_editors[&id].server_validation,
        ValidationState::Idle
    );

    // The fetched document is validated from scratch.
    let editor = ui.yaml_editors.remove(&id).expect("editor is kept");
    assert!(editor.revalidate);
}

/// A document of `lines` short lines.
fn long_document(lines: usize) -> String {
    (1..=lines).map(|n| format!("key-{n}: value\n")).collect()
}

#[test]
fn yaml_editor_gutter_snapshots_with_three_and_four_digit_line_numbers() {
    for (lines, name) in [(120, "three_digits"), (1200, "four_digits")] {
        let mut editor = editor(&long_document(lines));
        editor.validation_revision = 1;
        snapshot_editor(
            editor,
            &format!("yaml_editor/yaml_editor_wide_gutter_snapshot/{name}"),
        );
    }
}

#[test]
fn escape_leaves_the_find_field_but_keeps_its_text_and_the_bar() {
    let mut harness = editor_harness(editor("kind: ConfigMap\nmetadata:\n  name: map-config"));
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.step();
    harness.event(egui::Event::Text("map".into()));
    harness.step();
    assert!(harness.get_by_label("Search YAML").is_focused());

    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!harness.get_by_label("Search YAML").is_focused());
    assert_eq!(harness.state().editor.search.query, "map");
}
