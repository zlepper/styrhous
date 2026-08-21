use super::*;

#[test]
fn yaml_editor_diagnostics_snapshot() {
    snapshot_editor(
        diagnostic_editor(),
        "yaml_editor/yaml_editor_diagnostics_snapshot/diagnostics",
    );
}

#[test]
fn yaml_editor_many_diagnostics_snapshot() {
    let mut editor = diagnostic_editor();
    editor.diagnostics = (1..=100)
        .map(|index| YamlDiagnostic {
            path: format!("/data/field-{index}"),
            message: format!("Validation error {index}"),
            line: Some(4),
            range: editor.diagnostics[0].range.clone(),
        })
        .collect();
    editor.retained_diagnostics = editor.diagnostics.clone();

    snapshot_editor(
        editor,
        "yaml_editor/yaml_editor_many_diagnostics_snapshot/diagnostics_many",
    );
}

#[test]
fn yaml_editor_diagnostic_tooltip_snapshot() {
    let mut harness = editor_harness(diagnostic_editor());
    harness
        .get_by_label("Validation error: \"settings\" is not an allowed value")
        .hover();
    harness.run();
    harness.ui_harness("yaml_editor/yaml_editor_diagnostic_tooltip_snapshot/diagnostics_tooltip");
}

#[test]
fn yaml_editor_retained_diagnostics_snapshot() {
    let mut editor = diagnostic_editor();
    editor.diagnostics.clear();
    editor.server_validation = ValidationState::Pending;
    snapshot_editor(
        editor,
        "yaml_editor/yaml_editor_retained_diagnostics_snapshot/diagnostics_validating",
    );
}

#[test]
fn diagnostics_remain_visible_while_the_updated_yaml_is_validating() {
    let mut editor = diagnostic_editor();
    editor.diagnostics.clear();
    editor.validation_due = Some(Instant::now() + VALIDATION_DEBOUNCE);

    let (diagnostics, showing_retained_diagnostics) = diagnostics_to_display(&editor);
    assert!(showing_retained_diagnostics);
    assert_eq!(diagnostics, editor.retained_diagnostics);

    editor.validation_due = None;
    editor.server_validation = ValidationState::Valid;
    let (diagnostics, showing_retained_diagnostics) = diagnostics_to_display(&editor);
    assert!(!showing_retained_diagnostics);
    assert!(diagnostics.is_empty());
}

#[test]
fn clicking_a_diagnostic_focuses_and_selects_its_yaml_range() {
    let mut harness = editor_harness(diagnostic_editor());
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);

    harness
        .get_by_label("Line 4: \"settings\" is not an allowed value")
        .click_accesskit();
    harness.run_steps(2);

    assert!(ctx.memory(|memory| memory.has_focus(text_edit_id)));
    let selection = egui::widgets::text_edit::TextEditState::load(&ctx, text_edit_id)
        .and_then(|state| state.cursor.char_range())
        .expect("editor selection is available");
    let range = harness.state().editor.diagnostics[0]
        .range
        .as_ref()
        .expect("diagnostic has a source range");
    assert_eq!(
        selection.secondary.index,
        egui::text::CharIndex(range.start)
    );
    assert_eq!(selection.primary.index, egui::text::CharIndex(range.end));
    assert!(harness.state().editor.scroll_to_diagnostic.is_none());
}
