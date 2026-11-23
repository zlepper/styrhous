use super::*;
use components::tooltip::TooltipExt;

pub(super) fn show_log_window_with_scroll_state(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &mut LogDisplayOptions,
    log_store: &LogStoreService,
    _close_requested: &mut bool,
) -> egui::scroll_area::ScrollAreaOutput<()> {
    let ctx = ui.ctx().clone();
    let theme = ui.tokens();
    sync_search(&ctx, window, log_store);
    if let Some(text) = window.copied_text.take() {
        ctx.copy_text(text);
    }
    request_copy(&ctx, window, log_store);
    // The status bar shows the counts the search field shows this frame,
    // before the toolbar starts a pending search.
    let counts = LogCounts::of(window);
    egui::Panel::top("pod-log-header")
        .exact_size(size::HEADER_BAR + size::TOOLBAR)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            show_log_header(ui, window, display_options);
            show_log_search_controls(&ctx, ui, window, display_options, log_store);
        });
    egui::Panel::bottom("pod-log-status")
        .exact_size(size::STATUS_BAR)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            show_log_status_bar(ui, window.id, counts, display_options);
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme.code.surface)
                .inner_margin(egui::Margin::same(space::S16 as i8)),
        )
        .show(ui, |ui| {
            // Egui also applies `bar_outer_margin` while it paints floating
            // tracks. Moving those tracks keeps the existing content bounds,
            // virtual-scroll viewport, and panel gutter intact.
            ui.spacing_mut().scroll.bar_outer_margin = -space::S16;
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
            let tail_scroll_row = window
                .tail
                .scroll_requested()
                .then_some(display_count.saturating_sub(1));
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
                .or_else(|| {
                    tail_scroll_row.map(|row| egui::vec2(horizontal_offset, row as f32 * row_step))
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
                .stick_to_bottom(
                    window.tail.is_following() && requested_offset.is_none() && !caret_has_focus,
                );
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
                        let source_prefix = window.source_prefix(row.source.as_deref());
                        let visible_source_prefix =
                            (!source_prefix.is_empty()).then_some(source_prefix.as_str());
                        let prefix = log_line_prefix(
                            row.line_index,
                            row.timestamp.as_deref(),
                            visible_source_prefix,
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
                                            visible_source_prefix,
                                            &row.text,
                                            &row.style_spans,
                                            &highlight_ranges,
                                            *display_options,
                                            &theme,
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
                                                .color(theme.code.gutter),
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
                                            &theme,
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
                        if window.tail.release() {
                            ctx.request_repaint();
                        }
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
            update_tail_follow_state(&ctx, window, &output, requested_offset.is_none());
            output
        })
        .inner
}

pub(super) fn update_tail_follow_state(
    ctx: &egui::Context,
    window: &mut PodLogWindowState,
    output: &egui::scroll_area::ScrollAreaOutput<()>,
    may_attach_to_near_tail: bool,
) {
    let maximum_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
    if window.tail.observe_scroll(
        maximum_offset,
        output.state.offset.y,
        may_attach_to_near_tail,
        TAIL_BOTTOM_TOLERANCE_POINTS,
    ) {
        ctx.request_repaint();
    }
}

fn show_source_issue(
    ui: &mut egui::Ui,
    issues: &std::collections::HashMap<crate::worker::PodLogStreamTarget, String>,
    state: &str,
) {
    let theme = ui.tokens();
    if issues.is_empty() {
        return;
    }
    let text = format!(
        "{} source{} {state}",
        issues.len(),
        if issues.len() == 1 { "" } else { "s" }
    );
    type_scale::META
        .label(ui, &text, theme.warning.text)
        .tooltip(source_issue_details(issues));
}

fn source_issue_details(
    issues: &std::collections::HashMap<crate::worker::PodLogStreamTarget, String>,
) -> String {
    let mut details = issues
        .iter()
        .map(|(target, error)| format!("{}: {error}", target.display_name()))
        .collect::<Vec<_>>();
    details.sort();
    details.join("\n")
}

/// The window header: the source, its live state, the instance and export.
fn show_log_header(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &LogDisplayOptions,
) {
    let theme = ui.tokens();
    let (title, meta) = if window.has_multiple_sources() {
        (
            format!("{} sources", window.targets.len()),
            "Interleaved Pod logs".to_owned(),
        )
    } else {
        let target = window
            .targets
            .first()
            .expect("log windows always contain one source");
        (
            target.pod_name.clone(),
            format!("Pod logs · container {}", target.container),
        )
    };
    let label = status_label(window);
    let headline = status_headline(&label).to_owned();
    let all_sources_reconnecting = all_sources_are_reconnecting(window);
    let header = WindowHeader::new(("pod-log", window.id), icons::Icon::DocumentText, &title)
        .meta(&meta)
        .status(&headline, status_tone(window))
        .live_status()
        .show(ui, |ui| {
            if !all_sources_reconnecting {
                show_source_issue(ui, &window.source_reconnects, "reconnecting");
            }
            show_source_issue(ui, &window.source_failures, "unavailable");
            type_scale::META.label(ui, "Container start", theme.text.muted);
            let mut instance = window.instance;
            // Switching replaces the window's store, which an export in
            // progress still needs.
            if Segmented::new(&mut instance)
                .option(PodLogInstance::Current, "Current")
                .option(PodLogInstance::Previous, "Previous")
                .label("Log instance")
                .enabled(!window.export.is_busy())
                .show(ui)
                .changed()
            {
                window.requested_instance = Some(instance);
            }
            show_export_controls(ui, window, display_options);
        });
    if let Some(status) = header.status {
        if all_sources_reconnecting {
            status.tooltip(source_issue_details(&window.source_reconnects));
        } else if headline != label {
            status.tooltip(label.as_str());
        }
    }
}

/// The status bar: how many lines the surface holds and matches, and the
/// time zone of the timestamps.
fn show_log_status_bar(
    ui: &mut egui::Ui,
    window_id: u64,
    counts: LogCounts,
    display_options: &LogDisplayOptions,
) {
    StatusBar::new(("pod-log", window_id)).show(
        ui,
        |ui| {
            status_number(ui, &line_count_text(counts.lines, LINES));
            if let Some(matches) = counts.matches {
                status_number(ui, &line_count_text(matches, MATCHES));
            }
        },
        |ui| {
            if display_options.show_timestamps {
                status_text(ui, "UTC");
            }
        },
    );
}

/// What the status bar counts.
#[derive(Clone, Copy)]
struct LogCounts {
    lines: usize,
    /// Matches of the search, while there is one.
    matches: Option<usize>,
}

impl LogCounts {
    fn of(window: &PodLogWindowState) -> Self {
        let search = &window.search;
        Self {
            lines: window.total_lines,
            matches: (!search.query.is_empty() && search.error.is_none())
                .then_some(search.match_count),
        }
    }
}

/// The nouns of the status bar counts.
const LINES: [&str; 2] = ["line", "lines"];
const MATCHES: [&str; 2] = ["match", "matches"];

/// "1,284 lines", "1 match", "12.4k lines" (counts formatted like the
/// search field's).
pub(super) fn line_count_text(count: usize, [singular, plural]: [&str; 2]) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{} {noun}", components::search::format_count(count))
}

#[cfg(test)]
mod line_count_tests {
    use super::{LINES, MATCHES, line_count_text};

    #[test]
    fn counts_group_thousands_and_agree_in_number() {
        assert_eq!(line_count_text(0, LINES), "0 lines");
        assert_eq!(line_count_text(1, LINES), "1 line");
        assert_eq!(line_count_text(1_284, LINES), "1,284 lines");
        assert_eq!(line_count_text(12_400, LINES), "12.4k lines");
        assert_eq!(line_count_text(1, MATCHES), "1 match");
        assert_eq!(line_count_text(12, MATCHES), "12 matches");
    }
}
