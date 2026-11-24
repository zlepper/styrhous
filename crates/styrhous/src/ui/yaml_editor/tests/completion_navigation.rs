use super::*;

#[test]
fn yaml_editor_completion_top_right_caret_snapshot() {
    let yaml = format!("mode: {}", "x".repeat(160));
    snapshot_completion_at_focused_caret(
        &yaml,
        "yaml_editor/yaml_editor_completion_top_right_caret_snapshot/completion_top_right_caret",
    );
}

#[test]
fn yaml_editor_completion_bottom_left_caret_snapshot() {
    let yaml = format!("{}mode: Read", "filler: value\n".repeat(48));
    snapshot_completion_at_focused_caret(
        &yaml,
        "yaml_editor/yaml_editor_completion_bottom_left_caret_snapshot/completion_bottom_left_caret",
    );
}

#[test]
fn completion_navigation_moves_the_popup_selection_without_moving_the_editor_caret() {
    let mut editor = editor("alpha\nbeta");
    editor.suggestions = many_suggestions(3);
    editor.suggestions_visible = true;
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, 2);

    harness.key_press(egui::Key::ArrowDown);
    harness.run();

    assert_eq!(harness.state().editor.suggestion_selection, 1);
    assert_eq!(editor_caret(&ctx, text_edit_id), 2);
}

#[test]
fn filtering_keeps_the_selected_suggestion_when_it_remains_available() {
    let mut editor = editor("m");
    let schema = ResourceSchema::new(json!({
        "type": "object",
        "properties": {
            "metadata": {"type": "object"},
            "xmetadata": {"type": "object"},
            "immutable": {"type": "boolean"}
        }
    }));
    editor.suggestions = schema
        .completion_at(&editor.edited_yaml, editor.edited_yaml.len())
        .suggestions;
    editor.suggestion_selection = editor
        .suggestions
        .iter()
        .position(|suggestion| suggestion.label == "xmetadata")
        .expect("xmetadata is initially suggested");
    editor.suggestions_visible = true;
    editor.schema = Some(schema);
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, 1);

    harness.event(egui::Event::Text("eta".into()));
    harness.run();

    let editor = &harness.state().editor;
    assert_eq!(editor.edited_yaml, "meta");
    assert_eq!(
        editor.suggestions[editor.suggestion_selection].label,
        "xmetadata"
    );
    assert!(
        editor
            .suggestions
            .iter()
            .all(|suggestion| suggestion.label != "immutable")
    );
}

#[test]
fn enter_applies_the_selected_fuzzy_matched_value_completion() {
    let mut editor = editor("mode: ");
    editor.schema = Some(ResourceSchema::new(json!({
        "type": "object",
        "properties": {
            "mode": {
                "type": "string",
                "enum": ["ReadOnly", "ReadWrite", "WriteOnly"]
            }
        }
    })));
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, "mode: ".chars().count());

    harness.event(egui::Event::Text("rw".into()));
    harness.run();

    assert_eq!(harness.state().editor.edited_yaml, "mode: rw");
    assert_eq!(harness.state().editor.suggestions.len(), 1);
    assert_eq!(harness.state().editor.suggestions[0].label, "ReadWrite");

    harness.key_press(egui::Key::Enter);
    harness.run();

    assert_eq!(harness.state().editor.edited_yaml, "mode: ReadWrite");
    assert!(!harness.state().editor.suggestions_visible);
    assert_eq!(
        editor_caret(&ctx, text_edit_id),
        "mode: ReadWrite".chars().count()
    );
}

#[test]
fn completion_popup_flips_and_clamps_at_each_viewport_edge() {
    let viewport = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1600.0, 900.0));
    let surface = viewport.shrink(space::S12);
    let popup = completion_popup_max_size();
    let caret = |x: f32, y: f32| egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(2.0, 18.0));
    let (top_left, top_right, bottom_left, bottom_right) = (
        caret(20.0, 20.0),
        caret(1580.0, 20.0),
        caret(20.0, 870.0),
        caret(1580.0, 870.0),
    );
    let position = |caret| components::completion_position(surface, caret, popup);
    for corner in [top_left, top_right, bottom_left, bottom_right] {
        let position = position(corner);
        assert!(position.x >= space::S12);
        assert!(position.x + popup.x <= viewport.right() - space::S12);
        assert!(position.y >= space::S12);
        assert!(position.y + popup.y <= viewport.bottom() - space::S12);
    }
    assert!(position(top_left).y >= top_left.bottom());
    assert!(position(top_right).x < top_right.left());
    assert!(position(bottom_left).y < bottom_left.top());
    assert!(position(bottom_right).x < bottom_right.left());
    assert!(position(bottom_right).y < bottom_right.top());
}

#[test]
fn completion_documentation_sits_beside_the_list_without_overlapping_it() {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1536.0, 1024.0));
    let completion_popup = egui::Rect::from_min_size(
        egui::pos2(1230.0, 814.0),
        egui::vec2(size::COMPLETION_WIDTH, 144.0),
    );

    let position = components::completion_doc_position(
        viewport.shrink(space::S12),
        completion_popup,
        egui::vec2(size::COMPLETION_DOC_WIDTH_EDITOR, 0.0),
    );

    assert_eq!(position.y, completion_popup.top());
    assert!(position.x + size::COMPLETION_DOC_WIDTH_EDITOR <= completion_popup.left());
}
