use super::state::{UiState, ValidationState, YamlEditorWindowState};
use crate::resource_schema::{
    CompletionContext, CompletionContextKind, SourceRange, YamlDiagnostic,
};
use crate::worker::WorkerCommand;
use components::colors::{TABLE_BORDER, TOOLBAR_BACKGROUND, gray, indigo};
use components::design::{spacing, status, surface, typography};
use components::{
    ConfirmationDialog, ConfirmationDialogAction, ConfirmationDialogKind, PointingHand,
    TailwindButton,
};
use egui::text::{CCursor, CCursorRange};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};
use std::time::{Duration, Instant};

const TOOLBAR_HEIGHT: f32 = 52.0;
const VALIDATION_DEBOUNCE: Duration = Duration::from_millis(500);
const COMPLETION_LIST_MIN_WIDTH: f32 = 240.0;
const COMPLETION_LIST_MAX_WIDTH: f32 = 280.0;
const COMPLETION_DOCUMENTATION_WIDTH: f32 = 280.0;
const COMPLETION_ROW_HEIGHT: f32 = 26.0;
const COMPLETION_LIST_MAX_HEIGHT: f32 = 260.0;
const COMPLETION_POPUP_CHROME_HEIGHT: f32 = 68.0;
const DIAGNOSTIC_ROW_HEIGHT: f32 = 21.0;
const DIAGNOSTIC_LIST_MAX_HEIGHT: f32 = 6.0 * DIAGNOSTIC_ROW_HEIGHT;

pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    let ids = ui_state.yaml_editors.keys().copied().collect::<Vec<_>>();
    for id in ids {
        let Some(editor) = ui_state.yaml_editors.get_mut(&id) else {
            continue;
        };
        let viewport_id = egui::ViewportId::from_hash_of(("yaml-editor-window", id));
        if editor.focus_requested {
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
            editor.focus_requested = false;
        }
        let title = format!("Edit · {}", editor.resource_name);
        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(crate::DEFAULT_NATIVE_WINDOW_SIZE)
                .with_min_inner_size(crate::MIN_NATIVE_WINDOW_SIZE),
            |window_ui, _| show_editor_window(window_ui, editor, commands_to_send),
        );
    }
    ui_state
        .yaml_editors
        .retain(|_, editor| !editor.close_requested);
}

pub(super) fn show_editor_window(
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    let ctx = ui.ctx().clone();
    if ctx.input(|input| input.viewport().close_requested()) {
        request_close(&ctx, editor);
    }

    egui::Panel::top("yaml-editor-header")
        .exact_size(TOOLBAR_HEIGHT)
        .frame(toolbar_frame())
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Edit")
                        .font(typography::section_heading())
                        .color(gray::_900),
                );
                ui.add_space(spacing::LG);
                ui.label(
                    egui::RichText::new(&editor.resource_name)
                        .font(typography::section_heading())
                        .color(gray::_900),
                );
                ui.add_space(spacing::MD);
                ui.label(
                    egui::RichText::new(resource_scope(editor))
                        .font(typography::body())
                        .color(gray::_600),
                );
                if editor.saving {
                    ui.add_space(spacing::MD);
                    status_indicator(ui, gray::_400, "Applying…");
                } else if editor.is_modified() {
                    ui.add_space(spacing::MD);
                    status_indicator(ui, status::WARNING, "Modified");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let apply_clicked = ui
                        .add_enabled_ui(editor.is_modified() && !editor.saving, |ui| {
                            TailwindButton::primary("Apply changes")
                                .show(ui)
                                .with_pointing_hand()
                                .clicked()
                        })
                        .inner;
                    if apply_clicked {
                        apply_editor(editor, commands_to_send);
                    }
                    let close_label = if editor.is_modified() {
                        "Discard"
                    } else {
                        "Close"
                    };
                    if TailwindButton::secondary(close_label)
                        .show(ui)
                        .with_pointing_hand()
                        .clicked()
                    {
                        request_close(&ctx, editor);
                    }
                });
            });
        });

    egui::Panel::bottom("yaml-editor-footer")
        .exact_size(40.0)
        .frame(toolbar_frame())
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Changes apply directly to the cluster")
                        .font(typography::body())
                        .color(gray::_600),
                );
            });
        });

    if has_diagnostics_feedback(editor) {
        egui::Panel::bottom("yaml-editor-diagnostics")
            .frame(toolbar_frame())
            .show(ui, |ui| show_diagnostics(&ctx, ui, editor));
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(surface::TERMINAL_BACKGROUND)
                .inner_margin(egui::Margin::same(spacing::LG as i8)),
        )
        .show(ui, |ui| {
            if editor.loading {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading…");
                });
            } else if editor.original_yaml.is_none() {
                editor_error(
                    ui,
                    editor.error.as_deref().unwrap_or("Unable to load resource"),
                );
            } else {
                if editor.validation_revision == 0 && !editor.edited_yaml.is_empty() {
                    refresh_local_validation(editor);
                }
                if let Some(error) = &editor.error {
                    error_strip(ui, error);
                }
                if show_code_editor(&ctx, ui, editor) {
                    refresh_local_validation(editor);
                }
            }
        });

    maybe_request_server_validation(editor, commands_to_send);
    #[cfg(not(test))]
    if let Some(due) = editor.validation_due {
        ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
    }
    show_discard_confirmation(&ctx, editor);
}

fn toolbar_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(TOOLBAR_BACKGROUND)
        .stroke(egui::Stroke::new(1.0, TABLE_BORDER))
        .inner_margin(egui::Margin::symmetric(
            spacing::XL as i8,
            spacing::SM as i8,
        ))
}

fn resource_scope(editor: &YamlEditorWindowState) -> String {
    editor.namespace.as_deref().map_or_else(
        || format!("{} · Cluster-wide", editor.api_resource.kind),
        |namespace| format!("{} · {namespace}", editor.api_resource.kind),
    )
}

fn status_indicator(ui: &mut egui::Ui, color: egui::Color32, label: &str) {
    ui.label(
        egui::RichText::new("●")
            .font(typography::body())
            .color(color),
    );
    ui.label(
        egui::RichText::new(label)
            .font(typography::body())
            .color(gray::_600),
    );
}

fn error_strip(ui: &mut egui::Ui, error: &str) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(69, 10, 10))
        .stroke(egui::Stroke::new(1.0, status::DANGER))
        .inner_margin(egui::Margin::symmetric(
            spacing::LG as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(error)
                    .font(typography::body())
                    .color(egui::Color32::from_rgb(254, 202, 202)),
            );
        });
    ui.add_space(spacing::SM);
}

fn editor_error(ui: &mut egui::Ui, error: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(
            egui::RichText::new(error)
                .font(typography::body())
                .color(egui::Color32::from_rgb(254, 202, 202)),
        );
    });
}

fn show_code_editor(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
) -> bool {
    let text_edit_id = yaml_editor_text_edit_id(editor.id);
    // Consume this before `TextEdit` processes input. Otherwise the editor can handle the
    // keystroke first, leaving no reliable manual completion trigger.
    let requested = ctx.memory(|memory| memory.has_focus(text_edit_id))
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Space));
    let (move_selection_up, move_selection_down, accept_suggestion, dismiss_suggestions) =
        if editor.suggestions_visible && !editor.suggestions.is_empty() {
            ctx.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                        || input.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            })
        } else if editor.suggestions_visible {
            ctx.input_mut(|input| {
                (
                    false,
                    false,
                    false,
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                )
            })
        } else {
            (false, false, false, false)
        };
    let selection_changed = move_selection_up || move_selection_down;
    if move_selection_down {
        editor.suggestion_selection =
            (editor.suggestion_selection + 1).min(editor.suggestions.len().saturating_sub(1));
    }
    if move_selection_up {
        editor.suggestion_selection = editor.suggestion_selection.saturating_sub(1);
    }
    if dismiss_suggestions {
        editor.suggestions_visible = false;
        editor.completion_cursor = None;
    }

    let line_count = editor.edited_yaml.lines().count().max(1);
    let mut changed = false;
    let mut cursor_byte = None;
    let mut text_edit_id = None;
    let mut caret_rect = None;
    let scroll_target = editor.scroll_to_diagnostic.take();
    let fallback_completion_position = ui.clip_rect().left_top() + egui::vec2(74.0, 68.0);
    components::scroll::both()
        .id_salt(("yaml-editor-scroll", editor.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(36.0);
                    for line in 1..=line_count {
                        let has_diagnostic = editor
                            .diagnostics
                            .iter()
                            .any(|diagnostic| diagnostic.line == Some(line));
                        ui.label(
                            egui::RichText::new(line.to_string())
                                .font(typography::monospace())
                                .color(if has_diagnostic {
                                    status::DANGER
                                } else {
                                    gray::_500
                                }),
                        );
                    }
                });
                ui.separator();
                let theme = CodeTheme::dark(typography::MONOSPACE_SIZE);
                let mut layouter =
                    |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
                        let mut layout_job =
                            highlight(ui.ctx(), ui.style(), &theme, buffer.as_str(), "yaml");
                        layout_job.wrap.max_width = wrap_width;
                        ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
                    };
                ui.style_mut().visuals.text_cursor.stroke = egui::Stroke::new(4.0, indigo::_100);
                ui.style_mut().visuals.text_cursor.blink = false;
                let output = egui::TextEdit::multiline(&mut editor.edited_yaml)
                    .id(yaml_editor_text_edit_id(editor.id))
                    .font(typography::monospace())
                    .code_editor()
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 2)))
                    .text_color(gray::_100)
                    .background_color(surface::TERMINAL_BACKGROUND)
                    .desired_width(f32::INFINITY)
                    .desired_rows(line_count)
                    .layouter(&mut layouter)
                    .show(ui);
                changed = output.response.changed();
                cursor_byte = output
                    .cursor_range
                    .map(|range| byte_index(&editor.edited_yaml, range.primary.index.into()));
                caret_rect = output.cursor_range.map(|range| {
                    egui::text_selection::text_cursor_state::cursor_rect(
                        &output.galley,
                        &range.primary,
                        output
                            .galley
                            .rows
                            .first()
                            .map_or(typography::MONOSPACE_SIZE, |row| row.rect().height()),
                    )
                    .translate(output.galley_pos.to_vec2())
                });
                text_edit_id = Some(output.response.id);
                show_diagnostic_underlines(
                    ui,
                    editor.id,
                    output.galley.as_ref(),
                    output.galley_pos,
                    &editor.diagnostics,
                );
                if let Some(target) = &scroll_target
                    && let Some(rect) =
                        diagnostic_rects(output.galley.as_ref(), output.galley_pos, target).first()
                {
                    ui.scroll_to_rect(*rect, Some(egui::Align::Center));
                }
            });
        });

    let completion_position = caret_rect.map_or(fallback_completion_position, |caret_rect| {
        completion_popup_position(
            ctx.content_rect(),
            caret_rect,
            completion_popup_height(editor.suggestions.len()),
            completion_popup_width(&editor.suggestions),
        )
    });

    let cursor_moved = cursor_byte != editor.completion_cursor;
    if let Some(cursor) = cursor_byte
        && (changed || requested || (editor.suggestions_visible && cursor_moved))
    {
        if let Some(schema) = &editor.schema {
            let selected_label = editor
                .suggestions
                .get(editor.suggestion_selection)
                .map(|suggestion| suggestion.label.clone());
            let completion = schema.completion_at(&editor.edited_yaml, cursor);
            editor.suggestions = completion.suggestions;
            editor.completion_context = completion.context;
            editor.completion_cursor = Some(cursor);
            editor.suggestion_selection = selected_label
                .as_deref()
                .and_then(|label| {
                    editor
                        .suggestions
                        .iter()
                        .position(|suggestion| suggestion.label == label)
                })
                .unwrap_or(0);
            editor.suggestions_visible = requested || !editor.suggestions.is_empty();
        } else if requested {
            editor.suggestions.clear();
            editor.completion_context = None;
            editor.completion_cursor = Some(cursor);
            editor.suggestion_selection = 0;
            editor.suggestions_visible = true;
        }
    }
    if editor.suggestions_visible {
        let pointer_pressed = ctx.input(|input| input.pointer.any_pressed());
        let pointer_position = ctx.input(|input| input.pointer.interact_pos());
        let completion_list_width = completion_list_width(&editor.suggestions);
        let completion_popup_width = completion_list_width + 2.0 * spacing::MD;
        let mut selected_row_rect = None;
        let popup = egui::Area::new(egui::Id::new(("yaml-editor-suggestions", editor.id)))
            .order(egui::Order::Foreground)
            .fixed_pos(completion_position)
            .show(ctx, |ui| {
                ui.set_width(completion_popup_width);
                egui::Frame::new()
                    .fill(TOOLBAR_BACKGROUND)
                    .stroke(egui::Stroke::new(1.0, gray::_600))
                    .corner_radius(components::design::radius::surface())
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 5],
                        blur: 12,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(100),
                    })
                    .inner_margin(egui::Margin::same(spacing::MD as i8))
                    .show(ui, |ui| {
                        completion_context_header(ui, editor.completion_context.as_ref());
                        ui.add_space(spacing::XS);
                        let mut accepted = accept_suggestion.then_some(editor.suggestion_selection);
                        let mut hovered = None;
                        if editor.suggestions.is_empty() {
                            ui.add_sized(
                                egui::vec2(completion_list_width, COMPLETION_ROW_HEIGHT),
                                egui::Label::new(
                                    egui::RichText::new("No completions available")
                                        .font(typography::body())
                                        .color(gray::_500),
                                ),
                            );
                        } else {
                            components::scroll::vertical()
                                .id_salt(("yaml-editor-suggestion-list", editor.id))
                                .max_height(COMPLETION_LIST_MAX_HEIGHT)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.set_width(completion_list_width);
                                    for (index, suggestion) in editor.suggestions.iter().enumerate()
                                    {
                                        let is_selected = index == editor.suggestion_selection;
                                        let response = completion_row(
                                            ui,
                                            suggestion,
                                            is_selected,
                                            completion_list_width,
                                        );
                                        if is_selected {
                                            if selection_changed {
                                                response.scroll_to_me(Some(egui::Align::Center));
                                            }
                                            selected_row_rect = Some(response.rect);
                                        }
                                        if response.hovered() {
                                            hovered = Some((index, response.rect));
                                        }
                                        if response.clicked() {
                                            accepted = Some(index);
                                        }
                                    }
                                });
                        }
                        if let Some((index, rect)) = hovered {
                            editor.suggestion_selection = index;
                            selected_row_rect = Some(rect);
                        }
                        ui.add_space(spacing::XS);
                        ui.separator();
                        ui.add_space(spacing::XS);
                        ui.label(
                            egui::RichText::new(if editor.suggestions.is_empty() {
                                "Esc dismiss"
                            } else {
                                "↑↓ navigate  ·  Enter apply  ·  Esc dismiss"
                            })
                            .font(typography::metadata())
                            .color(gray::_500),
                        );
                        if let Some(index) = accepted
                            && let Some(suggestion) = editor.suggestions.get(index)
                            && let (Some(cursor), Some(id)) = (cursor_byte, text_edit_id)
                        {
                            let new_cursor = insert_suggestion(
                                &mut editor.edited_yaml,
                                cursor,
                                &suggestion.label,
                            );
                            let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id)
                                .unwrap_or_default();
                            state
                                .cursor
                                .set_char_range(Some(CCursorRange::one(CCursor::new(new_cursor))));
                            state.store(ctx, id);
                            editor.suggestions_visible = false;
                            editor.completion_cursor = None;
                            changed = true;
                        }
                    });
            });
        if pointer_pressed
            && pointer_position.is_some_and(|position| !popup.response.rect.contains(position))
        {
            editor.suggestions_visible = false;
            editor.completion_cursor = None;
        }
        if let Some(suggestion) = editor
            .suggestions
            .get(editor.suggestion_selection)
            .filter(|suggestion| suggestion.detail.is_some())
        {
            show_completion_documentation(
                ctx,
                popup.response.rect,
                selected_row_rect.unwrap_or_else(|| {
                    egui::Rect::from_min_size(completion_position, egui::Vec2::ZERO)
                }),
                suggestion,
            );
        }
    }
    changed
}

fn show_diagnostic_underlines(
    ui: &mut egui::Ui,
    editor_id: u64,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    diagnostics: &[YamlDiagnostic],
) {
    for (diagnostic_index, diagnostic) in diagnostics.iter().enumerate() {
        let Some(range) = &diagnostic.range else {
            continue;
        };
        for (segment_index, rect) in diagnostic_rects(galley, galley_pos, range)
            .into_iter()
            .enumerate()
        {
            let response = ui.interact(
                rect.expand2(egui::vec2(0.0, 3.0)),
                egui::Id::new((
                    "yaml-editor-diagnostic",
                    editor_id,
                    diagnostic_index,
                    segment_index,
                )),
                egui::Sense::hover(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Label,
                    true,
                    format!("Validation error: {}", diagnostic.message),
                )
            });
            response.on_hover_text(&diagnostic.message);
            paint_diagnostic_squiggle(ui.painter(), rect);
        }
    }
}

fn diagnostic_rects(
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    range: &SourceRange,
) -> Vec<egui::Rect> {
    let mut character_index = 0;
    let mut rects = Vec::new();
    for row in &galley.rows {
        let row_character_count: usize = row.char_count_including_newline().into();
        let row_end = character_index + row_character_count;
        let first = range.start.max(character_index);
        let last = range.end.min(row_end);
        let row_text_length: usize = row.char_count_excluding_newline().into();
        if first < last && first - character_index < row_text_length {
            let start_column = first - character_index;
            let end_column = (last - character_index).min(row_text_length);
            let end_column = end_column.max((start_column + 1).min(row_text_length));
            let row_rect = row.rect().translate(galley_pos.to_vec2());
            rects.push(egui::Rect::from_min_max(
                egui::pos2(
                    row_rect.left() + row.x_offset(egui::text::CharIndex(start_column)),
                    row_rect.top(),
                ),
                egui::pos2(
                    row_rect.left() + row.x_offset(egui::text::CharIndex(end_column)),
                    row_rect.bottom(),
                ),
            ));
        }
        character_index = row_end;
    }
    rects
}

fn paint_diagnostic_squiggle(painter: &egui::Painter, rect: egui::Rect) {
    let wavelength = 4.0;
    let amplitude = 1.5;
    let baseline = rect.bottom() - 2.0;
    let steps = (rect.width() / (wavelength / 2.0)).ceil() as usize;
    let points = (0..=steps)
        .map(|step| {
            let x = (rect.left() + step as f32 * (wavelength / 2.0)).min(rect.right());
            let y = baseline + if step % 2 == 0 { -amplitude } else { amplitude };
            egui::pos2(x, y)
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5, status::DANGER),
    ));
}

fn yaml_editor_text_edit_id(editor_id: u64) -> egui::Id {
    egui::Id::new(("yaml-editor-text", editor_id))
}

fn completion_popup_height(suggestion_count: usize) -> f32 {
    COMPLETION_POPUP_CHROME_HEIGHT
        + (suggestion_count as f32 * COMPLETION_ROW_HEIGHT).min(COMPLETION_LIST_MAX_HEIGHT)
}

fn completion_popup_position(
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

fn completion_context_header(ui: &mut egui::Ui, context: Option<&CompletionContext>) {
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

fn completion_row(
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
    response.interact(egui::Sense::click())
}

fn completion_list_width(suggestions: &[crate::resource_schema::CompletionSuggestion]) -> f32 {
    let longest_label = suggestions
        .iter()
        .map(|suggestion| suggestion.label.chars().count())
        .max()
        .unwrap_or("No completions available".chars().count());
    let label_width = longest_label as f32 * typography::MONOSPACE_SIZE * 0.65;
    (label_width + 2.0 * spacing::SM).clamp(COMPLETION_LIST_MIN_WIDTH, COMPLETION_LIST_MAX_WIDTH)
}

fn completion_popup_width(suggestions: &[crate::resource_schema::CompletionSuggestion]) -> f32 {
    completion_list_width(suggestions) + 2.0 * spacing::MD
}

fn completion_type_badge(ui: &mut egui::Ui, type_label: &str) -> egui::Response {
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

fn show_completion_documentation(
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

fn completion_documentation_position(
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

fn byte_index(text: &str, character_index: usize) -> usize {
    text.char_indices()
        .nth(character_index)
        .map_or(text.len(), |(index, _)| index)
}

fn insert_suggestion(text: &mut String, cursor: usize, suggestion: &str) -> usize {
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

fn refresh_local_validation(editor: &mut YamlEditorWindowState) {
    editor.validation_revision += 1;
    editor.validation_due = None;
    editor.server_validation = ValidationState::Idle;
    editor.diagnostics.clear();
    match &editor.schema {
        Some(schema) => match schema.validate_yaml(&editor.edited_yaml) {
            Ok(diagnostics) => {
                editor.diagnostics = diagnostics;
                if editor.diagnostics.is_empty() {
                    editor.validation_due = Some(Instant::now() + VALIDATION_DEBOUNCE);
                }
            }
            Err(message) if message.starts_with("Unable to compile the Kubernetes schema:") => {
                // A malformed or unsupported OpenAPI extension must not prevent the API server
                // from validating the document. Treat the local schema as unavailable.
                editor.server_validation = ValidationState::Failed(message);
                editor.validation_due = Some(Instant::now() + VALIDATION_DEBOUNCE);
            }
            Err(message) => editor.diagnostics.push(YamlDiagnostic {
                range: yaml_error_range(&editor.edited_yaml, &message),
                line: yaml_error_line(&message),
                path: String::new(),
                message,
            }),
        },
        None => match serde_yaml::from_str::<serde_yaml::Value>(&editor.edited_yaml) {
            Ok(_) => editor.validation_due = Some(Instant::now() + VALIDATION_DEBOUNCE),
            Err(error) => editor.diagnostics.push(YamlDiagnostic {
                range: error.location().and_then(|location| {
                    SourceRange::at_yaml_location(
                        &editor.edited_yaml,
                        location.line(),
                        location.column(),
                    )
                }),
                line: error.location().map(|location| location.line()),
                path: String::new(),
                message: error.to_string(),
            }),
        },
    }
    if !editor.diagnostics.is_empty() {
        editor.retained_diagnostics = editor.diagnostics.clone();
    }
}

fn yaml_error_line(message: &str) -> Option<usize> {
    message
        .rsplit_once(" at ")
        .and_then(|(_, location)| location.split_once(':'))
        .and_then(|(line, _)| line.parse().ok())
}

fn yaml_error_range(yaml: &str, message: &str) -> Option<SourceRange> {
    let (line, column) = message.rsplit_once(" at ")?.1.split_once(':')?;
    SourceRange::at_yaml_location(yaml, line.parse().ok()?, column.parse().ok()?)
}

fn maybe_request_server_validation(
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    if editor
        .validation_due
        .is_some_and(|due| due <= Instant::now())
    {
        editor.validation_due = None;
        editor.server_validation = ValidationState::Pending;
        commands_to_send.push(WorkerCommand::ValidateResourceYaml {
            editor_id: editor.id,
            revision: editor.validation_revision,
            cluster_key: editor.cluster_key,
            api_resource: editor.api_resource.clone(),
            namespace: editor.namespace.clone(),
            resource_name: editor.resource_name.clone(),
            yaml: editor.edited_yaml.clone(),
        });
    }
}

fn show_diagnostics(ctx: &egui::Context, ui: &mut egui::Ui, editor: &mut YamlEditorWindowState) {
    let (diagnostics, showing_retained_diagnostics) = diagnostics_to_display(editor);
    if let ValidationState::Failed(message) = &editor.server_validation {
        error_strip(ui, message);
    }
    if diagnostics.is_empty() {
        match &editor.server_validation {
            ValidationState::Pending => {
                status_indicator(ui, gray::_400, "Validating with cluster…")
            }
            ValidationState::Valid => status_indicator(ui, status::SUCCESS, "Validated by cluster"),
            ValidationState::Failed(_) => {}
            ValidationState::Idle => {
                if editor.schema_loading {
                    status_indicator(ui, gray::_400, "Loading Kubernetes schema…");
                }
            }
        }
        return;
    }
    let mut range_to_focus = None;
    egui::CollapsingHeader::new(format!("{} diagnostics", diagnostics.len()))
        .default_open(true)
        .show(ui, |ui| {
            if showing_retained_diagnostics {
                status_indicator(ui, gray::_400, "Validating updated YAML…");
            }
            components::scroll::vertical()
                .id_salt(("yaml-editor-diagnostic-list", editor.id))
                .max_height(DIAGNOSTIC_LIST_MAX_HEIGHT)
                .min_scrolled_height(DIAGNOSTIC_LIST_MAX_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for diagnostic in diagnostics {
                        let location = diagnostic
                            .line
                            .map(|line| format!("Line {line}: "))
                            .unwrap_or_default();
                        let button = egui::Button::new(
                            egui::RichText::new(format!("{location}{}", diagnostic.message))
                                .font(typography::metadata())
                                .color(status::DANGER),
                        )
                        .frame(false);
                        let response = if showing_retained_diagnostics {
                            ui.add_enabled(false, button).on_hover_text(
                                "Validating the updated YAML before this diagnostic can be located",
                            )
                        } else {
                            ui.add(button)
                                .with_pointing_hand()
                                .on_hover_text("Jump to the highlighted YAML location")
                        };
                        if !showing_retained_diagnostics && response.clicked() {
                            range_to_focus = diagnostic.range.clone();
                        }
                    }
                });
        });
    if let Some(range) = range_to_focus {
        focus_diagnostic(ctx, editor, range);
    }
}

fn diagnostics_to_display(editor: &YamlEditorWindowState) -> (&[YamlDiagnostic], bool) {
    let validation_in_progress = editor.validation_due.is_some()
        || matches!(editor.server_validation, ValidationState::Pending);
    if editor.diagnostics.is_empty()
        && validation_in_progress
        && !editor.retained_diagnostics.is_empty()
    {
        (&editor.retained_diagnostics, true)
    } else {
        (&editor.diagnostics, false)
    }
}

fn focus_diagnostic(ctx: &egui::Context, editor: &mut YamlEditorWindowState, range: SourceRange) {
    let text_edit_id = yaml_editor_text_edit_id(editor.id);
    ctx.memory_mut(|memory| memory.request_focus(text_edit_id));
    let mut state =
        egui::widgets::text_edit::TextEditState::load(ctx, text_edit_id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(range.start),
        CCursor::new(range.end),
    )));
    state.store(ctx, text_edit_id);
    editor.scroll_to_diagnostic = Some(range);
    ctx.request_repaint();
}

fn has_diagnostics_feedback(editor: &YamlEditorWindowState) -> bool {
    !diagnostics_to_display(editor).0.is_empty()
        || editor.schema_loading
        || !matches!(editor.server_validation, ValidationState::Idle)
}

fn apply_editor(editor: &mut YamlEditorWindowState, commands_to_send: &mut Vec<WorkerCommand>) {
    editor.saving = true;
    editor.error = None;
    commands_to_send.push(WorkerCommand::ApplyResourceYaml {
        editor_id: editor.id,
        cluster_key: editor.cluster_key,
        api_resource: editor.api_resource.clone(),
        namespace: editor.namespace.clone(),
        resource_name: editor.resource_name.clone(),
        yaml: editor.edited_yaml.clone(),
    });
}

fn request_close(ctx: &egui::Context, editor: &mut YamlEditorWindowState) {
    if editor.is_modified() {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        editor.confirm_discard = true;
    } else {
        editor.close_requested = true;
    }
}

fn show_discard_confirmation(ctx: &egui::Context, editor: &mut YamlEditorWindowState) {
    if !editor.confirm_discard {
        return;
    }
    match (ConfirmationDialog {
        id: egui::Id::new(("discard-yaml-changes", editor.id)),
        eyebrow: "Unsaved changes",
        title: "Discard changes?",
        message: "Your unsaved edits will be lost.",
        unavailable_message: None,
        cancel_label: "Keep editing",
        confirm_label: "Discard changes",
        kind: ConfirmationDialogKind::Destructive,
        confirm_enabled: true,
    })
    .show(ctx)
    {
        ConfirmationDialogAction::Confirm => {
            editor.close_requested = true;
            editor.confirm_discard = false;
        }
        ConfirmationDialogAction::Cancel => editor.confirm_discard = false,
        ConfirmationDialogAction::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_resource::ApiResource;
    use crate::resource_schema::{
        CompletionContext, CompletionContextKind, CompletionSuggestion, ResourceSchema,
    };
    use egui_kittest::{Harness, SnapshotOptions, kittest::Queryable};
    use k8s_openapi::serde_json::json;

    const COMPLETION_POPUP_MAX_WIDTH: f32 = COMPLETION_LIST_MAX_WIDTH + 2.0 * spacing::MD;
    const COMPLETION_POPUP_MAX_HEIGHT: f32 =
        COMPLETION_POPUP_CHROME_HEIGHT + COMPLETION_LIST_MAX_HEIGHT;

    #[test]
    fn yaml_highlighting_uses_the_yaml_language() {
        let ctx = egui::Context::default();
        let theme = CodeTheme::dark(typography::MONOSPACE_SIZE);
        let job = highlight(
            &ctx,
            &egui::Style::default(),
            &theme,
            "kind: ConfigMap",
            "yaml",
        );

        assert!(!job.sections.is_empty());
    }

    #[test]
    fn yaml_editor_clean_snapshot() {
        snapshot_editor(
            editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings"),
            "yaml_editor/clean",
        );
    }

    #[test]
    fn yaml_editor_modified_snapshot() {
        let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
        editor.edited_yaml.push_str("\ndata:\n  mode: development");
        snapshot_editor(editor, "yaml_editor/modified");
    }

    #[test]
    fn command_enter_does_not_apply_yaml_changes() {
        let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
        editor.edited_yaml.push_str("\ndata:\n  mode: development");
        let mut harness = editor_harness(editor);

        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
        harness.run();

        assert!(!harness.state().editor.saving);
        assert!(
            harness
                .state()
                .commands
                .iter()
                .all(|command| !matches!(command, WorkerCommand::ApplyResourceYaml { .. }))
        );
    }

    #[test]
    fn yaml_editor_apply_error_snapshot() {
        let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
        editor.edited_yaml.push_str("\ndata:\n  mode: development");
        editor.error = Some("The Kubernetes API rejected this resource".into());
        snapshot_editor(editor, "yaml_editor/apply_error");
    }

    #[test]
    fn yaml_editor_discard_confirmation_snapshot() {
        let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
        editor.edited_yaml.push_str("\ndata:\n  mode: development");
        editor.confirm_discard = true;
        snapshot_editor(editor, "yaml_editor/discard_confirmation");
    }

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
        snapshot_editor(editor, "yaml_editor/completion");
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
        snapshot_editor(editor, "yaml_editor/value_completion");
    }

    #[test]
    fn yaml_editor_deep_array_value_completion_snapshot() {
        let yaml = "spec:\n  templates:\n    - spec:\n        containers:\n          - imagePullPolicy: Al";
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
        snapshot_editor(editor, "yaml_editor/deep_array_value_completion");
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
        harness.snapshot("yaml_editor/completion_keyboard_navigation");
    }

    #[test]
    fn yaml_editor_completion_bottom_right_caret_snapshot() {
        let yaml = format!("{}deep: {}", "filler: value\n".repeat(48), "x".repeat(160));
        snapshot_completion_at_focused_caret(&yaml, "yaml_editor/completion_bottom_right_caret");
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
        harness.snapshot("yaml_editor/focused_caret");
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
        harness.snapshot("yaml_editor/no_completions");
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

    #[test]
    fn yaml_editor_completion_top_right_caret_snapshot() {
        let yaml = format!("mode: {}", "x".repeat(160));
        snapshot_completion_at_focused_caret(&yaml, "yaml_editor/completion_top_right_caret");
    }

    #[test]
    fn yaml_editor_completion_bottom_left_caret_snapshot() {
        let yaml = format!("{}mode: Read", "filler: value\n".repeat(48));
        snapshot_completion_at_focused_caret(&yaml, "yaml_editor/completion_bottom_left_caret");
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
        let bottom_right =
            egui::Rect::from_min_size(egui::pos2(1580.0, 870.0), egui::vec2(2.0, 18.0));

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
        assert!(
            position.x + COMPLETION_DOCUMENTATION_WIDTH + spacing::SM <= completion_popup.left()
        );
    }

    #[test]
    fn yaml_editor_diagnostics_snapshot() {
        snapshot_editor(diagnostic_editor(), "yaml_editor/diagnostics");
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

        snapshot_editor(editor, "yaml_editor/diagnostics_many");
    }

    #[test]
    fn yaml_editor_diagnostic_tooltip_snapshot() {
        let mut harness = editor_harness(diagnostic_editor());
        harness
            .get_by_label("Validation error: \"settings\" is not an allowed value")
            .hover();
        harness.run();
        harness.snapshot("yaml_editor/diagnostics_tooltip");
    }

    #[test]
    fn yaml_editor_retained_diagnostics_snapshot() {
        let mut editor = diagnostic_editor();
        editor.diagnostics.clear();
        editor.server_validation = ValidationState::Pending;
        snapshot_editor(editor, "yaml_editor/diagnostics_validating");
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

    fn editor(yaml: &str) -> YamlEditorWindowState {
        YamlEditorWindowState {
            id: 1,
            cluster_key: 7,
            api_resource: ApiResource {
                group: "core".into(),
                version: "v1".into(),
                kind: "ConfigMap".into(),
                name: "configmaps".into(),
                namespaced: true,
            },
            namespace: Some("kube-system".into()),
            resource_name: "settings".into(),
            original_yaml: Some(yaml.into()),
            edited_yaml: yaml.into(),
            loading: false,
            saving: false,
            error: None,
            close_requested: false,
            confirm_discard: false,
            focus_requested: false,
            schema: None,
            schema_loading: false,
            diagnostics: Vec::new(),
            retained_diagnostics: Vec::new(),
            scroll_to_diagnostic: None,
            server_validation: ValidationState::Idle,
            validation_revision: 0,
            validation_due: None,
            suggestions: Vec::new(),
            completion_context: None,
            completion_cursor: None,
            suggestions_visible: false,
            suggestion_selection: 0,
        }
    }

    fn diagnostic_editor() -> YamlEditorWindowState {
        let yaml = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings";
        let start = yaml
            .find("settings")
            .map(|index| yaml[..index].chars().count())
            .expect("diagnostic text is present");
        let mut editor = editor(yaml);
        editor.validation_revision = 1;
        editor.diagnostics = vec![YamlDiagnostic {
            path: "/metadata/name".into(),
            message: "\"settings\" is not an allowed value".into(),
            line: Some(4),
            range: Some(SourceRange {
                start,
                end: start + "settings".chars().count(),
            }),
        }];
        editor.retained_diagnostics = editor.diagnostics.clone();
        editor
    }

    fn snapshot_editor(editor: YamlEditorWindowState, name: &str) {
        let confirm_discard = editor.confirm_discard;
        let mut editor = editor;
        editor.confirm_discard = false;
        // Snapshot rendering can take longer than the server-validation
        // debounce when the suite is running in parallel. Keep ordinary
        // fixtures in their explicitly clean state; validation-specific
        // fixtures set a non-idle state before reaching this helper.
        if matches!(editor.server_validation, ValidationState::Idle)
            && editor.diagnostics.is_empty()
            && !editor.schema_loading
            && editor.validation_due.is_none()
        {
            editor.validation_revision = editor.validation_revision.max(1);
            editor.validation_due = None;
        }
        let mut harness = Harness::builder().build_ui_state(
            |ctx, state: &mut SnapshotState| {
                show_editor_window(ctx, &mut state.editor, &mut state.commands);
            },
            SnapshotState {
                editor,
                commands: Vec::new(),
                ctx: None,
            },
        );
        components::test_support::setup_egui(&mut harness);
        harness.state_mut().editor.confirm_discard = confirm_discard;
        harness.run();
        harness.snapshot(name);
    }

    struct SnapshotState {
        editor: YamlEditorWindowState,
        commands: Vec<WorkerCommand>,
        ctx: Option<egui::Context>,
    }

    fn editor_harness(editor: YamlEditorWindowState) -> Harness<'static, SnapshotState> {
        let mut harness = Harness::builder().build_ui_state(
            |ctx, state: &mut SnapshotState| {
                state.ctx = Some(ctx.clone());
                show_editor_window(ctx, &mut state.editor, &mut state.commands);
            },
            SnapshotState {
                editor,
                commands: Vec::new(),
                ctx: None,
            },
        );
        components::test_support::setup_egui(&mut harness);
        harness.run();
        harness
    }

    fn many_suggestions(count: usize) -> Vec<CompletionSuggestion> {
        (0..count)
            .map(|index| CompletionSuggestion {
                label: format!("field-{index:03}"),
                type_label: Some("string".into()),
                detail: Some(format!("Documentation for field {index:03}.")),
            })
            .collect()
    }

    fn snapshot_completion_at_focused_caret(yaml: &str, name: &str) {
        let mut editor = editor(yaml);
        editor.suggestions = many_suggestions(3);
        editor.suggestions_visible = true;
        editor.validation_revision = 1;
        let mut harness = editor_harness(editor);
        let ctx = harness
            .state()
            .ctx
            .clone()
            .expect("editor context is available");
        let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
        set_editor_caret(&ctx, text_edit_id, yaml.chars().count());

        harness.run();
        harness.snapshot_options(name, &SnapshotOptions::new().max_failed_pixels(1));
    }

    fn set_editor_caret(ctx: &egui::Context, id: egui::Id, character_index: usize) {
        ctx.memory_mut(|memory| memory.request_focus(id));
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id)
            .expect("text editor state is available");
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(character_index))));
        state.store(ctx, id);
    }

    fn editor_caret(ctx: &egui::Context, id: egui::Id) -> usize {
        egui::widgets::text_edit::TextEditState::load(ctx, id)
            .and_then(|state| state.cursor.char_range())
            .expect("editor caret is available")
            .primary
            .index
            .into()
    }
}
