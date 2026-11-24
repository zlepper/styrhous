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

    let theme = ui.tokens();
    let line_count = editor.edited_yaml.lines().count().max(1);
    let gutter = components::CodeSurface::gutter_width(ui, line_count);
    let mut changed = false;
    let mut cursor_byte = None;
    let mut text_edit_id = None;
    let mut caret_rect = None;
    apply_pending_focus(ctx, editor.id);
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
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            // egui's glyphs sit at the top of their rows; the shift puts them where CSS does.
            let row_shift = type_scale::MONO.row_shift(ui.ctx());
            ui.add_space(size::CODE_PADDING_TOP + row_shift);
            // Painted once the caret's row is known, but behind the text.
            let line_highlight = ui.painter().add(egui::Shape::Noop);
            // The rule scrolls with the line numbers; it spans the visible height.
            let rule_x =
                ui.min_rect().left() + gutter - (size::CODE_GUTTER - size::CODE_GUTTER_RULE);
            ui.painter().add(egui::Shape::rect_filled(
                egui::Rect::from_x_y_ranges(
                    rule_x..=rule_x + size::HAIRLINE,
                    ui.clip_rect().y_range(),
                ),
                0,
                theme.code.divider,
            ));
            ui.horizontal_top(|ui| {
                show_line_numbers(ui, line_count, &editor.diagnostics, &theme, gutter);
                let mut layouter =
                    |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
                        let mut layout_job = yaml_editor_layout_job(
                            ui,
                            &mut editor.highlight_cache,
                            buffer.as_str(),
                            &search_query,
                            search_regex_mode,
                            active_match.as_ref(),
                        );
                        layout_job.wrap.max_width = wrap_width;
                        ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
                    };
                ui.style_mut().visuals.text_cursor.stroke =
                    egui::Stroke::new(4.0, theme.code.caret);
                ui.style_mut().visuals.text_cursor.blink = false;
                let output = egui::TextEdit::multiline(&mut editor.edited_yaml)
                    .id(yaml_editor_text_edit_id(editor.id))
                    .font(type_scale::MONO.font_id(ui.ctx()))
                    .code_editor()
                    .frame(egui::Frame::new())
                    .text_color(theme.code.text)
                    .background_color(egui::Color32::TRANSPARENT)
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
                text_edit_id = Some(output.response.id);
                // The caret's line stays highlighted while a dialog has the focus.
                let caret = output
                    .cursor_range
                    .or_else(|| output.state.cursor.range(&output.galley));
                if let Some(range) = caret {
                    let row = output.galley.pos_from_cursor(range.primary);
                    let row =
                        row.translate(output.galley_pos.to_vec2() - egui::vec2(0.0, row_shift));
                    caret_rect = Some(egui::Rect::from_min_size(
                        row.left_top() + egui::vec2(0.0, size::CODE_CARET_INSET),
                        size::CODE_CARET,
                    ));
                    let highlight = egui::Rect::from_x_y_ranges(
                        ui.clip_rect().x_range(),
                        row.top()..=row.top() + type_scale::MONO.line_height(),
                    );
                    ui.painter().set(
                        line_highlight,
                        egui::Shape::rect_filled(highlight, 0, theme.code.line_highlight),
                    );
                }
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

    // Popovers stay inside the code area, clear of its edges.
    let surface = ui.clip_rect().shrink(space::S12);
    let popup_size = egui::vec2(
        size::COMPLETION_WIDTH,
        components::Completion::height(editor.suggestions.len(), COMPLETION_LIST_MAX_HEIGHT),
    );
    let completion_position = caret_rect.map_or(fallback_completion_position, |caret_rect| {
        components::completion_position(surface, caret_rect, popup_size)
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
        let labels: Vec<&str> = editor
            .suggestions
            .iter()
            .map(|suggestion| suggestion.label.as_str())
            .collect();
        let context = editor.completion_context.as_ref();
        let expects = match context.map(|context| &context.kind) {
            Some(CompletionContextKind::MappingKey) => CompletionExpects::Key,
            Some(CompletionContextKind::Value) => CompletionExpects::Value,
            None => CompletionExpects::Anything,
        };
        let popup = egui::Area::new(egui::Id::new(("yaml-editor-suggestions", editor.id)))
            .order(egui::Order::Foreground)
            .fixed_pos(completion_position)
            .constrain_to(surface)
            .show(ctx, |ui| {
                let mut completion = Completion::new(expects, &labels)
                    .id_salt(("yaml-editor-suggestion-list", editor.id))
                    .active((!labels.is_empty()).then_some(editor.suggestion_selection))
                    .max_rows_height(COMPLETION_LIST_MAX_HEIGHT)
                    .scroll_to_active(selection_changed)
                    .option_label("Insert YAML completion");
                if let Some(type_label) = context.and_then(|context| context.type_label.as_deref())
                {
                    completion = completion.type_label(type_label);
                }
                completion.show(ui)
            });
        let completion = popup.inner;
        if let Some((index, _)) = completion.hovered {
            editor.suggestion_selection = index;
        }
        let accepted = completion
            .clicked
            .or(accept_suggestion.then_some(editor.suggestion_selection));
        if let Some(index) = accepted
            && let Some(suggestion) = editor.suggestions.get(index)
            && let (Some(cursor), Some(id)) = (cursor_byte, text_edit_id)
        {
            let new_cursor = insert_suggestion(&mut editor.edited_yaml, cursor, &suggestion.label);
            let mut state =
                egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(new_cursor))));
            state.store(ctx, id);
            editor.suggestions_visible = false;
            editor.completion_cursor = None;
            changed = true;
        }
        if pointer_pressed
            && pointer_position.is_some_and(|position| !popup.response.rect.contains(position))
        {
            editor.suggestions_visible = false;
            editor.completion_cursor = None;
        }
        if editor.suggestions_visible
            && let Some(suggestion) = editor
                .suggestions
                .get(editor.suggestion_selection)
                .filter(|suggestion| suggestion.detail.is_some())
        {
            show_completion_documentation(ctx, surface, popup.response.rect, suggestion);
        }
    }
    changed
}
