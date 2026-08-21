use super::*;

fn log_row_for_display_row(
    window: &PodLogWindowState,
    display_row: usize,
) -> Option<&crate::log_store::LogPageRow> {
    let page_start = display_row / window.page_size * window.page_size;
    let key = LogPageKey {
        generation: window.search.generation,
        filter_matches: filter_is_active(window),
        page_start,
    };
    window
        .pages
        .get(&key)
        .and_then(|page| page.rows.get(display_row - page_start))
        .or_else(|| {
            (!filter_is_active(window))
                .then(|| window.live_rows.get(&display_row))
                .flatten()
        })
}

pub(super) fn resolve_pending_caret(
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    display_count: usize,
    _ctx: &egui::Context,
) {
    let Some(pending) = window.pending_caret else {
        return;
    };
    if pending.display_row >= display_count {
        window.pending_caret = None;
        return;
    }
    let Some(position) =
        log_row_for_display_row(window, pending.display_row).map(|row| LogTextPosition {
            display_row: pending.display_row,
            byte_offset: byte_offset_at_character_column(&row.text, pending.character_column),
        })
    else {
        request_page_for_display_row(window, log_store, pending.display_row);
        return;
    };
    window.set_selection(Some(LogTextSelection {
        anchor: pending.anchor.unwrap_or(position),
        focus: position,
    }));
    window.pending_caret = None;
    window.ensure_caret_visible = true;
}

pub(super) fn handle_log_keyboard(
    ctx: &egui::Context,
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    display_count: usize,
    page_rows: usize,
) {
    if display_count == 0 || window.pending_caret.is_some() {
        return;
    }
    let events = ctx.input(|input| input.events.clone());
    for event in events {
        let egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } = event
        else {
            continue;
        };
        if move_log_caret(
            window,
            log_store,
            display_count,
            page_rows,
            key,
            modifiers,
            ctx.input(|input| input.time),
        ) {
            ctx.input_mut(|input| {
                input.consume_key(modifiers, key);
            });
            // egui turns unconsumed arrow keys into directional focus
            // navigation at the end of the frame. This canvas owns the arrow
            // keys while its caret is active, so suppress that traversal.
            ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        }
    }
}

pub(super) fn move_log_caret(
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    display_count: usize,
    page_rows: usize,
    key: egui::Key,
    modifiers: egui::Modifiers,
    _interaction_time: f64,
) -> bool {
    if key == egui::Key::A && modifiers.command {
        let last_row = display_count - 1;
        set_caret_target(
            window,
            log_store,
            last_row,
            usize::MAX,
            Some(LogTextPosition {
                display_row: 0,
                byte_offset: 0,
            }),
            _interaction_time,
        );
        return true;
    }

    let Some(selection) = window.selection else {
        return false;
    };
    let mut focus = selection.focus;
    let mut anchor = modifiers.shift.then_some(selection.anchor);
    if !modifiers.shift && !selection_is_empty(selection) {
        match key {
            egui::Key::ArrowLeft => focus = selection.normalized().0,
            egui::Key::ArrowRight => focus = selection.normalized().1,
            _ => {}
        }
        if matches!(key, egui::Key::ArrowLeft | egui::Key::ArrowRight) {
            window.set_selection(Some(LogTextSelection {
                anchor: focus,
                focus,
            }));
            window.caret_preferred_column = None;
            window.ensure_caret_visible = true;
            return true;
        }
    }

    let Some(text) = log_row_for_display_row(window, focus.display_row).map(|row| row.text.clone())
    else {
        request_page_for_display_row(window, log_store, focus.display_row);
        return false;
    };
    let character_column = character_column_at_byte(&text, focus.byte_offset);
    let line_length = text.chars().count();
    let mut target_row = focus.display_row;
    let mut target_column = character_column;
    let mut preserve_column = false;

    match key {
        egui::Key::ArrowLeft => {
            if modifiers.alt || modifiers.ctrl {
                target_column = egui::text_selection::text_cursor_state::ccursor_previous_word(
                    &text,
                    egui::text::CCursor::new(character_column),
                )
                .index
                .into();
            } else if modifiers.mac_cmd {
                target_column = 0;
            } else if character_column > 0 {
                target_column = character_column - 1;
            } else if target_row > 0 {
                target_row -= 1;
                target_column = usize::MAX;
            }
        }
        egui::Key::ArrowRight => {
            if modifiers.alt || modifiers.ctrl {
                target_column = egui::text_selection::text_cursor_state::ccursor_next_word(
                    &text,
                    egui::text::CCursor::new(character_column),
                )
                .index
                .into();
            } else if modifiers.mac_cmd {
                target_column = line_length;
            } else if character_column < line_length {
                target_column = character_column + 1;
            } else if target_row + 1 < display_count {
                target_row += 1;
                target_column = 0;
            }
        }
        egui::Key::ArrowUp => {
            if modifiers.command {
                target_row = 0;
                target_column = 0;
            } else {
                target_row = target_row.saturating_sub(1);
                target_column = window.caret_preferred_column.unwrap_or(character_column);
                preserve_column = true;
            }
        }
        egui::Key::ArrowDown => {
            if modifiers.command {
                target_row = display_count - 1;
                target_column = usize::MAX;
            } else {
                target_row = (target_row + 1).min(display_count - 1);
                target_column = window.caret_preferred_column.unwrap_or(character_column);
                preserve_column = true;
            }
        }
        egui::Key::Home => {
            if modifiers.command {
                target_row = 0;
            }
            target_column = 0;
        }
        egui::Key::End => {
            if modifiers.command {
                target_row = display_count - 1;
            }
            target_column = usize::MAX;
        }
        egui::Key::PageUp => {
            target_row = target_row.saturating_sub(page_rows);
            target_column = window.caret_preferred_column.unwrap_or(character_column);
            preserve_column = true;
        }
        egui::Key::PageDown => {
            target_row = (target_row + page_rows).min(display_count - 1);
            target_column = window.caret_preferred_column.unwrap_or(character_column);
            preserve_column = true;
        }
        _ => return false,
    }
    if preserve_column {
        window.caret_preferred_column = Some(target_column);
    } else {
        window.caret_preferred_column = None;
    }
    set_caret_target(
        window,
        log_store,
        target_row,
        target_column,
        anchor.take(),
        _interaction_time,
    );
    true
}

pub(super) fn set_caret_target(
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    display_row: usize,
    character_column: usize,
    anchor: Option<LogTextPosition>,
    _interaction_time: f64,
) {
    let position = log_row_for_display_row(window, display_row).map(|row| LogTextPosition {
        display_row,
        byte_offset: byte_offset_at_character_column(&row.text, character_column),
    });
    if let Some(position) = position {
        window.set_selection(Some(LogTextSelection {
            anchor: anchor.unwrap_or(position),
            focus: position,
        }));
        window.pending_caret = None;
        window.ensure_caret_visible = true;
    } else {
        window.pending_caret = Some(PendingLogCaret {
            display_row,
            character_column,
            anchor,
        });
        request_page_for_display_row(window, log_store, display_row);
    }
}

pub(super) fn selection_is_empty(selection: LogTextSelection) -> bool {
    selection.anchor == selection.focus
}

pub(super) fn character_column_at_byte(text: &str, byte_offset: usize) -> usize {
    egui::text_selection::text_cursor_state::char_index_from_byte_index(
        text,
        egui::text::ByteIndex(byte_offset),
    )
    .into()
}

pub(super) fn byte_offset_at_character_column(text: &str, character_column: usize) -> usize {
    egui::text_selection::text_cursor_state::byte_index_from_char_index(
        text,
        egui::text::CharIndex(character_column),
    )
    .into()
}

pub(super) fn caret_horizontal_offset(
    window: &PodLogWindowState,
    display_options: LogDisplayOptions,
    horizontal_offset: f32,
    viewport_width: f32,
    character_width: f32,
) -> Option<f32> {
    let focus = window.selection?.focus;
    let row = log_row_for_display_row(window, focus.display_row)?;
    let prefix_width = log_line_prefix(row.line_index, row.timestamp.as_deref(), display_options)
        .chars()
        .count() as f32
        * character_width;
    let caret_x = prefix_width
        + character_column_at_byte(&row.text, focus.byte_offset) as f32 * character_width;
    if caret_x < horizontal_offset {
        Some(caret_x)
    } else if caret_x + character_width > horizontal_offset + viewport_width {
        Some((caret_x + character_width - viewport_width).max(0.0))
    } else {
        Some(horizontal_offset)
    }
}

pub(super) fn caret_vertical_offset(
    window: &PodLogWindowState,
    vertical_offset: f32,
    viewport_height: f32,
    row_step: f32,
) -> f32 {
    let Some(focus) = window.selection.map(|selection| selection.focus) else {
        return vertical_offset;
    };
    let caret_top = focus.display_row as f32 * row_step;
    let caret_bottom = caret_top + row_step;
    if caret_top < vertical_offset {
        caret_top
    } else if caret_bottom > vertical_offset + viewport_height {
        (caret_bottom - viewport_height).max(0.0)
    } else {
        vertical_offset
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_log_caret(
    ui: &egui::Ui,
    ctx: &egui::Context,
    window: &PodLogWindowState,
    focus_id: egui::Id,
    display_row: usize,
    text: &str,
    byte_range: &std::ops::Range<usize>,
    response_rect: egui::Rect,
    text_left: f32,
    character_width: f32,
    row_height: f32,
) {
    if !ctx.memory(|memory| memory.has_focus(focus_id)) {
        return;
    }
    let Some(focus) = window.selection.map(|selection| selection.focus) else {
        return;
    };
    if focus.display_row != display_row
        || focus.byte_offset < byte_range.start
        || focus.byte_offset > byte_range.end
    {
        return;
    }
    let relative_byte_offset = focus.byte_offset.saturating_sub(byte_range.start);
    let x = text_left
        + character_column_at_byte(&text[byte_range.clone()], relative_byte_offset) as f32
            * character_width;
    let cursor_rect = egui::Rect::from_min_max(
        egui::pos2(x, response_rect.top()),
        egui::pos2(x, response_rect.top() + row_height),
    );
    ui.painter().line_segment(
        [cursor_rect.center_top(), cursor_rect.center_bottom()],
        egui::Stroke::new(2.0, egui::Color32::WHITE),
    );
}

pub(super) fn rendered_log_text_left(
    response_rect: egui::Rect,
    byte_range: &std::ops::Range<usize>,
    text_len: usize,
    prefix_width: f32,
) -> f32 {
    if *byte_range == (0..text_len) {
        response_rect.left() + prefix_width
    } else {
        response_rect.left()
    }
}

pub(super) fn selection_position(
    ctx: &egui::Context,
    display_row: usize,
    text: &str,
    response: &egui::Response,
    text_left: f32,
    text_start_x: f32,
    character_width: f32,
) -> Option<(LogTextPosition, bool)> {
    let clicked = response.clicked();
    let pointer = response
        .interact_pointer_pos()
        .or_else(|| {
            ctx.pointer_interact_pos().map(|pointer| {
                ctx.layer_transform_from_global(response.layer_id)
                    .map_or(pointer, |from_global| from_global * pointer)
            })
        })
        .or_else(|| clicked.then_some(response.rect.center()))?;
    let byte_offset =
        byte_offset_at_response_x(text, pointer.x, text_left, text_start_x, character_width);
    let position = LogTextPosition {
        display_row,
        byte_offset,
    };
    let (primary_pressed, primary_down, primary_released) = ctx.input(|input| {
        (
            input.pointer.primary_pressed(),
            input.pointer.primary_down(),
            input.pointer.primary_released(),
        )
    });
    if response.contains_pointer() && primary_pressed {
        Some((position, true))
    } else if clicked {
        // Accessibility activation has no pointer position, so it uses the
        // row centre above as a stable caret location.
        Some((position, true))
    } else if response.rect.contains(pointer) && (primary_down || primary_released) {
        Some((position, false))
    } else {
        None
    }
}

pub(super) fn byte_offset_at_x(text: &str, x: f32, character_width: f32) -> usize {
    let column = (x.max(0.0) / character_width).round() as usize;
    if text.is_ascii() {
        return column.min(text.len());
    }
    text.char_indices()
        .nth(column)
        .map_or(text.len(), |(byte_offset, _)| byte_offset)
}
