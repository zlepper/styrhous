use super::*;

pub(super) fn completion_popup_height(suggestion_count: usize) -> f32 {
    COMPLETION_POPUP_CHROME_HEIGHT
        + (suggestion_count as f32 * COMPLETION_ROW_HEIGHT).min(COMPLETION_LIST_MAX_HEIGHT)
}

pub(super) fn completion_popup_position(
    viewport: egui::Rect,
    caret_rect: egui::Rect,
    popup_height: f32,
    popup_width: f32,
) -> egui::Pos2 {
    let padding = space::S12;
    let min_x = viewport.left() + padding;
    let max_x = (viewport.right() - popup_width - padding).max(min_x);
    let preferred_x = caret_rect.left();
    let x = if preferred_x <= max_x {
        preferred_x.max(min_x)
    } else {
        (caret_rect.right() - popup_width - padding).max(min_x)
    };

    let min_y = viewport.top() + padding;
    let max_y = (viewport.bottom() - popup_height - padding).max(min_y);
    let preferred_y = caret_rect.bottom() + space::S4;
    let y = if preferred_y <= max_y {
        preferred_y.max(min_y)
    } else {
        (caret_rect.top() - popup_height - space::S4).clamp(min_y, max_y)
    };
    egui::pos2(x, y)
}

pub(super) fn completion_context_header(ui: &mut egui::Ui, context: Option<&CompletionContext>) {
    let Some(context) = context else {
        return;
    };
    let kind = match context.kind {
        CompletionContextKind::MappingKey => "EXPECTS KEY",
        CompletionContextKind::Value => "EXPECTS VALUE",
    };
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(kind)
                .font(type_scale::META.font_id(ui.ctx()))
                .strong()
                .color(theme.accent.text),
        );
        if let Some(type_label) = &context.type_label {
            ui.label(
                egui::RichText::new(type_label)
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(theme.text.secondary),
            );
        }
    });
}

pub(super) fn completion_row(
    ui: &mut egui::Ui,
    suggestion: &crate::resource_schema::CompletionSuggestion,
    selected: bool,
    width: f32,
) -> egui::Response {
    let theme = ui.tokens();
    let fill = if selected {
        theme.item.highlight
    } else {
        theme.surface.raised
    };
    let outline = if selected {
        theme.accent.outline
    } else {
        egui::Color32::TRANSPARENT
    };
    let response = egui::Frame::new()
        .fill(fill)
        .stroke(stroke::hairline(outline))
        .corner_radius(egui::CornerRadius::same(components::design::radius::KEY))
        .inner_margin(egui::Margin::symmetric(space::S8 as i8, space::S4 as i8))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.label(
                egui::RichText::new(&suggestion.label)
                    .font(type_scale::MONO.font_id(ui.ctx()))
                    .strong()
                    .color(theme.text.primary),
            );
        })
        .response;
    let response = response.interact(egui::Sense::click());
    ui.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_label(format!("Insert YAML completion: {}", suggestion.label));
    });
    response
}

pub(super) fn completion_list_width(
    suggestions: &[crate::resource_schema::CompletionSuggestion],
) -> f32 {
    let longest_label = suggestions
        .iter()
        .map(|suggestion| suggestion.label.chars().count())
        .max()
        .unwrap_or("No completions available".chars().count());
    let label_width = longest_label as f32 * type_scale::MONO.size() * 0.65;
    (label_width + 2.0 * space::S8).clamp(COMPLETION_LIST_MIN_WIDTH, COMPLETION_LIST_MAX_WIDTH)
}

pub(super) fn completion_popup_width(
    suggestions: &[crate::resource_schema::CompletionSuggestion],
) -> f32 {
    completion_list_width(suggestions) + 2.0 * space::S12
}

pub(super) fn completion_type_badge(ui: &mut egui::Ui, type_label: &str) -> egui::Response {
    let theme = ui.tokens();
    let tone = if type_label == "boolean" {
        Tone::Success
    } else {
        Tone::Accent
    };
    let ToneColors {
        tint: fill, text, ..
    } = theme.tone(tone);
    egui::Frame::new()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(components::design::radius::KEY))
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(type_label.to_uppercase())
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(text),
            );
        })
        .response
}

pub(super) fn show_completion_documentation(
    ctx: &egui::Context,
    completion_popup_rect: egui::Rect,
    selected_row_rect: egui::Rect,
    suggestion: &crate::resource_schema::CompletionSuggestion,
) {
    let position = completion_documentation_position(
        ctx.content_rect(),
        completion_popup_rect,
        selected_row_rect,
    );
    egui::Area::new(egui::Id::new((
        "yaml-editor-suggestion-documentation",
        suggestion.label.as_str(),
    )))
    .order(egui::Order::Foreground)
    .fixed_pos(position)
    .show(ctx, |ui| {
        theme::scope(ui, ThemeScope::Popover, |ui| {
            let theme = ui.tokens();
            ui.set_width(COMPLETION_DOCUMENTATION_WIDTH);
            components::popover_frame(&theme)
                .inner_margin(egui::Margin::same(space::S12 as i8))
                .show(ui, |ui| {
                    ui.set_width(COMPLETION_DOCUMENTATION_WIDTH - 2.0 * space::S12);
                    ui.horizontal(|ui| {
                        completion_type_badge(
                            ui,
                            suggestion.type_label.as_deref().unwrap_or("field"),
                        );
                        ui.label(
                            egui::RichText::new(&suggestion.label)
                                .font(type_scale::MONO.font_id(ui.ctx()))
                                .strong()
                                .color(theme.text.primary),
                        );
                    });
                    ui.add_space(space::S8);
                    ui.separator();
                    ui.add_space(space::S8);
                    ui.label(
                        egui::RichText::new(
                            suggestion
                                .detail
                                .as_deref()
                                .unwrap_or("No schema documentation is available."),
                        )
                        .font(type_scale::META.font_id(ui.ctx()))
                        .color(theme.text.secondary),
                    );
                });
        })
    });
}

pub(super) fn completion_documentation_position(
    viewport: egui::Rect,
    completion_popup_rect: egui::Rect,
    selected_row_rect: egui::Rect,
) -> egui::Pos2 {
    let min_x = viewport.left() + space::S12;
    let max_x = (viewport.right() - COMPLETION_DOCUMENTATION_WIDTH - space::S12).max(min_x);
    let right_of_completion = completion_popup_rect.right() + space::S8;
    let x = if right_of_completion <= max_x {
        right_of_completion
    } else {
        (completion_popup_rect.left() - COMPLETION_DOCUMENTATION_WIDTH - space::S8)
            .clamp(min_x, max_x)
    };
    let min_y = viewport.top() + space::S12;
    let max_y = (viewport.bottom() - 160.0).max(min_y);
    egui::pos2(x, selected_row_rect.top().clamp(min_y, max_y))
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
