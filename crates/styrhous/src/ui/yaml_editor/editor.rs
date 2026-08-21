use super::*;

pub(super) fn show_code_editor(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
    search_matches: Option<&Vec<Range<usize>>>,
    #[cfg(any(test, feature = "benchmarks"))] scroll_metrics: Option<&mut YamlEditorScrollMetrics>,
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
    let search_scroll_target = editor
        .search
        .scroll_to_match
        .take()
        .and_then(|index| search_matches.and_then(|matches| matches.get(index)))
        .cloned();
    let search_query = editor.search.query.clone();
    let search_regex_mode = editor.search.regex_mode;
    let active_match = editor
        .search
        .active_match
        .and_then(|index| search_matches.and_then(|matches| matches.get(index)))
        .cloned();
    let fallback_completion_position = ui.clip_rect().left_top() + egui::vec2(74.0, 68.0);
    let _scroll_output = components::scroll::both()
        .id_salt(("yaml-editor-scroll", editor.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(36.0);
                    ui.add_space(2.0);
                    ui.label(line_number_layout_job(line_count, &editor.diagnostics));
                });
                ui.separator();
                let theme = CodeTheme::dark(typography::MONOSPACE_SIZE);
                let mut layouter =
                    |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
                        let mut layout_job = yaml_editor_layout_job(
                            ui,
                            &mut editor.highlight_cache,
                            &theme,
                            buffer.as_str(),
                            &search_query,
                            search_regex_mode,
                            active_match.as_ref(),
                        );
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
                ui.ctx()
                    .accesskit_node_builder(output.response.id, |builder| {
                        builder.set_label("Kubernetes resource manifest");
                    });
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
                if let Some(target) = search_scroll_target
                    .as_ref()
                    .map(|range| source_range_for_bytes(&editor.edited_yaml, range.clone()))
                    && let Some(rect) =
                        diagnostic_rects(output.galley.as_ref(), output.galley_pos, &target).first()
                {
                    ui.scroll_to_rect(*rect, Some(egui::Align::Center));
                }
            });
        });
    #[cfg(any(test, feature = "benchmarks"))]
    if let Some(scroll_metrics) = scroll_metrics {
        scroll_metrics.offset = _scroll_output.state.offset;
        scroll_metrics.inner_rect = _scroll_output.inner_rect;
    }

    if changed {
        clear_active_match(editor);
    }

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
