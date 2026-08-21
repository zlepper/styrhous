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
    let padding = spacing::MD;
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
    let preferred_y = caret_rect.bottom() + spacing::XS;
    let y = if preferred_y <= max_y {
        preferred_y.max(min_y)
    } else {
        (caret_rect.top() - popup_height - spacing::XS).clamp(min_y, max_y)
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
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(kind)
                .font(typography::metadata())
                .strong()
                .color(indigo::_700),
        );
        if let Some(type_label) = &context.type_label {
            ui.label(
                egui::RichText::new(type_label)
                    .font(typography::metadata())
                    .color(gray::_600),
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
    let fill = if selected {
        indigo::_100
    } else {
        TOOLBAR_BACKGROUND
    };
    let stroke = if selected {
        indigo::_400
    } else {
        egui::Color32::TRANSPARENT
    };
    let response = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(components::design::radius::control())
        .inner_margin(egui::Margin::symmetric(
            spacing::SM as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.label(
                egui::RichText::new(&suggestion.label)
                    .font(typography::monospace())
                    .strong()
                    .color(gray::_900),
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
    let label_width = longest_label as f32 * typography::MONOSPACE_SIZE * 0.65;
    (label_width + 2.0 * spacing::SM).clamp(COMPLETION_LIST_MIN_WIDTH, COMPLETION_LIST_MAX_WIDTH)
}

pub(super) fn completion_popup_width(
    suggestions: &[crate::resource_schema::CompletionSuggestion],
) -> f32 {
    completion_list_width(suggestions) + 2.0 * spacing::MD
}

pub(super) fn completion_type_badge(ui: &mut egui::Ui, type_label: &str) -> egui::Response {
    let is_boolean = type_label == "boolean";
    egui::Frame::new()
        .fill(if is_boolean {
            status::SUCCESS.gamma_multiply(0.15)
        } else {
            indigo::_200
        })
        .corner_radius(components::design::radius::control())
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(type_label.to_uppercase())
                    .font(typography::metadata())
                    .color(if is_boolean {
                        status::SUCCESS
                    } else {
                        indigo::_700
                    }),
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
        ui.set_width(COMPLETION_DOCUMENTATION_WIDTH);
        egui::Frame::new()
            .fill(TOOLBAR_BACKGROUND)
            .stroke(egui::Stroke::new(1.0, TABLE_BORDER))
            .corner_radius(components::design::radius::surface())
            .shadow(egui::epaint::Shadow {
                offset: [0, 4],
                blur: 10,
                spread: 0,
                color: egui::Color32::from_black_alpha(80),
            })
            .inner_margin(egui::Margin::same(spacing::MD as i8))
            .show(ui, |ui| {
                ui.set_width(COMPLETION_DOCUMENTATION_WIDTH - 2.0 * spacing::MD);
                ui.horizontal(|ui| {
                    completion_type_badge(ui, suggestion.type_label.as_deref().unwrap_or("field"));
                    ui.label(
                        egui::RichText::new(&suggestion.label)
                            .font(typography::monospace())
                            .strong()
                            .color(gray::_900),
                    );
                });
                ui.add_space(spacing::SM);
                ui.separator();
                ui.add_space(spacing::SM);
                ui.label(
                    egui::RichText::new(
                        suggestion
                            .detail
                            .as_deref()
                            .unwrap_or("No schema documentation is available."),
                    )
                    .font(typography::metadata())
                    .color(gray::_700),
                );
            });
    });
}

pub(super) fn completion_documentation_position(
    viewport: egui::Rect,
    completion_popup_rect: egui::Rect,
    selected_row_rect: egui::Rect,
) -> egui::Pos2 {
    let min_x = viewport.left() + spacing::MD;
    let max_x = (viewport.right() - COMPLETION_DOCUMENTATION_WIDTH - spacing::MD).max(min_x);
    let right_of_completion = completion_popup_rect.right() + spacing::SM;
    let x = if right_of_completion <= max_x {
        right_of_completion
    } else {
        (completion_popup_rect.left() - COMPLETION_DOCUMENTATION_WIDTH - spacing::SM)
            .clamp(min_x, max_x)
    };
    let min_y = viewport.top() + spacing::MD;
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
