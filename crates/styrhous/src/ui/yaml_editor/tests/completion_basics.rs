use super::*;

#[test]
fn yaml_editor_completion_snapshot() {
    let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmet");
    editor.suggestions = vec![
        CompletionSuggestion {
            label: "metadata".into(),
            type_label: Some("object".into()),
            detail: Some("Object metadata including the resource name and labels.".into()),
        },
        CompletionSuggestion {
            label: "immutable".into(),
            type_label: Some("boolean".into()),
            detail: Some("Whether the ConfigMap can change after it has been created.".into()),
        },
    ];
    editor.completion_context = Some(CompletionContext {
        kind: CompletionContextKind::MappingKey,
        type_label: Some("object".into()),
        description: Some("A Kubernetes resource mapping.".into()),
    });
    editor.suggestions_visible = true;
    editor.validation_revision = 1;
    snapshot_editor(
        editor,
        "yaml_editor/yaml_editor_completion_snapshot/completion",
    );
}

#[test]
fn yaml_editor_value_completion_snapshot() {
    let mut editor = editor("mode: Read");
    let schema = ResourceSchema::new(json!({
        "type": "object",
        "properties": {
            "mode": {
                "type": "string",
                "enum": ["ReadOnly", "ReadWrite"]
            }
        }
    }));
    let completion = schema.completion_at(&editor.edited_yaml, editor.edited_yaml.len());
    editor.suggestions = completion.suggestions;
    editor.completion_context = completion.context;
    editor.schema = Some(schema);
    editor.suggestions_visible = true;
    editor.validation_revision = 1;

    assert_eq!(
        editor
            .suggestions
            .iter()
            .map(|suggestion| suggestion.label.as_str())
            .collect::<Vec<_>>(),
        vec!["ReadOnly", "ReadWrite"],
    );
    snapshot_editor(
        editor,
        "yaml_editor/yaml_editor_value_completion_snapshot/value_completion",
    );
}

#[test]
fn yaml_editor_deep_array_value_completion_snapshot() {
    let yaml =
        "spec:\n  templates:\n    - spec:\n        containers:\n          - imagePullPolicy: Al";
    let mut editor = editor(yaml);
    let schema = ResourceSchema::new(json!({
        "type": "object",
        "properties": {
            "spec": {"type": "object", "properties": {
                "templates": {"type": "array", "items": {"type": "object", "properties": {
                    "spec": {"type": "object", "properties": {
                        "containers": {"type": "array", "items": {"type": "object", "properties": {
                            "imagePullPolicy": {
                                "type": "string",
                                "enum": ["Always", "IfNotPresent", "Never"]
                            }
                        }}}
                    }}
                }}}
            }}
        }
    }));
    let completion = schema.completion_at(&editor.edited_yaml, editor.edited_yaml.len());
    editor.suggestions = completion.suggestions;
    editor.completion_context = completion.context;
    editor.schema = Some(schema);
    editor.suggestions_visible = true;
    editor.validation_revision = 1;

    assert_eq!(editor.suggestions[0].label, "Always");
    snapshot_editor(
        editor,
        "yaml_editor/yaml_editor_deep_array_value_completion_snapshot/deep_array_value_completion",
    );
}

#[test]
fn yaml_editor_completion_keyboard_navigation_snapshot() {
    let mut editor = editor("alpha\nbeta");
    editor.suggestions = many_suggestions(128);
    editor.suggestions_visible = true;
    editor.validation_revision = 1;
    let mut harness = editor_harness(editor);

    for _ in 0..16 {
        harness.key_press(egui::Key::ArrowDown);
        harness.run();
    }
    for _ in 0..4 {
        harness.key_press(egui::Key::ArrowUp);
        harness.run();
    }

    assert_eq!(harness.state().editor.suggestion_selection, 12);
    assert_eq!(harness.state().editor.suggestions[12].label, "field-012",);
    harness.ui_harness("yaml_editor/yaml_editor_completion_keyboard_navigation_snapshot/completion_keyboard_navigation");
}

#[test]
fn yaml_editor_completion_bottom_right_caret_snapshot() {
    let yaml = format!("{}deep: {}", "filler: value\n".repeat(48), "x".repeat(160));
    snapshot_completion_at_focused_caret(
        &yaml,
        "yaml_editor/yaml_editor_completion_bottom_right_caret_snapshot/completion_bottom_right_caret",
    );
}

#[test]
fn yaml_editor_completion_top_left_caret_snapshot() {
    let mut editor = editor("mode: Read");
    editor.suggestions = vec![CompletionSuggestion {
        label: "ReadOnly".into(),
        type_label: Some("enum".into()),
        detail: Some("allowed value".into()),
    }];
    editor.suggestions_visible = true;
    editor.validation_revision = 1;
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, "mode: Read".chars().count());

    harness.run();
    harness.ui_harness("yaml_editor/yaml_editor_completion_top_left_caret_snapshot/focused_caret");
}

#[test]
fn ctrl_space_opens_completion_without_changing_the_yaml() {
    let mut editor = editor("met");
    editor.schema = Some(ResourceSchema::new(json!({
        "type": "object",
        "properties": {
            "metadata": {"type": "object"}
        }
    })));
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, 3);

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Space);
    harness.run();

    assert_eq!(harness.state().editor.edited_yaml, "met");
    assert!(harness.state().editor.suggestions_visible);
    assert_eq!(harness.state().editor.suggestions[0].label, "metadata");
}

#[test]
fn moving_the_caret_recalculates_the_visible_completion() {
    let mut editor = editor("met\nmode: Re");
    editor.schema = Some(ResourceSchema::new(json!({
        "type": "object",
        "properties": {
            "metadata": {"type": "object"},
            "mode": {"type": "string", "enum": ["ReadOnly", "ReadWrite"]}
        }
    })));
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, 3);

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Space);
    harness.run();
    assert_eq!(harness.state().editor.suggestions[0].label, "metadata");

    set_editor_caret(&ctx, text_edit_id, "met\nmode: Re".chars().count());
    harness.run();

    assert!(harness.state().editor.suggestions_visible);
    assert_eq!(harness.state().editor.suggestions[0].label, "ReadOnly");
}

#[test]
fn clicking_outside_the_completion_popup_dismisses_it() {
    let mut editor = editor("met");
    editor.schema = Some(ResourceSchema::new(json!({
        "type": "object",
        "properties": {"metadata": {"type": "object"}}
    })));
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, 3);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Space);
    harness.run();

    let outside = egui::pos2(1200.0, 700.0);
    harness.event(egui::Event::PointerMoved(outside));
    harness.event(egui::Event::PointerButton {
        pos: outside,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();

    assert!(!harness.state().editor.suggestions_visible);
    harness.event(egui::Event::PointerButton {
        pos: outside,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
}

#[test]
fn ctrl_space_shows_an_empty_completion_message_snapshot() {
    let mut editor = editor("unknown");
    editor.schema = Some(ResourceSchema::new(json!({
        "type": "object",
        "properties": {
            "metadata": {"type": "object"}
        }
    })));
    editor.validation_revision = 1;
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, "unknown".chars().count());

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Space);
    harness.run();

    assert!(harness.state().editor.suggestions_visible);
    assert!(harness.state().editor.suggestions.is_empty());
    harness.get_by_label("No completions available");
    harness.ui_harness(
        "yaml_editor/ctrl_space_shows_an_empty_completion_message_snapshot/no_completions",
    );
}

#[test]
fn arrow_keys_move_the_caret_when_the_completion_popup_is_empty() {
    let mut editor = editor("unknown\nnext");
    editor.schema = Some(ResourceSchema::new(json!({
        "type": "object",
        "properties": {"metadata": {"type": "object"}}
    })));
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, "unknown".chars().count());
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Space);
    harness.run();

    assert!(harness.state().editor.suggestions.is_empty());
    harness.key_press(egui::Key::ArrowDown);
    harness.run();

    assert_eq!(
        editor_caret(&ctx, text_edit_id),
        "unknown\nnext".chars().count()
    );
}
