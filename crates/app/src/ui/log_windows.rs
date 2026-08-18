use super::state::{
    LogDisplayOptions, LogPageKey, LogTextPosition, LogTextSelection, PendingLogCaret,
    PodLogStatus, PodLogWindowState, UiState,
};
use crate::ansi::AnsiStyleSpan;
use crate::log_store::LogStoreService;
use crate::worker::{
    PodLogStreamEnded, PodLogStreamFailed, PodLogStreamStarted, StopPodLogStream, WorkerCommandBox,
    WorkerResult,
};
use anstyle::{Ansi256Color, AnsiColor, Color, Effects, RgbColor, Style};
use components::colors::{SUCCESS, TABLE_BORDER, TOOLBAR_BACKGROUND, gray};
use components::design::{radius, search, spacing, status, surface, typography};
use components::{PointingHand, TailwindSearchInput, icons, search_navigation_button};
use std::time::{Duration, Instant};

const LOG_FONT_SIZE: f32 = 14.0;
const HORIZONTAL_OVERSCAN_POINTS: f32 = 120.0;

impl WorkerResult for PodLogStreamStarted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id)
            && matches!(window.status, PodLogStatus::Connecting)
        {
            window.status = PodLogStatus::Following;
        }
    }
}

impl WorkerResult for PodLogStreamEnded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id)
            && !matches!(window.status, PodLogStatus::Failed(_))
        {
            window.status = PodLogStatus::Finished;
        }
    }
}

impl WorkerResult for PodLogStreamFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id) {
            window.status = PodLogStatus::Failed(self.error);
        }
    }
}

/// Render native, independent Pod log windows and stop both the Kubernetes
/// stream and the independent disk store when a window is closed.
pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    log_store: &LogStoreService,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let ids = ui_state.log_windows.keys().copied().collect::<Vec<_>>();
    for id in ids {
        let (log_windows, display_options) =
            (&mut ui_state.log_windows, &mut ui_state.log_display_options);
        let Some(window) = log_windows.get_mut(&id) else {
            continue;
        };
        if !window.store_opened {
            window.store_opened = log_store.open(id);
        }
        let viewport_id = egui::ViewportId::from_hash_of(("pod-log-window", id));
        let title = format!(
            "Logs · {}/{} · {}",
            window.namespace, window.pod_name, window.container.name
        );
        let mut close_requested = false;
        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(crate::DEFAULT_NATIVE_WINDOW_SIZE)
                .with_min_inner_size(crate::MIN_NATIVE_WINDOW_SIZE),
            |window_ui, _| {
                let window_ctx = window_ui.ctx().clone();
                close_requested = window_ctx.input(|input| input.viewport().close_requested());
                show_log_window(
                    window_ui,
                    window,
                    display_options,
                    log_store,
                    &mut close_requested,
                );
            },
        );
        window.close_requested |= close_requested;
    }

    let closed = ui_state
        .log_windows
        .values()
        .filter(|window| window.close_requested)
        .map(|window| (window.id, window.cluster_key))
        .collect::<Vec<_>>();
    for (id, cluster_key) in closed {
        ui_state.log_windows.remove(&id);
        log_store.close(id);
        commands_to_send.push(Box::new(StopPodLogStream {
            cluster_key,
            log_window_id: id,
        }));
    }
}

pub(super) fn show_log_window(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &mut LogDisplayOptions,
    log_store: &LogStoreService,
    _close_requested: &mut bool,
) {
    let _ =
        show_log_window_with_scroll_state(ui, window, display_options, log_store, _close_requested);
}

fn show_log_window_with_scroll_state(
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
    request_copy(&ctx, window, display_options, log_store);
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
                        let (text_left, text_start_x) = if byte_range == (0..row.text.len()) {
                            (response.rect.left() + prefix_width, 0.0)
                        } else {
                            (response.rect.left(), fragment.start_x)
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
                                prefix_width,
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
                    if let Some((text, byte_range, response_rect, prefix_width)) = caret_paint {
                        paint_log_caret(
                            ui,
                            &ctx,
                            window,
                            caret_focus_id,
                            display_row,
                            &text,
                            &byte_range,
                            response_rect,
                            prefix_width,
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

fn display_row_is_visible(
    display_row: usize,
    row_step: f32,
    output: &egui::scroll_area::ScrollAreaOutput<()>,
) -> bool {
    let row_top = display_row as f32 * row_step;
    let row_bottom = row_top + row_step;
    row_bottom > output.state.offset.y
        && row_top < output.state.offset.y + output.inner_rect.height()
}

fn initial_spool_is_pending(window: &PodLogWindowState) -> bool {
    window.total_lines > 0
        && !window.initial_page_loaded
        && !filter_is_active(window)
        && !matches!(window.status, PodLogStatus::Failed(_))
}

fn show_initial_spool_state(ui: &mut egui::Ui, total_lines: usize) {
    let state_rect = egui::Rect::from_center_size(
        ui.available_rect_before_wrap().center(),
        egui::vec2(280.0, 64.0),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(state_rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
        |ui| {
            ui.horizontal_centered(|ui| {
                ui.add(egui::Spinner::new().size(20.0).color(gray::_200));
                ui.add_space(spacing::SM);
                ui.label(
                    egui::RichText::new("Spooling logs…")
                        .font(typography::section_heading())
                        .color(gray::_200),
                );
            });
            ui.add_space(spacing::SM);
            ui.label(
                egui::RichText::new(format!(
                    "{total_lines} {} received",
                    if total_lines == 1 { "line" } else { "lines" }
                ))
                .font(typography::body())
                .color(gray::_500),
            );
        },
    );
}

fn request_page_for_display_row(
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    display_row: usize,
) {
    let page_start = display_row / window.page_size * window.page_size;
    let key = LogPageKey {
        generation: window.search.generation,
        filter_matches: filter_is_active(window),
        page_start,
    };
    if !window.pages.contains_key(&key)
        && window.pending_pages.insert(key)
        && !log_store.load_page(
            window.id,
            key.generation,
            key.filter_matches,
            key.page_start,
        )
    {
        window.pending_pages.remove(&key);
    }
}

fn show_log_search_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &mut LogDisplayOptions,
    log_store: &LogStoreService,
) {
    let invalid = window.search.error.is_some();
    let focus_search =
        ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F));
    let search_response = ui
        .allocate_ui_with_layout(
            egui::vec2(212.0, 36.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                TailwindSearchInput::new(&mut window.search.query, &mut window.search.regex_mode)
                    .hint_text("Search logs...")
                    .id_salt(("pod-log-search", window.id))
                    .accessibility_label("Search logs")
                    .invalid(invalid)
                    .focus(focus_search)
                    .show(ui)
            },
        )
        .inner;
    if search_response.text.changed() || search_response.regex.changed() {
        window.search.generation += 1;
        window.search.match_count = 0;
        window.search.scanned_lines = 0;
        window.search.search_complete = window.search.query.is_empty();
        window.search.error = None;
        window.search.active_display_row = None;
        window.search.active_match = None;
        window.search.scroll_to_display_row = None;
        window.clear_pages();
        window.search.search_deadline = Some(Instant::now() + Duration::from_millis(150));
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    ui.add_space(spacing::SM);
    let navigation = ui
        .allocate_ui_with_layout(
            egui::vec2(158.0, 34.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .stroke(egui::Stroke::new(1.0, gray::_300))
                    .corner_radius(radius::control())
                    .inner_margin(egui::Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let previous_line = search_navigation_button(
                                ui,
                                icons::arrow_up_icon(),
                                "Previous displayed line",
                            );
                            ui.separator();
                            let previous_match = search_navigation_button(
                                ui,
                                icons::arrow_left_icon(),
                                "Previous matching line",
                            );
                            ui.separator();
                            let next_match = search_navigation_button(
                                ui,
                                icons::arrow_right_icon(),
                                "Next matching line",
                            );
                            ui.separator();
                            let next_line = search_navigation_button(
                                ui,
                                icons::arrow_down_icon(),
                                "Next displayed line",
                            );
                            ui.separator();
                            let filter = filter_button(ui, filter_is_active(window));
                            (previous_line, previous_match, next_match, next_line, filter)
                        })
                        .inner
                    })
                    .inner
            },
        )
        .inner;
    ui.add_space(spacing::SM);
    let display_controls = ui
        .allocate_ui_with_layout(
            egui::vec2(96.0, 34.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .stroke(egui::Stroke::new(1.0, gray::_300))
                    .corner_radius(radius::control())
                    .inner_margin(egui::Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let line_numbers = log_display_toggle_button(
                                ui,
                                icons::numbered_list_icon(),
                                display_options.show_line_numbers,
                                "Show log line numbers",
                            );
                            ui.separator();
                            let timestamps = log_display_toggle_button(
                                ui,
                                icons::calendar_days_icon(),
                                display_options.show_timestamps,
                                "Show Kubernetes log timestamps",
                            );
                            ui.separator();
                            let ansi = log_display_toggle_button(
                                ui,
                                icons::swatch_icon(),
                                display_options.render_ansi,
                                "Render ANSI styling",
                            );
                            (line_numbers, timestamps, ansi)
                        })
                        .inner
                    })
                    .inner
            },
        )
        .inner;
    if navigation.1 {
        advance_log_match(window, log_store, false);
    }
    if navigation.2 {
        advance_log_match(window, log_store, true);
    }
    if navigation.0 {
        advance_log_line(window, false);
    }
    if navigation.3 {
        advance_log_line(window, true);
    }
    if navigation.4 {
        window.search.filter_matches = !window.search.filter_matches;
        window.search.active_display_row = None;
        window.search.scroll_to_display_row = None;
        window.clear_pages();
    }
    if display_controls.0 {
        display_options.show_line_numbers = !display_options.show_line_numbers;
    }
    if display_controls.1 {
        display_options.show_timestamps = !display_options.show_timestamps;
    }
    if display_controls.2 {
        display_options.render_ansi = !display_options.render_ansi;
    }
    sync_search(ctx, window, log_store);
}

fn sync_search(ctx: &egui::Context, window: &mut PodLogWindowState, log_store: &LogStoreService) {
    let Some(deadline) = window.search.search_deadline else {
        return;
    };
    if Instant::now() < deadline {
        ctx.request_repaint_after(deadline.saturating_duration_since(Instant::now()));
        return;
    }
    window.search.search_deadline = None;
    if window.search.query.is_empty() {
        window.search.search_complete = true;
        return;
    }
    let pattern = if window.search.regex_mode {
        window.search.query.clone()
    } else {
        regex::escape(&window.search.query)
    };
    if let Err(error) = regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
    {
        window.search.error = Some(error.to_string());
        window.search.search_complete = true;
        return;
    }
    window.search.search_complete = false;
    if !log_store.search(
        window.id,
        window.search.generation,
        window.search.query.clone(),
        window.search.regex_mode,
    ) {
        window.search.search_deadline = Some(Instant::now() + Duration::from_millis(100));
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn filter_button(ui: &mut egui::Ui, active: bool) -> bool {
    log_display_toggle_button(ui, icons::funnel_icon(), active, "Filter to matching lines")
}

fn log_display_toggle_button(
    ui: &mut egui::Ui,
    icon: egui::Image<'static>,
    active: bool,
    label: &str,
) -> bool {
    let response = ui
        .add_sized(
            egui::Vec2::splat(28.0),
            egui::Button::image(
                icon.fit_to_exact_size(egui::Vec2::splat(14.0))
                    .tint(gray::_700),
            )
            .frame(false),
        )
        .with_pointing_hand();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Checkbox, ui.is_enabled(), label)
    });
    if active {
        let center = response.rect.right_bottom() - egui::vec2(3.5, 3.5);
        ui.painter()
            .circle_filled(center, 5.5, components::colors::SUCCESS);
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            "✓",
            egui::FontId::proportional(8.0),
            egui::Color32::WHITE,
        );
    }
    response.on_hover_text(label).clicked()
}

fn filter_is_active(window: &PodLogWindowState) -> bool {
    window.search.filter_matches && !window.search.query.is_empty()
}

fn displayed_line_count(window: &PodLogWindowState) -> usize {
    if filter_is_active(window) {
        window.search.match_count
    } else {
        window.total_lines
    }
}

fn request_copy(
    ctx: &egui::Context,
    window: &PodLogWindowState,
    display_options: &LogDisplayOptions,
    log_store: &LogStoreService,
) {
    let copy_requested = ctx.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Copy))
    });
    let Some(selection) = copy_requested.then_some(window.selection).flatten() else {
        return;
    };
    let (start, end) = selection.normalized();
    if start == end {
        return;
    }
    let _ = log_store.copy(
        window.id,
        window.selection_generation,
        window.search.generation,
        filter_is_active(window),
        start.display_row,
        start.byte_offset,
        end.display_row,
        end.byte_offset,
        display_options.show_line_numbers,
        display_options.show_timestamps,
    );
}

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

fn resolve_pending_caret(
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

fn handle_log_keyboard(
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

fn move_log_caret(
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

fn set_caret_target(
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

fn selection_is_empty(selection: LogTextSelection) -> bool {
    selection.anchor == selection.focus
}

fn character_column_at_byte(text: &str, byte_offset: usize) -> usize {
    egui::text_selection::text_cursor_state::char_index_from_byte_index(
        text,
        egui::text::ByteIndex(byte_offset),
    )
    .into()
}

fn byte_offset_at_character_column(text: &str, character_column: usize) -> usize {
    egui::text_selection::text_cursor_state::byte_index_from_char_index(
        text,
        egui::text::CharIndex(character_column),
    )
    .into()
}

fn caret_horizontal_offset(
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

fn caret_vertical_offset(
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
fn paint_log_caret(
    ui: &egui::Ui,
    ctx: &egui::Context,
    window: &PodLogWindowState,
    focus_id: egui::Id,
    display_row: usize,
    text: &str,
    byte_range: &std::ops::Range<usize>,
    response_rect: egui::Rect,
    prefix_width: f32,
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
    let text_x = if byte_range.start == 0 {
        response_rect.left() + prefix_width
    } else {
        response_rect.left()
    };
    let x = text_x
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

fn selection_position(
    ctx: &egui::Context,
    display_row: usize,
    text: &str,
    response: &egui::Response,
    text_left: f32,
    text_start_x: f32,
    character_width: f32,
) -> Option<(LogTextPosition, bool)> {
    let pointer = response.interact_pointer_pos()?;
    let byte_offset =
        byte_offset_at_response_x(text, pointer.x, text_left, text_start_x, character_width);
    let position = LogTextPosition {
        display_row,
        byte_offset,
    };
    if response.drag_started() || response.clicked() {
        Some((position, true))
    } else if response.hovered() && ctx.input(|input| input.pointer.primary_down()) {
        Some((position, false))
    } else {
        None
    }
}

fn byte_offset_at_x(text: &str, x: f32, character_width: f32) -> usize {
    let column = (x.max(0.0) / character_width).round() as usize;
    if text.is_ascii() {
        return column.min(text.len());
    }
    text.char_indices()
        .nth(column)
        .map_or(text.len(), |(byte_offset, _)| byte_offset)
}

fn byte_offset_at_response_x(
    text: &str,
    pointer_x: f32,
    text_left: f32,
    text_start_x: f32,
    character_width: f32,
) -> usize {
    byte_offset_at_x(text, pointer_x - text_left + text_start_x, character_width)
}

#[derive(Debug, Clone)]
struct VisibleTextFragment {
    byte_range: std::ops::Range<usize>,
    start_x: f32,
}

fn visible_text_fragment(
    text: &str,
    horizontal_offset: f32,
    viewport_width: f32,
    character_width: f32,
) -> VisibleTextFragment {
    let overscan_columns = (HORIZONTAL_OVERSCAN_POINTS / character_width).ceil() as usize;
    let first_column = (horizontal_offset / character_width).floor().max(0.0) as usize;
    let visible_columns = (viewport_width / character_width).ceil() as usize;
    let start_column = first_column.saturating_sub(overscan_columns);
    let end_column = first_column
        .saturating_add(visible_columns)
        .saturating_add(overscan_columns);
    let byte_range = character_column_range(text, start_column, end_column);
    VisibleTextFragment {
        byte_range,
        start_x: start_column as f32 * character_width,
    }
}

fn character_column_range(
    text: &str,
    start_column: usize,
    end_column: usize,
) -> std::ops::Range<usize> {
    if text.is_ascii() {
        return start_column.min(text.len())..end_column.min(text.len());
    }

    let start = text
        .char_indices()
        .nth(start_column)
        .map_or(text.len(), |(byte_index, _)| byte_index);
    let end = text
        .char_indices()
        .nth(end_column)
        .map_or(text.len(), |(byte_index, _)| byte_index);
    start..end
}

fn log_line_prefix(
    line_index: usize,
    timestamp: Option<&str>,
    display_options: LogDisplayOptions,
) -> String {
    let mut prefix = String::new();
    if display_options.show_line_numbers {
        prefix.push_str(&format!("{line_index:>6}  "));
    }
    if display_options.show_timestamps
        && let Some(timestamp) = timestamp
    {
        prefix.push_str(timestamp);
        prefix.push_str("  ");
    }
    prefix
}

fn show_loading_row(
    ui: &mut egui::Ui,
    row_height: f32,
    display_row: usize,
    display_row_is_line_index: bool,
    display_options: LogDisplayOptions,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            if display_row_is_line_index && display_options.show_line_numbers {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(log_line_prefix(display_row, None, display_options))
                            .font(egui::FontId::monospace(LOG_FONT_SIZE))
                            .color(egui::Color32::from_rgb(156, 163, 175)),
                    )
                    .selectable(false),
                );
            }
            ui.add(
                egui::Label::new(
                    egui::RichText::new("Loading…")
                        .font(egui::FontId::monospace(LOG_FONT_SIZE))
                        .color(gray::_500),
                )
                .selectable(false),
            );
        },
    );
}

fn clipped_style_spans(
    style_spans: &[AnsiStyleSpan],
    byte_range: std::ops::Range<usize>,
) -> Vec<AnsiStyleSpan> {
    style_spans
        .iter()
        .filter_map(|span| {
            let start = span.range.0.max(byte_range.start);
            let end = span.range.1.min(byte_range.end);
            (start < end).then_some(AnsiStyleSpan {
                range: (start - byte_range.start, end - byte_range.start),
                style: span.style,
            })
        })
        .collect()
}

fn clipped_ranges(
    ranges: &[(usize, usize)],
    byte_range: std::ops::Range<usize>,
) -> Vec<(usize, usize)> {
    ranges
        .iter()
        .filter_map(|&(range_start, range_end)| {
            let start = range_start.max(byte_range.start);
            let end = range_end.min(byte_range.end);
            (start < end).then_some((start - byte_range.start, end - byte_range.start))
        })
        .collect()
}

fn log_line_layout_job(
    line_index: usize,
    timestamp: Option<&str>,
    line: &str,
    style_spans: &[AnsiStyleSpan],
    ranges: &[(usize, usize)],
    display_options: LogDisplayOptions,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob {
        wrap: egui::text::TextWrapping::no_max_width(),
        ..Default::default()
    };
    let number = egui::TextFormat {
        font_id: egui::FontId::monospace(LOG_FONT_SIZE),
        color: egui::Color32::from_rgb(156, 163, 175),
        ..Default::default()
    };
    if display_options.show_line_numbers {
        job.append(&format!("{line_index:>6}  "), 0.0, number.clone());
    }
    if display_options.show_timestamps
        && let Some(timestamp) = timestamp
    {
        job.append(&format!("{timestamp}  "), 0.0, number);
    }
    append_log_line_text(&mut job, line, style_spans, ranges, display_options);
    job
}

fn log_line_text_layout_job(
    line: &str,
    style_spans: Vec<AnsiStyleSpan>,
    ranges: Vec<(usize, usize)>,
    display_options: LogDisplayOptions,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob {
        wrap: egui::text::TextWrapping::no_max_width(),
        ..Default::default()
    };
    append_log_line_text(&mut job, line, &style_spans, &ranges, display_options);
    job
}

fn append_log_line_text(
    job: &mut egui::text::LayoutJob,
    line: &str,
    style_spans: &[AnsiStyleSpan],
    ranges: &[(usize, usize)],
    display_options: LogDisplayOptions,
) {
    let text = egui::TextFormat {
        font_id: egui::FontId::monospace(LOG_FONT_SIZE),
        color: egui::Color32::from_rgb(229, 231, 235),
        ..Default::default()
    };
    let mut boundaries = Vec::with_capacity(2 + style_spans.len() * 2 + ranges.len() * 2);
    boundaries.extend([0, line.len()]);
    boundaries.extend(
        style_spans
            .iter()
            .flat_map(|span| [span.range.0, span.range.1]),
    );
    boundaries.extend(ranges.iter().flat_map(|&(start, end)| [start, end]));
    boundaries.sort_unstable();
    boundaries.dedup();
    for boundary_pair in boundaries.windows(2) {
        let start = boundary_pair[0];
        let end = boundary_pair[1];
        if start == end {
            continue;
        }
        let style = display_options
            .render_ansi
            .then(|| {
                style_spans
                    .iter()
                    .find(|span| span.range.0 <= start && start < span.range.1)
                    .map(|span| span.style)
            })
            .flatten();
        let mut format = style.map_or_else(|| text.clone(), |style| ansi_text_format(style, &text));
        if ranges
            .iter()
            .any(|&(match_start, match_end)| match_start <= start && end <= match_end)
        {
            if style.is_none() {
                format.color = egui::Color32::from_rgb(254, 243, 199);
            }
            format.background = search::MATCH_BACKGROUND;
        }
        job.append(&line[start..end], 0.0, format);
    }
}

fn ansi_text_format(style: Style, default: &egui::TextFormat) -> egui::TextFormat {
    let default_background = egui::Color32::from_rgb(10, 10, 11);
    let mut format = default.clone();
    let foreground = style.get_fg_color().map(ansi_color).unwrap_or(format.color);
    let background = style
        .get_bg_color()
        .map(ansi_color)
        .unwrap_or(default_background);
    if style.get_effects().contains(Effects::INVERT) {
        format.color = background;
        format.background = foreground;
    } else {
        format.color = foreground;
        if style.get_bg_color().is_some() {
            format.background = background;
        }
    }
    let effects = style.get_effects();
    if effects.contains(Effects::DIMMED) {
        format.color = format.color.gamma_multiply(0.65);
    }
    if effects.contains(Effects::HIDDEN) {
        format.color = egui::Color32::TRANSPARENT;
    }
    format.italics = effects.contains(Effects::ITALIC);
    if effects.contains(Effects::UNDERLINE)
        || effects.contains(Effects::DOUBLE_UNDERLINE)
        || effects.contains(Effects::CURLY_UNDERLINE)
        || effects.contains(Effects::DOTTED_UNDERLINE)
        || effects.contains(Effects::DASHED_UNDERLINE)
    {
        format.underline = egui::Stroke::new(
            1.0,
            style
                .get_underline_color()
                .map(ansi_color)
                .unwrap_or(format.color),
        );
    }
    if effects.contains(Effects::STRIKETHROUGH) {
        format.strikethrough = egui::Stroke::new(1.0, format.color);
    }
    format
}

fn ansi_color(color: Color) -> egui::Color32 {
    match color {
        Color::Ansi(color) => ansi_palette_color(color),
        Color::Ansi256(Ansi256Color(index)) => ansi_256_color(index),
        Color::Rgb(RgbColor(red, green, blue)) => egui::Color32::from_rgb(red, green, blue),
    }
}

fn ansi_256_color(index: u8) -> egui::Color32 {
    if index < 16 {
        return ansi_palette_color(ansi_color_from_index(index));
    }
    if index >= 232 {
        let gray = 8 + (index - 232) * 10;
        return egui::Color32::from_gray(gray);
    }
    let color_index = index - 16;
    let component = |value| if value == 0 { 0 } else { 55 + value * 40 };
    egui::Color32::from_rgb(
        component(color_index / 36),
        component((color_index / 6) % 6),
        component(color_index % 6),
    )
}

fn ansi_color_from_index(index: u8) -> AnsiColor {
    match index {
        0 => AnsiColor::Black,
        1 => AnsiColor::Red,
        2 => AnsiColor::Green,
        3 => AnsiColor::Yellow,
        4 => AnsiColor::Blue,
        5 => AnsiColor::Magenta,
        6 => AnsiColor::Cyan,
        7 => AnsiColor::White,
        8 => AnsiColor::BrightBlack,
        9 => AnsiColor::BrightRed,
        10 => AnsiColor::BrightGreen,
        11 => AnsiColor::BrightYellow,
        12 => AnsiColor::BrightBlue,
        13 => AnsiColor::BrightMagenta,
        14 => AnsiColor::BrightCyan,
        15 => AnsiColor::BrightWhite,
        _ => unreachable!("ANSI 16-color palette index must be below 16"),
    }
}

fn ansi_palette_color(color: AnsiColor) -> egui::Color32 {
    match color {
        AnsiColor::Black => egui::Color32::from_rgb(0, 0, 0),
        AnsiColor::Red => egui::Color32::from_rgb(239, 68, 68),
        AnsiColor::Green => egui::Color32::from_rgb(34, 197, 94),
        AnsiColor::Yellow => egui::Color32::from_rgb(234, 179, 8),
        AnsiColor::Blue => egui::Color32::from_rgb(59, 130, 246),
        AnsiColor::Magenta => egui::Color32::from_rgb(217, 70, 239),
        AnsiColor::Cyan => egui::Color32::from_rgb(6, 182, 212),
        AnsiColor::White => egui::Color32::from_rgb(229, 231, 235),
        AnsiColor::BrightBlack => egui::Color32::from_rgb(107, 114, 128),
        AnsiColor::BrightRed => egui::Color32::from_rgb(248, 113, 113),
        AnsiColor::BrightGreen => egui::Color32::from_rgb(74, 222, 128),
        AnsiColor::BrightYellow => egui::Color32::from_rgb(250, 204, 21),
        AnsiColor::BrightBlue => egui::Color32::from_rgb(96, 165, 250),
        AnsiColor::BrightMagenta => egui::Color32::from_rgb(232, 121, 249),
        AnsiColor::BrightCyan => egui::Color32::from_rgb(34, 211, 238),
        AnsiColor::BrightWhite => egui::Color32::from_rgb(255, 255, 255),
    }
}

fn advance_log_match(window: &mut PodLogWindowState, log_store: &LogStoreService, forward: bool) {
    if window.search.match_count == 0 {
        return;
    }
    let current = window.search.active_match.unwrap_or_else(|| {
        if forward {
            window.search.match_count - 1
        } else {
            0
        }
    });
    let next = if forward {
        (current + 1) % window.search.match_count
    } else {
        (current + window.search.match_count - 1) % window.search.match_count
    };
    window.search.active_match = Some(next);
    let _ = log_store.resolve_match(window.id, window.search.generation, next);
}

fn advance_log_line(window: &mut PodLogWindowState, forward: bool) {
    let count = displayed_line_count(window);
    if count == 0 {
        return;
    }
    let current = window.search.active_display_row;
    let next = match (current, forward) {
        (Some(row), true) => (row + 1) % count,
        (Some(row), false) => (row + count - 1) % count,
        (None, true) => 0,
        (None, false) => count - 1,
    };
    window.search.active_display_row = Some(next);
    window.search.scroll_to_display_row = Some(next);
}

fn status_label(window: &PodLogWindowState) -> String {
    let status = if !window.search.query.is_empty() && !window.search.search_complete {
        format!("Searching… {} matches", window.search.match_count)
    } else if initial_spool_is_pending(window) {
        format!("Spooling… {} lines", window.total_lines)
    } else {
        match &window.status {
            PodLogStatus::Connecting => "Connecting…".to_owned(),
            PodLogStatus::Following => "Following".to_owned(),
            PodLogStatus::Finished => "Stream finished".to_owned(),
            PodLogStatus::Failed(error) => format!("Stream failed: {error}"),
        }
    };
    if let Some(backfill_lines) = window.backfill_lines {
        format!("{status} · backfill {}", compact_line_count(backfill_lines))
    } else {
        status
    }
}

fn compact_line_count(lines: usize) -> String {
    match lines {
        0..=999 => lines.to_string(),
        1_000..=999_999 => format!("{:.1}k", lines as f64 / 1_000.0),
        _ => format!("{:.1}M", lines as f64 / 1_000_000.0),
    }
}

fn status_color(status: &PodLogStatus) -> egui::Color32 {
    match status {
        PodLogStatus::Connecting => gray::_400,
        PodLogStatus::Following => SUCCESS,
        PodLogStatus::Finished => gray::_400,
        PodLogStatus::Failed(_) => status::DANGER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_store::{LOG_PAGE_SIZE, LogPageRow, LogStoreConfig, LogStoreResult};
    use crate::minimal_resource::PodLogContainer;
    use crate::resource_table::ContainerKind;
    use crate::worker::MockWorker;
    use components::test_support::UiHarnessSnapshot;
    use egui_kittest::{Harness, kittest::Queryable};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    fn log_window(lines: &[&str]) -> PodLogWindowState {
        let mut window = PodLogWindowState::new(
            1,
            1,
            "default".to_owned(),
            "api-0".to_owned(),
            PodLogContainer {
                name: "api".to_owned(),
                kind: ContainerKind::App,
                image: None,
            },
        );
        window.total_lines = lines.len();
        window.initial_page_loaded = true;
        window.store_opened = true;
        window.status = PodLogStatus::Following;
        window.insert_page(
            LogPageKey {
                generation: 0,
                filter_matches: false,
                page_start: 0,
            },
            lines
                .iter()
                .enumerate()
                .map(|(line_index, text)| {
                    let parsed = crate::ansi::parse_kubernetes_log_line(text);
                    LogPageRow {
                        display_row: line_index,
                        line_index,
                        timestamp: parsed.timestamp,
                        text: parsed.line.text,
                        style_spans: parsed.line.style_spans,
                        match_ranges: Vec::new(),
                    }
                })
                .collect(),
        );
        window
    }

    fn fully_loaded_log_window(line_count: usize) -> PodLogWindowState {
        let mut window = log_window(&[]);
        window.total_lines = line_count;
        window.status = PodLogStatus::Finished;

        for page_start in (0..line_count).step_by(LOG_PAGE_SIZE) {
            let page_end = (page_start + LOG_PAGE_SIZE).min(line_count);
            window.insert_page(
                LogPageKey {
                    generation: 0,
                    filter_matches: false,
                    page_start,
                },
                (page_start..page_end)
                    .map(|line_index| LogPageRow {
                        display_row: line_index,
                        line_index,
                        timestamp: None,
                        text: format!("line {line_index}"),
                        style_spans: Vec::new(),
                        match_ranges: Vec::new(),
                    })
                    .collect(),
            );
        }

        window
    }

    fn select_log_position(window: &mut PodLogWindowState, display_row: usize, byte_offset: usize) {
        let position = LogTextPosition {
            display_row,
            byte_offset,
        };
        window.selection = Some(LogTextSelection {
            anchor: position,
            focus: position,
        });
        window.caret_preferred_column = None;
    }

    fn move_key(
        window: &mut PodLogWindowState,
        log_store: &LogStoreService,
        key: egui::Key,
        modifiers: egui::Modifiers,
        page_rows: usize,
    ) {
        let display_count = displayed_line_count(window);
        assert!(move_log_caret(
            window,
            log_store,
            display_count,
            page_rows,
            key,
            modifiers,
            1.0,
        ));
    }

    fn caret_focus(window: &PodLogWindowState) -> LogTextPosition {
        window.selection.expect("test positions a log caret").focus
    }

    #[test]
    fn keyboard_caret_moves_by_character_word_and_line() {
        let log_store = LogStoreService::default();
        let mut window = log_window(&["alpha beta", "xy", "012345"]);
        select_log_position(&mut window, 0, 5);

        assert!(move_log_caret(
            &mut window,
            &log_store,
            3,
            1,
            egui::Key::ArrowRight,
            egui::Modifiers::NONE,
            1.0,
        ));
        assert_eq!(window.selection.unwrap().focus.byte_offset, 6);

        assert!(move_log_caret(
            &mut window,
            &log_store,
            3,
            1,
            egui::Key::ArrowRight,
            egui::Modifiers::CTRL,
            2.0,
        ));
        assert_eq!(window.selection.unwrap().focus.byte_offset, 10);

        assert!(move_log_caret(
            &mut window,
            &log_store,
            3,
            1,
            egui::Key::ArrowRight,
            egui::Modifiers::NONE,
            3.0,
        ));
        assert_eq!(window.selection.unwrap().focus.display_row, 1);
        assert_eq!(window.selection.unwrap().focus.byte_offset, 0);

        assert!(move_log_caret(
            &mut window,
            &log_store,
            3,
            1,
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
            4.0,
        ));
        assert_eq!(window.selection.unwrap().focus.display_row, 2);
        assert_eq!(window.selection.unwrap().focus.byte_offset, 0);
    }

    #[test]
    fn keyboard_caret_shift_extends_and_plain_arrow_collapses_selection() {
        let log_store = LogStoreService::default();
        let mut window = log_window(&["abcdef"]);
        select_log_position(&mut window, 0, 2);

        assert!(move_log_caret(
            &mut window,
            &log_store,
            1,
            1,
            egui::Key::ArrowRight,
            egui::Modifiers::SHIFT,
            1.0,
        ));
        assert_eq!(
            window.selection,
            Some(LogTextSelection {
                anchor: LogTextPosition {
                    display_row: 0,
                    byte_offset: 2,
                },
                focus: LogTextPosition {
                    display_row: 0,
                    byte_offset: 3,
                },
            })
        );

        assert!(move_log_caret(
            &mut window,
            &log_store,
            1,
            1,
            egui::Key::ArrowLeft,
            egui::Modifiers::NONE,
            2.0,
        ));
        assert_eq!(
            window.selection,
            Some(LogTextSelection {
                anchor: LogTextPosition {
                    display_row: 0,
                    byte_offset: 2,
                },
                focus: LogTextPosition {
                    display_row: 0,
                    byte_offset: 2,
                },
            })
        );
    }

    #[test]
    fn keyboard_caret_moves_in_every_direction_and_preserves_vertical_column() {
        let log_store = LogStoreService::default();
        let mut window = log_window(&["abc", "d", "abcdef"]);
        select_log_position(&mut window, 1, 1);

        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowLeft,
            egui::Modifiers::NONE,
            1,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 1,
                byte_offset: 0
            }
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowLeft,
            egui::Modifiers::NONE,
            1,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 0,
                byte_offset: 3
            }
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowUp,
            egui::Modifiers::NONE,
            1,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 0,
                byte_offset: 3
            }
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
            1,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 1,
                byte_offset: 1
            }
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
            1,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 2,
                byte_offset: 3
            }
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowUp,
            egui::Modifiers::NONE,
            1,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 1,
                byte_offset: 1
            }
        );

        select_log_position(&mut window, 0, 3);
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowRight,
            egui::Modifiers::NONE,
            1,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 1,
                byte_offset: 0
            }
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowRight,
            egui::Modifiers::NONE,
            1,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 1,
                byte_offset: 1
            }
        );
    }

    #[test]
    fn keyboard_caret_word_navigation_and_shift_control_selection_work_in_both_directions() {
        let log_store = LogStoreService::default();
        let mut window = log_window(&["alpha beta gamma"]);
        select_log_position(&mut window, 0, 11);

        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowLeft,
            egui::Modifiers::CTRL,
            1,
        );
        assert_eq!(caret_focus(&window).byte_offset, 6);
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowRight,
            egui::Modifiers::CTRL,
            1,
        );
        assert_eq!(caret_focus(&window).byte_offset, 10);

        select_log_position(&mut window, 0, 6);
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowRight,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            1,
        );
        assert_eq!(
            window.selection.unwrap().normalized(),
            (
                LogTextPosition {
                    display_row: 0,
                    byte_offset: 6
                },
                LogTextPosition {
                    display_row: 0,
                    byte_offset: 10
                },
            )
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowLeft,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            1,
        );
        assert_eq!(
            window.selection,
            Some(LogTextSelection {
                anchor: LogTextPosition {
                    display_row: 0,
                    byte_offset: 6
                },
                focus: LogTextPosition {
                    display_row: 0,
                    byte_offset: 6
                },
            })
        );
    }

    #[test]
    fn keyboard_caret_supports_line_document_page_and_select_all_navigation() {
        let log_store = LogStoreService::default();
        let mut window = log_window(&["zero", "one", "two", "three", "four", "five"]);
        select_log_position(&mut window, 3, 2);

        move_key(
            &mut window,
            &log_store,
            egui::Key::Home,
            egui::Modifiers::NONE,
            2,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 3,
                byte_offset: 0
            }
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::End,
            egui::Modifiers::NONE,
            2,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 3,
                byte_offset: 5
            }
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::PageUp,
            egui::Modifiers::NONE,
            2,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 1,
                byte_offset: 3
            }
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::PageDown,
            egui::Modifiers::NONE,
            2,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 3,
                byte_offset: 5
            }
        );

        select_log_position(&mut window, 3, 2);
        move_key(
            &mut window,
            &log_store,
            egui::Key::Home,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            2,
        );
        assert_eq!(
            window.selection.unwrap().normalized(),
            (
                LogTextPosition {
                    display_row: 0,
                    byte_offset: 0
                },
                LogTextPosition {
                    display_row: 3,
                    byte_offset: 2
                },
            )
        );

        select_log_position(&mut window, 2, 1);
        move_key(
            &mut window,
            &log_store,
            egui::Key::End,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            2,
        );
        assert_eq!(
            window.selection.unwrap().normalized(),
            (
                LogTextPosition {
                    display_row: 2,
                    byte_offset: 1
                },
                LogTextPosition {
                    display_row: 5,
                    byte_offset: 4
                },
            )
        );

        select_log_position(&mut window, 3, 2);
        move_key(
            &mut window,
            &log_store,
            egui::Key::PageDown,
            egui::Modifiers::SHIFT,
            2,
        );
        assert_eq!(
            window.selection.unwrap().normalized(),
            (
                LogTextPosition {
                    display_row: 3,
                    byte_offset: 2
                },
                LogTextPosition {
                    display_row: 5,
                    byte_offset: 2
                },
            )
        );

        select_log_position(&mut window, 2, 1);
        move_key(
            &mut window,
            &log_store,
            egui::Key::PageUp,
            egui::Modifiers::SHIFT,
            2,
        );
        assert_eq!(
            window.selection.unwrap().normalized(),
            (
                LogTextPosition {
                    display_row: 0,
                    byte_offset: 1
                },
                LogTextPosition {
                    display_row: 2,
                    byte_offset: 1
                },
            )
        );

        select_log_position(&mut window, 2, 1);
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowUp,
            egui::Modifiers::COMMAND,
            2,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 0,
                byte_offset: 0
            }
        );
        select_log_position(&mut window, 2, 1);
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowDown,
            egui::Modifiers::COMMAND,
            2,
        );
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 5,
                byte_offset: 4
            }
        );

        select_log_position(&mut window, 2, 1);
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowUp,
            egui::Modifiers::SHIFT,
            2,
        );
        assert_eq!(
            window.selection.unwrap().normalized(),
            (
                LogTextPosition {
                    display_row: 1,
                    byte_offset: 1
                },
                LogTextPosition {
                    display_row: 2,
                    byte_offset: 1
                },
            )
        );
        move_key(
            &mut window,
            &log_store,
            egui::Key::ArrowDown,
            egui::Modifiers::SHIFT,
            2,
        );
        assert_eq!(
            window.selection,
            Some(LogTextSelection {
                anchor: LogTextPosition {
                    display_row: 2,
                    byte_offset: 1
                },
                focus: LogTextPosition {
                    display_row: 2,
                    byte_offset: 1
                },
            })
        );

        select_log_position(&mut window, 2, 1);
        move_key(
            &mut window,
            &log_store,
            egui::Key::A,
            egui::Modifiers::COMMAND,
            2,
        );
        assert_eq!(
            window.selection.unwrap().normalized(),
            (
                LogTextPosition {
                    display_row: 0,
                    byte_offset: 0
                },
                LogTextPosition {
                    display_row: 5,
                    byte_offset: 4
                },
            )
        );
    }

    #[test]
    fn keyboard_caret_ignores_typing() {
        let log_store = LogStoreService::default();
        let mut window = log_window(&["readonly"]);
        select_log_position(&mut window, 0, 2);

        assert!(!move_log_caret(
            &mut window,
            &log_store,
            1,
            1,
            egui::Key::A,
            egui::Modifiers::NONE,
            1.0,
        ));
        assert_eq!(
            caret_focus(&window),
            LogTextPosition {
                display_row: 0,
                byte_offset: 2
            }
        );
    }

    #[test]
    fn keyboard_caret_waits_for_an_unloaded_target_page() {
        let log_store = LogStoreService::default();
        let context = egui::Context::default();
        let mut window = log_window(&["start"]);
        window.total_lines = LOG_PAGE_SIZE + 1;
        let total_lines = window.total_lines;
        select_log_position(&mut window, 0, 0);

        assert!(move_log_caret(
            &mut window,
            &log_store,
            total_lines,
            1,
            egui::Key::End,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            1.0,
        ));
        assert_eq!(
            window.pending_caret,
            Some(PendingLogCaret {
                display_row: LOG_PAGE_SIZE,
                character_column: usize::MAX,
                anchor: Some(LogTextPosition {
                    display_row: 0,
                    byte_offset: 0,
                }),
            })
        );

        window.insert_page(
            LogPageKey {
                generation: 0,
                filter_matches: false,
                page_start: LOG_PAGE_SIZE,
            },
            vec![LogPageRow {
                display_row: LOG_PAGE_SIZE,
                line_index: LOG_PAGE_SIZE,
                timestamp: None,
                text: "destination".to_owned(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            }],
        );
        resolve_pending_caret(&mut window, &log_store, total_lines, &context);

        assert_eq!(window.pending_caret, None);
        assert_eq!(
            window.selection,
            Some(LogTextSelection {
                anchor: LogTextPosition {
                    display_row: 0,
                    byte_offset: 0,
                },
                focus: LogTextPosition {
                    display_row: LOG_PAGE_SIZE,
                    byte_offset: "destination".len(),
                },
            })
        );
    }

    #[test]
    fn clicking_a_log_row_focuses_the_keyboard_caret() {
        let window = Rc::new(RefCell::new(log_window(&["clickable log line"])));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            );
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        harness.get_by_label("clickable log line").click();
        harness.step();
        let clicked_position = window
            .borrow()
            .selection
            .expect("clicking a log row places the caret")
            .focus;

        harness.key_press(egui::Key::ArrowRight);
        harness.step();
        let moved_position = window
            .borrow()
            .selection
            .expect("focused log caret remains present")
            .focus;

        assert_eq!(moved_position.display_row, clicked_position.display_row);
        assert!(moved_position.byte_offset > clicked_position.byte_offset);
    }

    #[test]
    fn hovering_a_log_row_uses_a_text_cursor() {
        let window = Rc::new(RefCell::new(log_window(&["hoverable log line"])));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            );
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        let hover_position = harness.get_by_label("hoverable log line").rect().center();
        harness.event(egui::Event::PointerMoved(hover_position));
        harness.step();

        assert_eq!(
            harness.output().platform_output.cursor_icon,
            egui::CursorIcon::Text
        );
    }

    #[test]
    fn focused_log_caret_moves_between_rows_in_the_ui() {
        let window = Rc::new(RefCell::new(log_window(&["first row", "second row"])));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            );
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        harness.get_by_label("first row").click();
        harness.step();
        harness.key_press(egui::Key::ArrowDown);
        harness.step();

        assert_eq!(caret_focus(&window.borrow()).display_row, 1);
    }

    #[test]
    fn clicking_a_prefixed_log_row_places_the_caret_in_its_message() {
        let window = Rc::new(RefCell::new(log_window(&[
            "2026-08-11T10:00:00Z focused caret line",
        ])));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions {
            show_line_numbers: true,
            show_timestamps: true,
            ..LogDisplayOptions::default()
        }));
        let display_options_for_ui = display_options.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            );
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        let label = harness.get_by_label_contains("focused caret line");
        let character_width = label.rect().width()
            / "       0  2026-08-11T10:00:00Z  focused caret line"
                .chars()
                .count() as f32;
        let prefix_width =
            "       0  2026-08-11T10:00:00Z  ".chars().count() as f32 * character_width;
        let clicked_column = 7;
        let click_position = egui::pos2(
            label.rect().left() + prefix_width + clicked_column as f32 * character_width,
            label.rect().center().y,
        );
        harness.event(egui::Event::PointerMoved(click_position));
        harness.event(egui::Event::PointerButton {
            pos: click_position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.event(egui::Event::PointerButton {
            pos: click_position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();

        let position = caret_focus(&window.borrow());
        assert_eq!(position.display_row, 0);
        assert_eq!(position.byte_offset, clicked_column);
    }

    #[test]
    fn clicking_a_horizontally_scrolled_log_row_uses_the_original_text_column() {
        let line = "x".repeat(2_000);
        let window = Rc::new(RefCell::new(log_window(&[&line])));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let scroll_state = Rc::new(RefCell::new(None));
        let scroll_state_for_ui = scroll_state.clone();
        let viewport = Rc::new(RefCell::new(egui::Rect::NOTHING));
        let viewport_for_ui = viewport.clone();
        let character_width = Rc::new(RefCell::new(0.0));
        let character_width_for_ui = character_width.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            *character_width_for_ui.borrow_mut() = ctx
                .fonts_mut(|fonts| fonts.glyph_width(&egui::FontId::monospace(LOG_FONT_SIZE), '0'));
            let output = show_log_window_with_scroll_state(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            );
            *scroll_state_for_ui.borrow_mut() = Some(output.state);
            *viewport_for_ui.borrow_mut() = output.inner_rect;
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        harness.event(egui::Event::PointerMoved(egui::pos2(400.0, 100.0)));
        harness.step();
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(-500.0, 0.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
        harness.run_steps(2);

        let viewport = *viewport.borrow();
        let scroll_offset = scroll_state
            .borrow()
            .as_ref()
            .expect("the log scroll area was rendered")
            .offset;
        assert!(
            scroll_offset.x > 0.0,
            "the log view was horizontally scrolled"
        );
        let click_position = egui::pos2(viewport.left() + 160.0, viewport.top() + 8.0);
        let expected_column = ((scroll_offset.x + click_position.x - viewport.left())
            / *character_width.borrow())
        .round() as usize;
        harness.event(egui::Event::PointerMoved(click_position));
        harness.event(egui::Event::PointerButton {
            pos: click_position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.event(egui::Event::PointerButton {
            pos: click_position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();

        assert_eq!(
            caret_focus(&window.borrow()),
            LogTextPosition {
                display_row: 0,
                byte_offset: expected_column,
            }
        );
    }

    #[test]
    fn pod_log_viewer_keyboard_caret_after_arrow_down_snapshot() {
        let window = Rc::new(RefCell::new(log_window(&[
            "first row",
            "second row",
            "third row",
        ])));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            );
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        harness.get_by_label("first row").click();
        harness.step();
        harness.key_press(egui::Key::ArrowDown);
        harness.step();
        harness.ui_harness("pod_logs/pod_log_viewer_keyboard_caret_after_arrow_down_snapshot/keyboard_caret_after_arrow_down");
    }

    #[test]
    fn pod_log_viewer_keyboard_caret_after_arrow_up_snapshot() {
        let window = Rc::new(RefCell::new(log_window(&[
            "first row",
            "second row",
            "third row",
        ])));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let caret_has_focus = Rc::new(RefCell::new(false));
        let caret_has_focus_for_ui = caret_has_focus.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            );
            *caret_has_focus_for_ui.borrow_mut() =
                ctx.memory(|memory| memory.has_focus(egui::Id::new(("pod-log-caret", 1))));
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        harness.get_by_label("second row").click();
        harness.step();
        harness.key_press(egui::Key::ArrowUp);
        harness.step();
        harness.run_steps(2);

        assert_eq!(caret_focus(&window.borrow()).display_row, 0);
        assert!(*caret_has_focus.borrow(), "ArrowUp must retain caret focus");
        harness.ui_harness("pod_logs/pod_log_viewer_keyboard_caret_after_arrow_up_snapshot/keyboard_caret_after_arrow_up");
    }

    #[test]
    fn pod_log_viewer_keyboard_caret_snapshot() {
        let window = Rc::new(RefCell::new(log_window(&[
            "2026-08-11T10:00:00Z first readonly line",
            "2026-08-11T10:00:01Z focused caret line",
        ])));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions {
            show_line_numbers: true,
            show_timestamps: true,
            ..LogDisplayOptions::default()
        }));
        let display_options_for_ui = display_options.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            );
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        let label = harness.get_by_label_contains("focused caret line");
        let character_width = label.rect().width()
            / "       1  2026-08-11T10:00:01Z  focused caret line"
                .chars()
                .count() as f32;
        let prefix_width =
            "       1  2026-08-11T10:00:01Z  ".chars().count() as f32 * character_width;
        let click_position = egui::pos2(
            label.rect().left()
                + prefix_width
                + "focused ".chars().count() as f32 * character_width,
            label.rect().center().y,
        );
        harness.event(egui::Event::PointerMoved(click_position));
        harness.event(egui::Event::PointerButton {
            pos: click_position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.event(egui::Event::PointerButton {
            pos: click_position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
        harness.ui_harness("pod_logs/pod_log_viewer_keyboard_caret_snapshot/keyboard_caret");
    }

    #[test]
    fn completed_fully_loaded_logs_do_not_oscillate_at_the_bottom() {
        let window = Rc::new(RefCell::new(fully_loaded_log_window(10_000)));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let scroll_state = Rc::new(RefCell::new(None));
        let scroll_state_for_ui = scroll_state.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            *scroll_state_for_ui.borrow_mut() = Some(show_log_window_with_scroll_state(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            ));
        });
        components::test_support::setup_egui(&mut harness);
        harness.run();

        let bottom_offset = scroll_state
            .borrow()
            .as_ref()
            .expect("log scroll area was rendered")
            .state
            .offset
            .y;
        assert!(bottom_offset > 0.0);

        for _ in 0..5 {
            harness
                .input_mut()
                .events
                .push(egui::Event::PointerMoved(egui::pos2(400.0, 100.0)));
            harness.input_mut().events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -120.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            });
            harness.step();
            let offset = scroll_state
                .borrow()
                .as_ref()
                .expect("log scroll area was rendered")
                .state
                .offset
                .y;
            assert_eq!(offset, bottom_offset);
        }
    }

    #[test]
    fn displayed_line_navigation_scrolls_the_viewer_and_snapshots_the_destination() {
        let window = Rc::new(RefCell::new(fully_loaded_log_window(512)));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let scroll_state = Rc::new(RefCell::new(None));
        let scroll_state_for_ui = scroll_state.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            *scroll_state_for_ui.borrow_mut() = Some(show_log_window_with_scroll_state(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            ));
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);

        window.borrow_mut().search.active_display_row = Some(0);
        harness.get_by_label("Previous displayed line").click();
        harness.run_steps(2);
        assert_eq!(window.borrow().search.active_display_row, Some(511));
        assert!(
            harness.get_by_label("line 511").rect().intersects(
                scroll_state
                    .borrow()
                    .as_ref()
                    .expect("log scroll area was rendered")
                    .inner_rect
            ),
            "wrapped previous navigation must move the viewport"
        );

        window.borrow_mut().search.active_display_row = Some(399);
        harness.get_by_label("Next displayed line").click();
        harness.run_steps(2);

        let scroll_state = scroll_state.borrow();
        let output = scroll_state.as_ref().expect("log scroll area was rendered");
        assert_eq!(window.borrow().search.active_display_row, Some(400));
        assert_eq!(window.borrow().search.scroll_to_display_row, None);
        assert!(
            output.state.offset.y > 0.0,
            "navigation must move the viewport"
        );
        assert!(
            harness
                .get_by_label("line 400")
                .rect()
                .intersects(output.inner_rect),
            "the requested line must be visible after navigation"
        );
        harness.ui_harness(
            "pod_logs/pod_log_viewer_displayed_line_navigation_snapshot/next_displayed_line",
        );
    }

    #[test]
    fn resolved_match_navigation_scrolls_the_viewer_and_snapshots_the_destination() {
        let window = Rc::new(RefCell::new(fully_loaded_log_window(512)));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let scroll_state = Rc::new(RefCell::new(None));
        let scroll_state_for_ui = scroll_state.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            *scroll_state_for_ui.borrow_mut() = Some(show_log_window_with_scroll_state(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            ));
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);

        {
            let mut window = window.borrow_mut();
            let page = window
                .pages
                .get_mut(&LogPageKey {
                    generation: 0,
                    filter_matches: false,
                    page_start: LOG_PAGE_SIZE,
                })
                .expect("target page is loaded");
            let target = &mut page.rows[400 - LOG_PAGE_SIZE];
            target.text = "needle line 400".to_owned();
            target.match_ranges = vec![(0, "needle".len())];
            window.search.query = "needle".to_owned();
            window.search.match_count = 2;
            window.search.active_match = Some(0);
        }
        harness.get_by_label("Previous matching line").click();
        harness.step();
        assert_eq!(window.borrow().search.active_match, Some(1));
        window.borrow_mut().search.active_match = Some(0);
        harness.get_by_label("Next matching line").click();
        harness.step();
        assert_eq!(window.borrow().search.active_match, Some(1));
        // The store resolves the selected match asynchronously. State-level
        // coverage below verifies that this result maps unfiltered matches to
        // their source line and filtered matches to their match row.
        window.borrow_mut().search.active_display_row = Some(400);
        window.borrow_mut().search.scroll_to_display_row = Some(400);
        harness.run_steps(2);

        let scroll_state = scroll_state.borrow();
        let output = scroll_state.as_ref().expect("log scroll area was rendered");
        assert_eq!(window.borrow().search.active_match, Some(1));
        assert_eq!(window.borrow().search.active_display_row, Some(400));
        assert_eq!(window.borrow().search.scroll_to_display_row, None);
        assert!(
            output.state.offset.y > 0.0,
            "match navigation must move the viewport"
        );
        assert!(
            harness
                .get_by_label("needle line 400")
                .rect()
                .intersects(output.inner_rect),
            "the resolved matching line must be visible"
        );
        harness.ui_harness(
            "pod_logs/pod_log_viewer_match_navigation_snapshot/resolved_match_destination",
        );
    }

    #[test]
    fn log_navigation_wraps_at_both_ends() {
        let log_store = LogStoreService::default();
        let mut window = log_window(&["zero", "one", "two"]);
        window.search.match_count = 3;

        advance_log_line(&mut window, false);
        assert_eq!(window.search.active_display_row, Some(2));
        advance_log_line(&mut window, true);
        assert_eq!(window.search.active_display_row, Some(0));

        advance_log_match(&mut window, &log_store, false);
        assert_eq!(window.search.active_match, Some(2));
        advance_log_match(&mut window, &log_store, true);
        assert_eq!(window.search.active_match, Some(0));
    }

    #[test]
    fn loading_a_wide_page_does_not_move_the_bottom_offset() {
        let wide_line = "x".repeat(4 * 1024);
        let mut window = log_window(&[]);
        window.total_lines = LOG_PAGE_SIZE * 2;
        window.status = PodLogStatus::Finished;
        window.insert_page(
            LogPageKey {
                generation: 0,
                filter_matches: false,
                page_start: LOG_PAGE_SIZE,
            },
            (LOG_PAGE_SIZE..window.total_lines)
                .map(|line_index| LogPageRow {
                    display_row: line_index,
                    line_index,
                    timestamp: None,
                    text: wide_line.clone(),
                    style_spans: Vec::new(),
                    match_ranges: Vec::new(),
                })
                .collect(),
        );
        window.search.scroll_to_display_row = Some(window.total_lines - 1);

        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut display_options = LogDisplayOptions::default();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut render = |window: &mut PodLogWindowState| {
            let mut scroll_state = None;
            let mut output = context.run_ui(input.clone(), |ctx| {
                scroll_state = Some(show_log_window_with_scroll_state(
                    ctx,
                    window,
                    &mut display_options,
                    &log_store,
                    &mut close_requested,
                ));
            });
            output.textures_delta.clear();
            scroll_state.expect("log scroll area was rendered")
        };

        let _ = render(&mut window);
        let loaded_offset = render(&mut window);
        window.pages.clear();
        window.page_order.clear();
        window.page_cache_bytes = 0;
        let loading_offset = render(&mut window);

        assert_eq!(loading_offset.inner_rect, loaded_offset.inner_rect);
        assert_eq!(loading_offset.content_size, loaded_offset.content_size);
        assert_eq!(loading_offset.state.offset.y, loaded_offset.state.offset.y);
    }

    #[test]
    fn loading_a_narrow_page_does_not_move_the_bottom_offset() {
        let mut window = log_window(&[]);
        window.total_lines = LOG_PAGE_SIZE * 2;
        window.status = PodLogStatus::Finished;
        window.insert_page(
            LogPageKey {
                generation: 0,
                filter_matches: false,
                page_start: LOG_PAGE_SIZE,
            },
            (LOG_PAGE_SIZE..window.total_lines)
                .map(|line_index| LogPageRow {
                    display_row: line_index,
                    line_index,
                    timestamp: None,
                    text: format!("line {line_index}"),
                    style_spans: Vec::new(),
                    match_ranges: Vec::new(),
                })
                .collect(),
        );
        window.search.scroll_to_display_row = Some(window.total_lines - 1);

        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut display_options = LogDisplayOptions::default();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut render = |window: &mut PodLogWindowState| {
            let mut scroll_state = None;
            let mut output = context.run_ui(input.clone(), |ctx| {
                scroll_state = Some(show_log_window_with_scroll_state(
                    ctx,
                    window,
                    &mut display_options,
                    &log_store,
                    &mut close_requested,
                ));
            });
            output.textures_delta.clear();
            scroll_state.expect("log scroll area was rendered")
        };

        let _ = render(&mut window);
        let loaded_offset = render(&mut window);
        window.pages.clear();
        window.page_order.clear();
        window.page_cache_bytes = 0;
        let loading_offset = render(&mut window);

        assert_eq!(loading_offset.inner_rect, loaded_offset.inner_rect);
        assert_eq!(loading_offset.content_size, loaded_offset.content_size);
        assert_eq!(loading_offset.state.offset.y, loaded_offset.state.offset.y);
    }

    #[test]
    fn first_unfiltered_page_ends_the_initial_spool_state() {
        let mut window = log_window(&[]);
        window.total_lines = 1;
        window.initial_page_loaded = false;

        assert!(initial_spool_is_pending(&window));
        window.insert_page(
            LogPageKey {
                generation: 0,
                filter_matches: false,
                page_start: 0,
            },
            vec![LogPageRow {
                display_row: 0,
                line_index: 0,
                timestamp: None,
                text: "first line".to_owned(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            }],
        );

        assert!(!initial_spool_is_pending(&window));
    }

    #[test]
    fn first_wide_page_does_not_change_the_vertical_viewport() {
        let wide_line = "x".repeat(4 * 1024);
        let mut window = log_window(&[]);
        window.total_lines = LOG_PAGE_SIZE * 2;
        window.status = PodLogStatus::Finished;
        window.search.scroll_to_display_row = Some(window.total_lines - 1);

        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut display_options = LogDisplayOptions::default();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut render = |window: &mut PodLogWindowState| {
            let mut scroll_state = None;
            let mut output = context.run_ui(input.clone(), |ctx| {
                scroll_state = Some(show_log_window_with_scroll_state(
                    ctx,
                    window,
                    &mut display_options,
                    &log_store,
                    &mut close_requested,
                ));
            });
            output.textures_delta.clear();
            scroll_state.expect("log scroll area was rendered")
        };

        let _ = render(&mut window);
        let loading_offset = render(&mut window);
        window.insert_page(
            LogPageKey {
                generation: 0,
                filter_matches: false,
                page_start: LOG_PAGE_SIZE,
            },
            (LOG_PAGE_SIZE..window.total_lines)
                .map(|line_index| LogPageRow {
                    display_row: line_index,
                    line_index,
                    timestamp: None,
                    text: wide_line.clone(),
                    style_spans: Vec::new(),
                    match_ranges: Vec::new(),
                })
                .collect(),
        );
        let loaded_offset = render(&mut window);

        assert_eq!(loaded_offset.inner_rect, loading_offset.inner_rect);
        assert_eq!(loaded_offset.content_size.y, loading_offset.content_size.y);
        assert_eq!(loaded_offset.state.offset.y, loading_offset.state.offset.y);
    }

    #[test]
    fn horizontal_fragment_uses_byte_boundaries_for_utf8_text() {
        let text = "aé日z";

        assert_eq!(character_column_range(text, 1, 3), 1..6);
        assert_eq!(&text[character_column_range(text, 1, 3)], "é日");
        assert_eq!(character_column_range(text, 3, 4), 6..7);
    }

    #[test]
    fn pointer_position_excludes_metadata_prefix_and_restores_fragment_offset() {
        let text = "abcdef";

        assert_eq!(
            byte_offset_at_response_x(text, 70.0, 30.0, 0.0, 10.0),
            4,
            "line numbers and timestamps are before the text, not part of its cursor offset",
        );
        assert_eq!(
            byte_offset_at_response_x(text, 30.0, 10.0, 30.0, 10.0),
            5,
            "a horizontally clipped fragment restores its omitted character columns",
        );
    }

    #[test]
    fn caret_vertical_scroll_moves_only_when_the_caret_leaves_the_viewport() {
        let mut window = log_window(&["zero", "one", "two", "three", "four"]);
        select_log_position(&mut window, 2, 0);

        assert_eq!(caret_vertical_offset(&window, 20.0, 20.0, 10.0), 20.0);

        select_log_position(&mut window, 1, 0);
        assert_eq!(caret_vertical_offset(&window, 20.0, 20.0, 10.0), 10.0);

        select_log_position(&mut window, 4, 0);
        assert_eq!(caret_vertical_offset(&window, 20.0, 20.0, 10.0), 30.0);
    }

    #[test]
    fn horizontal_fragment_limits_ascii_layout_to_the_visible_columns() {
        let text = "x".repeat(4_096);
        let fragment = visible_text_fragment(&text, 1_000.0, 120.0, 10.0);

        assert_eq!(fragment.byte_range, 88..124);
        assert_eq!(fragment.start_x, 880.0);
    }

    #[test]
    fn wide_log_window_exposes_a_horizontal_scroll_range() {
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 200.0),
            )),
            ..Default::default()
        };
        let mut window = log_window(&[&"x".repeat(4 * 1024)]);
        let mut display_options = LogDisplayOptions::default();
        let log_store = LogStoreService::default();
        let mut close_requested = false;

        let mut output = context.run_ui(input, |context| {
            show_log_window(
                context,
                &mut window,
                &mut display_options,
                &log_store,
                &mut close_requested,
            );
        });
        output.textures_delta.clear();

        assert!(window.horizontal_content_width > 320.0);
    }

    #[test]
    fn layout_highlights_only_matching_segments() {
        let job = log_line_layout_job(
            4,
            None,
            "http http",
            &[],
            &[(0, 4), (5, 9)],
            LogDisplayOptions {
                show_line_numbers: true,
                ..Default::default()
            },
        );
        assert_eq!(job.sections.len(), 4);
        assert_eq!(job.text, "     4  http http");
    }

    #[test]
    fn layout_preserves_ansi_style_while_highlighting_matches() {
        let style = Style::new()
            .fg_color(Some(AnsiColor::Red.into()))
            .underline();
        let job = log_line_layout_job(
            0,
            None,
            "error",
            &[AnsiStyleSpan {
                range: (0, 5),
                style,
            }],
            &[(1, 4)],
            LogDisplayOptions {
                show_line_numbers: true,
                ..Default::default()
            },
        );

        assert_eq!(job.text, "     0  error");
        assert_eq!(job.sections.len(), 4);
        assert_eq!(
            job.sections[1].format.color,
            ansi_palette_color(AnsiColor::Red)
        );
        assert!(!job.sections[1].format.underline.is_empty());
        assert_eq!(
            job.sections[2].format.background,
            egui::Color32::from_rgb(120, 53, 15)
        );
    }

    #[test]
    fn wide_log_rows_are_not_wrapped_and_need_horizontal_scrolling() {
        let wide_line = "x".repeat(4 * 1024);
        let context = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 200.0),
            )),
            ..Default::default()
        };
        let mut first_scroll_output = None;
        let mut first_output_frame = context.run_ui(input(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                first_scroll_output = Some(show_wide_test_scroll_area(ui, &wide_line));
            });
        });
        first_output_frame.textures_delta.clear();

        let first_output = first_scroll_output.expect("scroll area was rendered");
        assert!(first_output.content_size.x > first_output.inner_rect.width());

        let mut scroll_input = input();
        scroll_input.events = vec![
            egui::Event::PointerMoved(first_output.inner_rect.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(-120.0, 0.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            },
        ];
        let mut second_scroll_output = None;
        let mut second_output_frame = context.run_ui(scroll_input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                second_scroll_output = Some(show_wide_test_scroll_area(ui, &wide_line));
            });
        });
        second_output_frame.textures_delta.clear();

        assert!(
            second_scroll_output
                .expect("scroll area was rendered")
                .state
                .offset
                .x
                > 0.0
        );
    }

    fn show_wide_test_scroll_area(
        ui: &mut egui::Ui,
        wide_line: &str,
    ) -> egui::scroll_area::ScrollAreaOutput<()> {
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        components::scroll::both()
            .id_salt("wide-log-scroll-test")
            .auto_shrink([false, false])
            .show_rows(ui, row_height, 1, |ui, _| {
                ui.add(
                    egui::Label::new(log_line_layout_job(
                        0,
                        None,
                        wide_line,
                        &[],
                        &[],
                        LogDisplayOptions::default(),
                    ))
                    .extend(),
                );
            })
    }

    #[test]
    fn layout_toggles_metadata_and_ansi_styling_independently() {
        let style = Style::new().fg_color(Some(AnsiColor::Red.into()));
        let job = log_line_layout_job(
            4,
            Some("2026-08-08T15:22:17.143Z"),
            "error",
            &[AnsiStyleSpan {
                range: (0, 5),
                style,
            }],
            &[],
            LogDisplayOptions {
                show_line_numbers: true,
                show_timestamps: true,
                render_ansi: false,
            },
        );

        assert_eq!(job.text, "     4  2026-08-08T15:22:17.143Z  error");
        assert_eq!(
            job.sections.last().expect("message section").format.color,
            egui::Color32::from_rgb(229, 231, 235)
        );
    }

    #[test]
    fn display_toggles_update_the_shared_options() {
        let window = Rc::new(RefCell::new(log_window(&["api ready"])));
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let window_for_ui = window.clone();
        let display_options_for_ui = display_options.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            )
        });
        components::test_support::setup_egui(&mut harness);
        harness.run();

        harness
            .get_by_label("Show log line numbers")
            .click_accesskit();
        harness
            .get_by_label("Show Kubernetes log timestamps")
            .click_accesskit();
        harness
            .get_by_label("Render ANSI styling")
            .click_accesskit();
        harness.run();

        assert_eq!(
            *display_options.borrow(),
            LogDisplayOptions {
                show_line_numbers: true,
                show_timestamps: true,
                render_ansi: false,
            }
        );
    }

    #[test]
    fn scrolling_requests_one_background_page_and_renders_it_when_loaded() {
        let service = LogStoreService::new(LogStoreConfig {
            page_size: 2,
            ..LogStoreConfig::default()
        });
        let lines = ["line 0", "line 1", "line 2", "line 3", "line 4"];
        assert!(service.append(1, lines.into_iter().map(str::to_owned).collect()));
        let _ = wait_for_store_result(&service, |result| {
            matches!(result, LogStoreResult::Updated { .. })
        });

        let mut window = log_window(&lines);
        window.page_size = 2;
        window.clear_pages();

        // The virtualized row callback can run more than once before I/O
        // finishes. It must issue one request for the missing page.
        request_page_for_display_row(&mut window, &service, 2);
        request_page_for_display_row(&mut window, &service, 2);
        let key = LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: 2,
        };
        assert_eq!(window.pending_pages, std::collections::HashSet::from([key]));

        let LogStoreResult::PageLoaded { rows, .. } = wait_for_store_result(&service, |result| {
            matches!(result, LogStoreResult::PageLoaded { page_start: 2, .. })
        }) else {
            unreachable!()
        };
        window.insert_page(key, rows);

        assert!(!window.pending_pages.contains(&key));
        assert_eq!(window.pages[&key].rows[0].text, "line 2");
        assert_eq!(window.pages[&key].rows[1].text, "line 3");
    }

    #[test]
    fn pod_log_viewer_snapshot() {
        let mut window = log_window(&[
            "2026-08-08T15:22:17.143Z  INFO  server: listening on 0.0.0.0:8080",
            "2026-08-08T15:22:17.145Z  INFO  database: connection pool initialized",
            "2026-08-08T15:22:18.021Z  INFO  http: GET /healthz 200 2ms",
            "2026-08-08T15:22:19.403Z  INFO  http: GET /v1/widgets 200 14ms",
            "2026-08-08T15:22:21.687Z  \u{1b}[33mWARN\u{1b}[0m  cache: refreshing stale entry widgets:featured",
            "2026-08-08T15:22:22.004Z  INFO  cache: refresh complete",
            "2026-08-08T15:22:24.631Z  INFO  http: POST /v1/widgets 201 38ms",
            "2026-08-08T15:22:26.144Z  INFO  metrics: flushed 18 samples",
            "2026-08-08T15:22:29.711Z  INFO  http: GET /healthz 200 1ms",
            "2026-08-08T15:22:31.218Z  INFO  worker: processed batch of 42 jobs",
        ]);
        window.search.query = "http".to_owned();
        add_match_ranges(&mut window, false);
        snapshot_window(window, "pod_logs/pod_log_viewer_snapshot/viewer");
    }

    #[test]
    fn pod_log_viewer_wide_selected_fragment_snapshot() {
        let line = format!(
            "INFO  {}",
            (0..512)
                .map(|index| format!("column-{index:04} "))
                .collect::<String>()
        );
        let mut window = log_window(&[&line]);
        let text = &window.pages[&LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: 0,
        }]
            .rows[0]
            .text;
        let selected_start = text
            .find("column-0010")
            .expect("selection marker is present");
        let selected_end = text
            .find("column-0014")
            .expect("selection end marker is present")
            + "column-0014".len();
        window.selection = Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 0,
                byte_offset: selected_start,
            },
            focus: LogTextPosition {
                display_row: 0,
                byte_offset: selected_end,
            },
        });

        snapshot_window_after_horizontal_scroll(
            window,
            "pod_logs/pod_log_viewer_wide_selected_fragment_snapshot/wide_selected_fragment",
            1_000.0,
        );
    }

    #[test]
    fn pod_log_viewer_wide_multiline_selection_after_scroll_snapshot() {
        let lines = (0..3)
            .map(|row| {
                format!(
                    "INFO row-{row}  {}",
                    (0..512)
                        .map(|column| format!("column-{column:04} "))
                        .collect::<String>()
                )
            })
            .collect::<Vec<_>>();
        let line_refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
        let mut window = log_window(&line_refs);
        let page = &window.pages[&LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: 0,
        }];
        let start = page.rows[0]
            .text
            .find("column-0010")
            .expect("selection start marker is present");
        let end = page.rows[2]
            .text
            .find("column-0014")
            .expect("selection end marker is present")
            + "column-0014".len();
        window.selection = Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 0,
                byte_offset: start,
            },
            focus: LogTextPosition {
                display_row: 2,
                byte_offset: end,
            },
        });

        snapshot_window_after_horizontal_scroll(
            window,
            "pod_logs/pod_log_viewer_wide_multiline_selection_after_scroll_snapshot/wide_multiline_selection_after_scroll",
            1_000.0,
        );
    }

    #[test]
    fn pod_log_viewer_utf8_grapheme_selection_snapshot() {
        let line = "INFO  café e\u{301} 日本語 👩‍💻 family: 👨‍👩‍👧‍👦  ".repeat(12);
        let mut window = log_window(&[&line]);
        let text = &window.pages[&LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: 0,
        }]
            .rows[0]
            .text;
        let selected_start = text
            .find("e\u{301}")
            .expect("combining sequence is present");
        let selected_end = selected_start + "e\u{301} 日本語 👩‍💻".len();
        window.selection = Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 0,
                byte_offset: selected_start,
            },
            focus: LogTextPosition {
                display_row: 0,
                byte_offset: selected_end,
            },
        });

        snapshot_window(
            window,
            "pod_logs/pod_log_viewer_utf8_grapheme_selection_snapshot/utf8_grapheme_selection",
        );
    }

    #[test]
    fn pod_log_viewer_loading_placeholder_snapshot() {
        let mut window = log_window(&[]);
        window.total_lines = 100;
        window.initial_page_loaded = false;
        snapshot_initial_spool_window(
            window,
            "pod_logs/pod_log_viewer_loading_placeholder_snapshot/loading_placeholder",
            LogDisplayOptions {
                show_line_numbers: true,
                ..LogDisplayOptions::default()
            },
        );
    }

    #[test]
    fn pod_log_viewer_renders_live_tail_rows_while_disk_page_catches_up_snapshot() {
        let mut window = log_window(&[]);
        window.total_lines = 1;
        window.backfill_lines = Some(12_345);
        window.live_rows.insert(
            0,
            LogPageRow {
                display_row: 0,
                line_index: 0,
                timestamp: None,
                text: "live row arrives without a placeholder".into(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            },
        );
        snapshot_window(
            window,
            "pod_logs/pod_log_viewer_renders_live_tail_rows_while_disk_page_catches_up_snapshot/live_tail_rows_while_disk_page_catches_up",
        );
    }

    #[test]
    fn command_f_focuses_log_search_input() {
        let window = Rc::new(RefCell::new(log_window(&["one line"])));
        let window_for_ui = window.clone();
        let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
        let display_options_for_ui = display_options.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window_for_ui.borrow_mut(),
                &mut display_options_for_ui.borrow_mut(),
                &log_store,
                &mut close_requested,
            );
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
        harness.step();
        harness.event(egui::Event::Text("find me".into()));
        harness.step();

        assert_eq!(window.borrow().search.query, "find me");
    }

    #[test]
    fn status_label_compacts_history_spool_progress() {
        let mut window = log_window(&[]);
        window.backfill_lines = Some(12_345);
        assert_eq!(status_label(&window), "Following · backfill 12.3k");
        window.backfill_lines = Some(1_250_000);
        assert_eq!(status_label(&window), "Following · backfill 1.2M");
    }

    #[test]
    fn pod_log_viewer_rebase_keeps_scrolled_wide_text_in_place() {
        let live_lines = (0..LOG_PAGE_SIZE)
            .map(|line_index| format!("record {line_index:03} :: ").repeat(32))
            .collect::<Vec<_>>();
        let live_line_refs = live_lines.iter().map(String::as_str).collect::<Vec<_>>();

        // The worker only controls stream lifecycle now; the storage service
        // owns log data. Drive both boundaries explicitly so this test fixes
        // the exact frame in which the source swap is rendered.
        let mut state = UiState::default();
        let mut commands = Vec::new();
        state.open_pod_log_window(
            1,
            "api-0".into(),
            Some("default".into()),
            PodLogContainer {
                name: "api".into(),
                kind: ContainerKind::App,
                image: None,
            },
            &mut commands,
        );
        let mut worker = MockWorker {
            results: VecDeque::from([Box::new(crate::worker::PodLogStreamStarted {
                log_window_id: 1,
            }) as crate::worker::WorkerResultBox]),
            commands: Vec::new(),
        };
        let _ = state.update(&mut worker);
        state.log_windows.insert(1, log_window(&live_line_refs));

        let state = Rc::new(RefCell::new(state));
        let rendered_scroll_offset = Rc::new(RefCell::new(egui::Vec2::ZERO));
        let rendered_scroll_id = Rc::new(RefCell::new(None));
        let state_for_ui = state.clone();
        let rendered_scroll_offset_for_ui = rendered_scroll_offset.clone();
        let rendered_scroll_id_for_ui = rendered_scroll_id.clone();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            let mut state = state_for_ui.borrow_mut();
            let UiState {
                log_windows,
                log_display_options,
                ..
            } = &mut *state;
            let window = log_windows.get_mut(&1).expect("log window exists");
            let output = show_log_window_with_scroll_state(
                ctx,
                window,
                log_display_options,
                &log_store,
                &mut close_requested,
            );
            *rendered_scroll_offset_for_ui.borrow_mut() = output.state.offset;
            *rendered_scroll_id_for_ui.borrow_mut() = Some(output.id);
        });
        components::test_support::setup_egui(&mut harness);
        harness.run_steps(2);
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(egui::pos2(400.0, 180.0)));
        harness.step();
        harness.input_mut().events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::Vec2::ZERO,
            phase: egui::TouchPhase::Start,
            modifiers: egui::Modifiers::default(),
        });
        harness.input_mut().events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(-600.0, -700.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        });
        harness.run_steps(3);

        // Snap the fixture to a physical-pixel x offset before capturing the
        // two frames. Real scroll input can end between pixels; this isolates
        // source-rebase behavior from wheel-event rounding.
        let scroll_id = rendered_scroll_id
            .borrow()
            .expect("log scroll area was rendered");
        let mut scroll_state = egui::scroll_area::State::load(&harness.ctx, scroll_id)
            .expect("log scroll state was persisted");
        scroll_state.offset.x = 600.0;
        scroll_state.store(&harness.ctx, scroll_id);
        harness.run_steps(2);

        let before_offset = *rendered_scroll_offset.borrow();
        let old_visible_row = state.borrow().log_windows[&1].visible_top_display_row;
        assert!(
            before_offset.x > 0.0,
            "the test must exercise horizontal scroll"
        );
        assert!(
            old_visible_row > 0,
            "the test must exercise vertical scroll"
        );
        harness.ui_harness(
            "pod_logs/pod_log_viewer_rebase_keeps_scrolled_wide_text_in_place/rebase_before",
        );

        // A full history request returned 100 older records plus the complete
        // initial tail. The visible tail record therefore moves down by 100
        // logical rows, but its rendered position must not move.
        state
            .borrow_mut()
            .apply_log_store_result(LogStoreResult::Rebased {
                window_id: 1,
                total_lines: LOG_PAGE_SIZE + 100,
                history_lines: LOG_PAGE_SIZE + 100,
                live_start: LOG_PAGE_SIZE,
            });
        state
            .borrow_mut()
            .apply_log_store_result(LogStoreResult::PageLoaded {
                window_id: 1,
                generation: 0,
                filter_matches: false,
                page_start: LOG_PAGE_SIZE,
                total_rows: LOG_PAGE_SIZE + 100,
                rows: (LOG_PAGE_SIZE..LOG_PAGE_SIZE + 100)
                    .map(|display_row| {
                        let live_index = display_row - 100;
                        LogPageRow {
                            display_row,
                            line_index: display_row,
                            timestamp: None,
                            text: live_lines[live_index].clone(),
                            style_spans: Vec::new(),
                            match_ranges: Vec::new(),
                        }
                    })
                    .collect(),
            });
        harness.run_steps(3);

        let after_offset = *rendered_scroll_offset.borrow();
        let expected_row = old_visible_row + 100;
        assert_eq!(
            state.borrow().log_windows[&1].visible_top_display_row,
            expected_row,
            "the rendered anchor must refer to the same record after rebasing"
        );
        assert!(
            (after_offset.x - before_offset.x).abs() <= 0.1,
            "rebasing must preserve the horizontal scroll offset: before={before_offset:?}, after={after_offset:?}"
        );
        harness.ui_harness(
            "pod_logs/pod_log_viewer_rebase_keeps_scrolled_wide_text_in_place/rebase_after",
        );

        // Supply the newly prepended page, then scroll up into it. This
        // verifies that the post-rebase cache can move in the opposite
        // direction without losing the horizontal position or showing the
        // old tail at the wrong logical index.
        state
            .borrow_mut()
            .apply_log_store_result(LogStoreResult::PageLoaded {
                window_id: 1,
                generation: 0,
                filter_matches: false,
                page_start: 0,
                total_rows: LOG_PAGE_SIZE + 100,
                rows: (0..LOG_PAGE_SIZE)
                    .map(|display_row| {
                        let text = if display_row < 100 {
                            format!("history {display_row:03} :: ").repeat(32)
                        } else {
                            live_lines[display_row - 100].clone()
                        };
                        LogPageRow {
                            display_row,
                            line_index: display_row,
                            timestamp: None,
                            text,
                            style_spans: Vec::new(),
                            match_ranges: Vec::new(),
                        }
                    })
                    .collect(),
            });
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(egui::pos2(400.0, 180.0)));
        harness.step();
        harness.input_mut().events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 10_000.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        });
        harness.run_steps(3);
        let history_scroll_offset = *rendered_scroll_offset.borrow();
        assert!(
            state.borrow().log_windows[&1].visible_top_display_row < 100,
            "upward scrolling must reach the prepended history segment"
        );
        assert!(
            (history_scroll_offset.x - before_offset.x).abs() <= 0.1,
            "upward scrolling through history must retain the horizontal offset"
        );
        harness.ui_harness("pod_logs/pod_log_viewer_rebase_keeps_scrolled_wide_text_in_place/rebase_history_after_scroll");
    }

    #[test]
    fn pod_log_viewer_filter_active_snapshot() {
        let mut window = log_window(&[
            "2026-08-08T15:22:17.143Z  INFO  server: listening on 0.0.0.0:8080",
            "2026-08-08T15:22:18.021Z  INFO  http: GET /healthz 200 2ms",
            "2026-08-08T15:22:19.403Z  INFO  http: GET /v1/widgets 200 14ms",
            "2026-08-08T15:22:21.687Z  WARN  cache: refreshing stale entry",
        ]);
        window.search.query = "http".to_owned();
        window.search.filter_matches = true;
        window.search.match_count = 2;
        window.insert_page(
            LogPageKey {
                generation: 0,
                filter_matches: true,
                page_start: 0,
            },
            [1, 2]
                .into_iter()
                .enumerate()
                .map(|(display_row, line_index)| {
                    let text = window.pages[&LogPageKey {
                        generation: 0,
                        filter_matches: false,
                        page_start: 0,
                    }]
                        .rows[line_index]
                        .text
                        .clone();
                    let style_spans = window.pages[&LogPageKey {
                        generation: 0,
                        filter_matches: false,
                        page_start: 0,
                    }]
                        .rows[line_index]
                        .style_spans
                        .clone();
                    LogPageRow {
                        display_row,
                        line_index,
                        timestamp: window.pages[&LogPageKey {
                            generation: 0,
                            filter_matches: false,
                            page_start: 0,
                        }]
                            .rows[line_index]
                            .timestamp
                            .clone(),
                        style_spans,
                        match_ranges: regex::Regex::new("(?i)http")
                            .expect("valid test matcher")
                            .find_iter(&text)
                            .map(|range| (range.start(), range.end()))
                            .collect(),
                        text,
                    }
                })
                .collect(),
        );
        snapshot_window(
            window,
            "pod_logs/pod_log_viewer_filter_active_snapshot/filter_active",
        );
    }

    #[test]
    fn pod_log_viewer_stream_failure_snapshot() {
        let mut window = log_window(&[
            "2026-08-08T15:22:17.143Z  INFO  server: listening on 0.0.0.0:8080",
            "2026-08-08T15:22:21.687Z  WARN  retrying log stream",
        ]);
        window.status = PodLogStatus::Failed(
            "The Kubernetes API closed the log stream unexpectedly".to_owned(),
        );

        snapshot_window(
            window,
            "pod_logs/pod_log_viewer_stream_failure_snapshot/stream_failed",
        );
    }

    #[test]
    fn pod_log_viewer_invalid_regex_snapshot() {
        let mut window = log_window(&["api ready", "worker ready"]);
        window.search.query = "[".to_owned();
        window.search.regex_mode = true;
        window.search.error = Some("unclosed character class".to_owned());

        snapshot_window(
            window,
            "pod_logs/pod_log_viewer_invalid_regex_snapshot/invalid_regex",
        );
    }

    fn add_match_ranges(window: &mut PodLogWindowState, filter_matches: bool) {
        let key = LogPageKey {
            generation: 0,
            filter_matches,
            page_start: 0,
        };
        let matcher = regex::Regex::new("(?i)http").expect("valid test matcher");
        let page = window.pages.get_mut(&key).expect("test page exists");
        for row in &mut page.rows {
            row.match_ranges = matcher
                .find_iter(&row.text)
                .map(|range| (range.start(), range.end()))
                .collect();
        }
    }

    fn snapshot_window(window: PodLogWindowState, name: &str) {
        snapshot_window_with_display_options(window, name, LogDisplayOptions::default());
    }

    fn snapshot_window_with_display_options(
        window: PodLogWindowState,
        name: &str,
        display_options: LogDisplayOptions,
    ) {
        snapshot_window_after_horizontal_scroll_with_display_options(
            window,
            name,
            0.0,
            display_options,
            true,
        );
    }

    fn snapshot_initial_spool_window(
        window: PodLogWindowState,
        name: &str,
        display_options: LogDisplayOptions,
    ) {
        snapshot_window_after_horizontal_scroll_with_display_options(
            window,
            name,
            0.0,
            display_options,
            false,
        );
    }

    fn snapshot_window_after_horizontal_scroll(
        window: PodLogWindowState,
        name: &str,
        horizontal_offset: f32,
    ) {
        snapshot_window_after_horizontal_scroll_with_display_options(
            window,
            name,
            horizontal_offset,
            LogDisplayOptions::default(),
            true,
        );
    }

    fn snapshot_window_after_horizontal_scroll_with_display_options(
        window: PodLogWindowState,
        name: &str,
        horizontal_offset: f32,
        mut display_options: LogDisplayOptions,
        settle: bool,
    ) {
        let mut window = window;
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build_ui(move |ctx| {
            show_log_window(
                ctx,
                &mut window,
                &mut display_options,
                &log_store,
                &mut close_requested,
            )
        });
        components::test_support::setup_egui(&mut harness);
        if settle {
            harness.run();
        } else {
            // The spinner repaints continuously. A fixed frame is sufficient
            // for this visual regression without asking the harness to settle.
            harness.step();
        }
        if horizontal_offset > 0.0 {
            harness
                .input_mut()
                .events
                .push(egui::Event::PointerMoved(egui::pos2(400.0, 100.0)));
            harness.step();
            harness.input_mut().events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(-horizontal_offset, 0.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            });
            harness.step();
            // The wheel event updates ScrollArea state during this frame. Draw
            // a couple more frames so the virtual text fragment observes the
            // resulting offset before snapshotting.
            harness.run_steps(2);
        }
        harness.ui_harness(name);
    }

    fn wait_for_store_result(
        service: &LogStoreService,
        matches: impl Fn(&LogStoreResult) -> bool,
    ) -> LogStoreResult {
        let start = Instant::now();
        loop {
            if let Some(result) = service.try_next_result()
                && matches(&result)
            {
                return result;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "timed out waiting for log-store result"
            );
            std::thread::yield_now();
        }
    }
}
