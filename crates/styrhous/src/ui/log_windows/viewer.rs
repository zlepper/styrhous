use super::*;

pub(super) fn show_log_window_with_scroll_state(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &mut LogDisplayOptions,
    log_store: &LogStoreService,
    _close_requested: &mut bool,
) -> egui::scroll_area::ScrollAreaOutput<()> {
    let ctx = ui.ctx().clone();
    sync_search(&ctx, window, log_store);
    if let Some(text) = window.copied_text.take() {
        ctx.copy_text(text);
    }
    request_copy(&ctx, window, log_store);
    egui::Panel::top("pod-log-header")
        .exact_size(52.0)
        .frame(
            egui::Frame::new()
                .fill(TOOLBAR_BACKGROUND)
                .stroke(egui::Stroke::new(1.0, TABLE_BORDER))
                .inner_margin(egui::Margin::symmetric(
                    spacing::XL as i8,
                    spacing::SM as i8,
                )),
        )
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Logs")
                        .font(typography::section_heading())
                        .color(gray::_900),
                );
                ui.add_space(spacing::LG);
                ui.label(
                    egui::RichText::new(&window.pod_name)
                        .font(typography::section_heading())
                        .color(gray::_900),
                );
                ui.add_space(spacing::MD);
                ui.label(
                    egui::RichText::new(format!("Container: {}", window.container.name))
                        .font(typography::body())
                        .color(gray::_600),
                );
                ui.add_space(spacing::MD);
                ui.label(
                    egui::RichText::new("●")
                        .font(typography::body())
                        .color(status_color(&window.status)),
                );
                ui.label(
                    egui::RichText::new(status_label(window))
                        .font(typography::body())
                        .color(gray::_600),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    show_log_search_controls(&ctx, ui, window, display_options, log_store)
                });
            });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(surface::TERMINAL_BACKGROUND)
                .inner_margin(egui::Margin::same(spacing::LG as i8)),
        )
        .show(ui, |ui| {
            let pixels_per_point = ui.pixels_per_point();
            let font_row_height =
                ui.fonts_mut(|fonts| fonts.row_height(&egui::FontId::monospace(LOG_FONT_SIZE)));
            // Keep virtual row origins on the physical-pixel grid. Otherwise
            // equivalent rows at different logical indices can round to
            // different raster positions during a source rebase.
            let row_step = ((font_row_height + ui.spacing().item_spacing.y) * pixels_per_point)
                .round()
                / pixels_per_point;
            let row_height = row_step - ui.spacing().item_spacing.y;
            let scroll_area_salt = egui::Id::new(("pod-log-lines", window.id));
            if initial_spool_is_pending(window) {
                request_page_for_display_row(
                    window,
                    log_store,
                    window.total_lines.saturating_sub(1),
                );
                show_initial_spool_state(ui, window.total_lines);
                return components::scroll::both()
                    .id_salt(scroll_area_salt)
                    .show_rows(ui, row_height, 0, |_ui, _rows| {});
            }

            let display_count = displayed_line_count(window);
            // `ScrollArea::id_salt` hashes the salt into an `IdSalt` before it
            // scopes it to this UI. Mirror both steps when reading its state.
            let scroll_id = ui.make_persistent_id(egui::IdSalt::new(scroll_area_salt));
            let scroll_state = egui::scroll_area::State::load(&ctx, scroll_id);
            let horizontal_offset = scroll_state.as_ref().map_or(0.0, |state| state.offset.x);
            // The virtual fragment renderer rounds its content origin to the
            // physical pixel grid. Store that same normalized value so a
            // rebase cannot turn a fractional wheel offset into a one-pixel
            // horizontal text jump on its next frame.
            let horizontal_offset =
                (horizontal_offset * pixels_per_point).round() / pixels_per_point;
            let vertical_offset = scroll_state.as_ref().map_or(0.0, |state| state.offset.y);
            window.visible_top_display_row = (vertical_offset / row_step).max(0.0).floor() as usize;
            let viewport_width = ui.available_width();
            let character_width = ui
                .fonts_mut(|fonts| fonts.glyph_width(&egui::FontId::monospace(LOG_FONT_SIZE), '0'));
            let caret_focus_id = egui::Id::new(("pod-log-caret", window.id));
            // Register a real focusable egui node for the virtual text canvas.
            // Individual rows request this ID on click, but cannot own it because
            // their set changes as pages are virtualized in and out of view.
            let caret_focus_response = ui.interact(
                ui.available_rect_before_wrap(),
                caret_focus_id,
                egui::Sense::focusable_noninteractive(),
            );
            ui.ctx()
                .accesskit_node_builder(caret_focus_response.id, |builder| {
                    builder.set_label("Pod log text");
                });
            resolve_pending_caret(window, log_store, displayed_line_count(window), &ctx);
            let caret_has_focus = ctx.memory(|memory| memory.has_focus(caret_focus_id));
            if caret_has_focus {
                ctx.memory_mut(|memory| {
                    memory.set_focus_lock_filter(
                        caret_focus_id,
                        egui::EventFilter {
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            ..Default::default()
                        },
                    );
                });
                handle_log_keyboard(
                    &ctx,
                    window,
                    log_store,
                    displayed_line_count(window),
                    (ui.available_height() / row_step).floor().max(1.0) as usize,
                );
            }
            let mut caret_scroll_offset = None;
            let horizontal_offset = if window.ensure_caret_visible {
                if let Some(offset) = caret_horizontal_offset(
                    window,
                    *display_options,
                    horizontal_offset,
                    viewport_width,
                    character_width,
                ) {
                    let vertical = caret_vertical_offset(
                        window,
                        vertical_offset,
                        ui.available_height(),
                        row_step,
                    );
                    if (offset - horizontal_offset).abs() > f32::EPSILON
                        || (vertical - vertical_offset).abs() > f32::EPSILON
                    {
                        caret_scroll_offset = Some(egui::vec2(offset, vertical));
                    }
                    window.ensure_caret_visible = false;
                    offset
                } else {
                    horizontal_offset
                }
            } else {
                horizontal_offset
            };
            // Keep a navigation target alive until the virtual scroll area has
            // actually brought that row into view. A one-frame request can be
            // clamped while the area is still measuring or loading its target
            // page, which made the toolbar arrows appear to do nothing.
            let requested_scroll_row = window.search.scroll_to_display_row;
            let requested_offset = requested_scroll_row
                .map(|row| {
                    let requested_vertical_offset = window
                        .search
                        .rebase_scroll_row_delta
                        .take()
                        .map_or(row as f32 * row_step, |delta| {
                            vertical_offset + delta as f32 * row_step
                        });
                    egui::vec2(horizontal_offset, requested_vertical_offset)
                })
                .or(caret_scroll_offset);
            let mut scroll_area = components::scroll::both()
                .id_salt(scroll_area_salt)
                .auto_shrink([false, false])
                // A wide page can arrive after placeholders have already been
                // rendered. Reserving both bars from the first frame keeps
                // that discovery from changing the vertical viewport.
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .horizontal_scroll_offset(horizontal_offset)
                // A focused text caret is an explicit request to inspect the
                // current records. Do not let new tail records carry it out
                // of view between keyboard frames.
                .stick_to_bottom(requested_offset.is_none() && !caret_has_focus);
            if let Some(offset) = requested_offset {
                scroll_area = scroll_area.scroll_offset(offset);
            }
            let output = scroll_area.show_rows(ui, row_height, display_count, |ui, rows| {
                ui.set_min_width(window.horizontal_content_width);
                for display_row in rows {
                    let mut selection_update = None;
                    let mut caret_paint = None;
                    let mut row_content_width = None;
                    request_page_for_display_row(window, log_store, display_row);
                    let page_start = display_row / window.page_size * window.page_size;
                    let key = LogPageKey {
                        generation: window.search.generation,
                        filter_matches: filter_is_active(window),
                        page_start,
                    };
                    let row_offset = display_row - page_start;
                    let cached_row = window
                        .pages
                        .get(&key)
                        .and_then(|page| {
                            page.rows
                                .get(row_offset)
                                .map(|row| (row, page.max_text_columns))
                        })
                        .or_else(|| {
                            if !filter_is_active(window) {
                                window
                                    .live_rows
                                    .get(&display_row)
                                    .map(|row| (row, row.text.chars().count()))
                            } else {
                                None
                            }
                        });
                    if let Some((row, max_text_columns)) = cached_row {
                        let prefix = log_line_prefix(
                            row.line_index,
                            row.timestamp.as_deref(),
                            *display_options,
                        );
                        let prefix_width = prefix.chars().count() as f32 * character_width;
                        let fragment = visible_text_fragment(
                            &row.text,
                            (horizontal_offset - prefix_width).max(0.0),
                            viewport_width,
                            character_width,
                        );
                        row_content_width =
                            Some(prefix_width + max_text_columns as f32 * character_width);
                        let byte_range = fragment.byte_range.clone();
                        let selection_range = window.selection.and_then(|selection| {
                            selection.range_for_row(display_row, row.text.len())
                        });
                        let (response, interaction_response) = if byte_range == (0..row.text.len())
                        {
                            let mut highlight_ranges = row.match_ranges.clone();
                            if let Some(range) = selection_range {
                                highlight_ranges.push(range);
                            }
                            let row_response = ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(log_line_layout_job(
                                            row.line_index,
                                            row.timestamp.as_deref(),
                                            &row.text,
                                            &row.style_spans,
                                            &highlight_ranges,
                                            *display_options,
                                        ))
                                        .extend()
                                        .selectable(false)
                                        .sense(egui::Sense::hover()),
                                    )
                                },
                            );
                            let interaction_response = ui
                                .interact(
                                    row_response.response.rect,
                                    egui::Id::new(("pod-log-line", window.id, display_row)),
                                    egui::Sense::click_and_drag(),
                                )
                                .on_hover_cursor(egui::CursorIcon::Text);
                            (row_response.inner, interaction_response)
                        } else {
                            let row_response = ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    if !prefix.is_empty() {
                                        ui.add(egui::Label::new(
                                            egui::RichText::new(prefix)
                                                .font(egui::FontId::monospace(LOG_FONT_SIZE))
                                                .color(egui::Color32::from_rgb(156, 163, 175)),
                                        ));
                                    }
                                    ui.add_space(fragment.start_x);
                                    let mut highlight_ranges =
                                        clipped_ranges(&row.match_ranges, byte_range.clone());
                                    if let Some(range) = selection_range {
                                        highlight_ranges
                                            .extend(clipped_ranges(&[range], byte_range.clone()));
                                    }
                                    ui.add(
                                        egui::Label::new(log_line_text_layout_job(
                                            &row.text[byte_range.clone()],
                                            clipped_style_spans(
                                                &row.style_spans,
                                                byte_range.clone(),
                                            ),
                                            highlight_ranges,
                                            *display_options,
                                        ))
                                        .extend()
                                        .selectable(false)
                                        .sense(egui::Sense::hover()),
                                    )
                                },
                            );
                            let interaction_response = ui
                                .interact(
                                    row_response.response.rect,
                                    egui::Id::new(("pod-log-line", window.id, display_row)),
                                    egui::Sense::click_and_drag(),
                                )
                                .on_hover_cursor(egui::CursorIcon::Text);
                            (row_response.inner, interaction_response)
                        };
                        ui.ctx()
                            .accesskit_node_builder(interaction_response.id, |builder| {
                                builder.set_label(format!("Pod log line {}", display_row + 1));
                            });
                        let text_left = rendered_log_text_left(
                            response.rect,
                            &byte_range,
                            row.text.len(),
                            prefix_width,
                        );
                        let text_start_x = if byte_range != (0..row.text.len()) {
                            fragment.start_x
                        } else {
                            0.0
                        };
                        selection_update = selection_position(
                            &ctx,
                            display_row,
                            &row.text,
                            &interaction_response,
                            text_left,
                            text_start_x,
                            character_width,
                        );
                        if selection_update.is_some()
                            || window
                                .selection
                                .is_some_and(|selection| selection.focus.display_row == display_row)
                        {
                            caret_paint = Some((
                                row.text.clone(),
                                byte_range.clone(),
                                response.rect,
                                text_left,
                            ));
                        }
                    } else {
                        show_loading_row(
                            ui,
                            row_height,
                            display_row,
                            !filter_is_active(window),
                            *display_options,
                        );
                    }
                    if let Some(row_content_width) = row_content_width {
                        window.horizontal_content_width =
                            window.horizontal_content_width.max(row_content_width);
                    }
                    if let Some((position, starts_selection)) = selection_update {
                        if starts_selection {
                            window.set_selection(Some(LogTextSelection {
                                anchor: position,
                                focus: position,
                            }));
                            ctx.memory_mut(|memory| memory.request_focus(caret_focus_id));
                        } else if let Some(selection) = window.selection {
                            window.set_selection(Some(LogTextSelection {
                                anchor: selection.anchor,
                                focus: position,
                            }));
                        }
                        window.caret_preferred_column =
                            Some(caret_paint.as_ref().map_or(0, |(text, _, _, _)| {
                                character_column_at_byte(text, position.byte_offset)
                            }));
                        window.ensure_caret_visible = false;
                    }
                    if let Some((text, byte_range, response_rect, text_left)) = caret_paint {
                        paint_log_caret(
                            ui,
                            &ctx,
                            window,
                            caret_focus_id,
                            display_row,
                            &text,
                            &byte_range,
                            response_rect,
                            text_left,
                            character_width,
                            row_height,
                        );
                    }
                }
            });
            window.following_bottom = output.state.offset.y + output.inner_rect.height()
                >= output.content_size.y - row_step;
            if let Some(row) = requested_scroll_row {
                if display_row_is_visible(row, row_step, &output) {
                    window.search.scroll_to_display_row = None;
                } else {
                    // Keep repainting until egui has accepted the requested
                    // offset. This also covers the frame that replaces a
                    // virtual placeholder with the requested disk page.
                    ctx.request_repaint();
                }
            }
            output
        })
        .inner
}
