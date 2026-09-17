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
    let mut harness = editor_harness(editor("kind: ConfigMap\nmetadata:\n  name: map-config"));

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.step();
    harness.event(egui::Event::Text("map".into()));
    harness.step();
    harness.key_press(egui::Key::Enter);

    // Keep this interaction snapshot focused on find results rather than
    // allowing the asynchronous server-validation footer to race the final
    // rendered frame.
    let editor = &mut harness.state_mut().editor;
    editor.validation_revision = editor.validation_revision.saturating_add(1);
    editor.validation_due = None;
    editor.server_validation = ValidationState::Idle;
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
    editor.validation_due = Some(Instant::now());
    let mut commands = Vec::new();

    maybe_request_server_validation(&mut editor, &mut commands);

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
        editor.error.as_deref(),
        Some(
            "This resource changed on the cluster. Discard your edits and reopen the editor before applying changes."
        )
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
    editor.error = Some(
        "This resource changed on the cluster. Discard your edits and reopen the editor before applying changes."
            .into(),
    );
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
