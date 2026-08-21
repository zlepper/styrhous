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
    let top_left = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(2.0, 18.0));
    let top_right = egui::Rect::from_min_size(egui::pos2(1580.0, 20.0), egui::vec2(2.0, 18.0));
    let bottom_left = egui::Rect::from_min_size(egui::pos2(20.0, 870.0), egui::vec2(2.0, 18.0));
    let bottom_right = egui::Rect::from_min_size(egui::pos2(1580.0, 870.0), egui::vec2(2.0, 18.0));

    let top_left_position = completion_popup_position(
        viewport,
        top_left,
        COMPLETION_POPUP_MAX_HEIGHT,
        COMPLETION_POPUP_MAX_WIDTH,
    );
    let top_right_position = completion_popup_position(
        viewport,
        top_right,
        COMPLETION_POPUP_MAX_HEIGHT,
        COMPLETION_POPUP_MAX_WIDTH,
    );
    let bottom_left_position = completion_popup_position(
        viewport,
        bottom_left,
        COMPLETION_POPUP_MAX_HEIGHT,
        COMPLETION_POPUP_MAX_WIDTH,
    );
    let bottom_right_position = completion_popup_position(
        viewport,
        bottom_right,
        COMPLETION_POPUP_MAX_HEIGHT,
        COMPLETION_POPUP_MAX_WIDTH,
    );

    for position in [
        top_left_position,
        top_right_position,
        bottom_left_position,
        bottom_right_position,
    ] {
        assert!(position.x >= spacing::MD);
        assert!(position.x + COMPLETION_POPUP_MAX_WIDTH <= viewport.right() - spacing::MD);
        assert!(position.y >= spacing::MD);
        assert!(position.y + COMPLETION_POPUP_MAX_HEIGHT <= viewport.bottom() - spacing::MD);
    }
    assert!(top_left_position.y >= top_left.bottom());
    assert!(top_right_position.x < top_right.left());
    assert!(bottom_left_position.y < bottom_left.top());
    assert!(bottom_right_position.x < bottom_right.left());
    assert!(bottom_right_position.y < bottom_right.top());
    assert_eq!(
        completion_popup_height(3),
        COMPLETION_POPUP_CHROME_HEIGHT + 3.0 * COMPLETION_ROW_HEIGHT
    );
    assert_eq!(completion_popup_height(128), COMPLETION_POPUP_MAX_HEIGHT);
}

#[test]
fn completion_popup_width_fits_short_labels_without_penalizing_long_ones() {
    assert_eq!(
        completion_list_width(&many_suggestions(1)),
        COMPLETION_LIST_MIN_WIDTH
    );
    assert_eq!(
        completion_list_width(&[CompletionSuggestion {
            label: "a".repeat(80),
            type_label: None,
            detail: None,
        }]),
        COMPLETION_LIST_MAX_WIDTH
    );
}

#[test]
fn completion_documentation_attaches_to_the_selected_row_without_overlapping_the_list() {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1536.0, 1024.0));
    let selected_row =
        egui::Rect::from_min_size(egui::pos2(1238.0, 830.0), egui::vec2(240.0, 26.0));
    let completion_popup = egui::Rect::from_min_size(
        egui::pos2(1230.0, 814.0),
        egui::vec2(COMPLETION_POPUP_MAX_WIDTH, 144.0),
    );

    let position = completion_documentation_position(viewport, completion_popup, selected_row);

    assert_eq!(position.y, selected_row.top());
    assert!(position.x + COMPLETION_DOCUMENTATION_WIDTH + spacing::SM <= completion_popup.left());
}
