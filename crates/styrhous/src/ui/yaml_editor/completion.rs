use super::*;

/// The documentation of the highlighted completion beside the completion
/// popover at `completion` on `surface`.
pub(super) fn show_completion_documentation(
    ctx: &egui::Context,
    surface: egui::Rect,
    completion: egui::Rect,
    suggestion: &crate::resource_schema::CompletionSuggestion,
) {
    let position = components::completion_doc_position(
        surface,
        completion,
        egui::vec2(size::COMPLETION_DOC_WIDTH_EDITOR, 0.0),
    );
    egui::Area::new(egui::Id::new((
        "yaml-editor-suggestion-documentation",
        suggestion.label.as_str(),
    )))
    .order(egui::Order::Foreground)
    .fixed_pos(position)
    .constrain_to(surface)
    .show(ctx, |ui| {
        CompletionDoc::new(
            suggestion.type_label.as_deref().unwrap_or("field"),
            &suggestion.label,
            suggestion
                .detail
                .as_deref()
                .unwrap_or("No schema documentation is available."),
        )
        .width(size::COMPLETION_DOC_WIDTH_EDITOR)
        .show(ui);
    });
}

pub(super) fn byte_index(text: &str, character_index: usize) -> usize {
    text.char_indices()
        .nth(character_index)
        .map_or(text.len(), |(index, _)| index)
}

pub(super) fn insert_suggestion(text: &mut String, cursor: usize, suggestion: &str) -> usize {
    let line_start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let before_cursor = &text[line_start..cursor];
    let start = before_cursor
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            (!character.is_alphanumeric() && character != '-' && character != '_')
                .then_some(line_start + index + character.len_utf8())
        })
        .unwrap_or(line_start);
    let is_value = before_cursor.contains(':');
    let replacement = if is_value {
        suggestion.to_owned()
    } else {
        format!("{suggestion}: ")
    };
    text.replace_range(start..cursor, &replacement);
    text[..start].chars().count() + replacement.chars().count()
}
