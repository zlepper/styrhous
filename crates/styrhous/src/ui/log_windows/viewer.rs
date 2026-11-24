use super::*;
use crate::log_level::parse_log_level;
use components::tooltip::TooltipExt;
use components::{CodeColor, CodeSpan, EmptyState, LogLine, ProblemTick, ProblemTicks};

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
    show_log_toolbar(&ctx, window, display_options, log_store);
    step_problems_with_keys(&ctx, window);

    let frame = egui::Frame::new()
        .fill(theme.surface.canvas)
        .inner_margin(egui::Margin {
            left: size::LOG_WINDOW_PADDING_X as i8,
            right: size::LOG_WINDOW_PADDING_X as i8,
            top: size::LOG_WINDOW_PADDING_TOP as i8,
            bottom: 0,
        });
    egui::CentralPanel::default()
        .frame(frame)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            show_log_header(ui, window, display_options);
            ui.add_space(size::LOG_HEADER_GAP);
            show_volume_strip(ui, window, display_options, log_store);
            ui.add_space(size::LOG_STRIP_GAP);
            let panel = egui::Rect::from_min_max(
                ui.cursor().min,
                egui::pos2(
                    ui.max_rect().right(),
                    (ui.max_rect().bottom() - size::FLOATING_TOOLBAR_RESERVE)
                        .max(ui.cursor().min.y),
                ),
            );
            ui.painter()
                .rect_filled(panel, components::design::radius::CARD, theme.code.surface);
            let mut lines = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_max(
                        panel.min + egui::vec2(0.0, size::LOG_PANEL_PADDING_TOP),
                        panel.max,
                    ))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            lines.set_clip_rect(panel.intersect(ui.clip_rect()));
            lines.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let output = show_log_lines(&mut lines, window, display_options, log_store);
            // The rows run to the panel's edge like the board's, so a last
            // row only partly in view is cut off by it; the square clip
            // is rounded again by covering what lies outside the corners.
            cover_outside_bottom_corners(ui.painter(), panel, theme.surface.canvas);
            if !initial_spool_is_pending(window) && !log_is_unavailable(window) {
                show_problem_ticks(ui, panel, window, &output);
            }
            ui.allocate_rect(panel, egui::Sense::hover());
            output
        })
        .inner
}

/// The focus id of the window's text canvas, which the caret lives on.
fn caret_focus_id(window_id: u64) -> egui::Id {
    egui::Id::new(("pod-log-caret", window_id))
}

/// `[` and `]` step back and forth through the errors and warnings.
fn step_problems_with_keys(ctx: &egui::Context, window: &mut PodLogWindowState) {
    // Typing in the search field must reach it; the log text itself is fine.
    let caret_focus_id = caret_focus_id(window.id);
    if ctx
        .memory(|memory| memory.focused())
        .is_some_and(|focused| focused != caret_focus_id)
    {
        return;
    }
    let (previous, next) = ctx.input_mut(|input| {
        (
            input.consume_key(egui::Modifiers::NONE, egui::Key::OpenBracket),
            input.consume_key(egui::Modifiers::NONE, egui::Key::CloseBracket),
        )
    });
    if previous {
        step_problem(window, false);
    }
    if next {
        step_problem(window, true);
    }
}

/// The track beside the lines: the part on screen, and a tick for every
/// error and warning.
fn show_problem_ticks(
    ui: &mut egui::Ui,
    panel: egui::Rect,
    window: &mut PodLogWindowState,
    output: &egui::scroll_area::ScrollAreaOutput<()>,
) {
    let count = displayed_line_count(window).max(1) as f32;
    let rows = problem_rows(window);
    let ticks: Vec<_> = rows
        .iter()
        .map(|(row, problem)| ProblemTick {
            position: *row as f32 / count,
            tone: match problem {
                Problem::Error => Tone::Danger,
                Problem::Warning => Tone::Warning,
            },
        })
        .collect();
    let track = egui::Rect::from_min_max(
        egui::pos2(
            panel.right() - size::LOG_TICKS_INSET - size::PROBLEM_TRACK,
            panel.top() + size::LOG_TICKS_INSET,
        ),
        egui::pos2(
            panel.right() - size::LOG_TICKS_INSET,
            panel.bottom() - size::LOG_TICKS_INSET,
        ),
    );
    let content = output.content_size.y.max(1.0);
    let visible = (output.inner_rect.height() / content).clamp(0.0, 1.0);
    let start = (output.state.offset.y / content).clamp(0.0, 1.0 - visible);
    // The track floats over the lines like a scroll bar, so it is a layer of
    // its own rather than a sibling of their text.
    let response = egui::Area::new(egui::Id::new(("pod-log-problems", window.id)))
        .order(egui::Order::Middle)
        .fixed_pos(track.min)
        .constrain(false)
        .show(ui.ctx(), |ui| {
            ProblemTicks::new(&ticks)
                .id_salt(("pod-log-problems", window.id))
                .thumb(start, visible)
                .draggable()
                .show_in(ui, track)
        })
        .inner;
    if let Some(tick) = response.clicked {
        jump_to_display_row(window, rows[tick].0);
    } else if let Some(fraction) = response.scrubbed_to {
        let row = (fraction * count - window.visible_rows as f32 / 2.0).max(0.0) as usize;
        window.search.scroll_to_display_row = Some(row);
        window.search.active_display_row = None;
        window.tail.release();
    }
}

/// The empty state of a log that failed before it had any line.
fn show_unavailable_state(ui: &mut egui::Ui, window: &mut PodLogWindowState) {
    let previous = window.instance == PodLogInstance::Previous;
    let subject = match window.targets.as_slice() {
        [target] => target.pod_name.clone(),
        targets => format!("{} sources", targets.len()),
    };
    let title = if previous {
        format!("No previous logs for {subject}")
    } else {
        format!("Couldn’t read the logs of {subject}")
    };
    let description = match &window.status {
        PodLogStatus::Failed(error) => error.clone(),
        _ => String::new(),
    };
    let mut state = EmptyState::error(&title).on_code_surface(size::LOG_UNAVAILABLE_TOP);
    if !description.is_empty() {
        state = state.description(&description);
    }
    if previous {
        state = state.action_with_icon(icons::Icon::ArrowUturnLeft, "Show current logs");
    }
    if state.show(ui).is_some_and(|action| action.clicked()) {
        window.requested_instance = Some(PodLogInstance::Current);
    }
}

/// The empty state while the history of a log is being spooled to disk.
fn show_spooling_state(ui: &mut egui::Ui, total_lines: usize) {
    let received = format!(
        "{} {} received · newest shown when complete",
        components::search::format_exact_count(total_lines),
        if total_lines == 1 { "line" } else { "lines" }
    );
    EmptyState::loading("Spooling logs…")
        .description(&received)
        .on_code_surface(size::LOG_SPOOLING_TOP)
        .show(ui);
}

/// The footer under the last line of a short, filtered or reconnecting log.
fn show_log_footer(ui: &mut egui::Ui, window: &PodLogWindowState, rect: egui::Rect) {
    let theme = ui.tokens();
    let filtered = window.filter_is_active() && window.search.search_complete;
    let reconnecting = window
        .source_reconnects
        .keys()
        .map(|target| {
            (
                window
                    .source_label(Some(&target.display_name()))
                    .map_or_else(|| target.pod_name.clone(), str::to_owned),
                window
                    .source_reconnect_attempts
                    .get(target)
                    .copied()
                    .unwrap_or(1),
            )
        })
        .min();
    if !filtered && reconnecting.is_none() {
        return;
    }
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row.spacing_mut().item_spacing = egui::vec2(space::S8, 0.0);
    row.add_space(size::LOG_ROW_PADDING_X);
    let muted = theme.code.text_muted;
    if filtered {
        let (icon, _) =
            row.allocate_exact_size(egui::Vec2::splat(size::ICON_SM), egui::Sense::hover());
        icons::Icon::Funnel.paint(&row, icon, muted);
        let shown = format!(
            "Showing {} of {}",
            line_count_text(
                window.search.match_count,
                ["matching line", "matching lines"]
            ),
            components::search::format_count(window.total_lines)
        );
        type_scale::META.label(&mut row, &shown, muted);
    }
    if let Some((name, attempt)) = reconnecting {
        if filtered {
            type_scale::META.label(&mut row, "·", muted);
        }
        let (icon, _) =
            row.allocate_exact_size(egui::Vec2::splat(size::ICON_SM), egui::Sense::hover());
        icons::Icon::ExclamationTriangle.paint(&row, icon, theme.warning.text);
        type_scale::META.label(
            &mut row,
            &format!("{name} reconnecting (attempt {attempt})"),
            theme.warning.text,
        );
    }
}

/// The lines of the log, virtualised over the pages the store loads.
fn show_log_lines(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &LogDisplayOptions,
    log_store: &LogStoreService,
) -> egui::scroll_area::ScrollAreaOutput<()> {
    let ctx = ui.ctx().clone();
    let theme = ui.tokens();
    let pixels_per_point = ui.pixels_per_point();
    // Keep virtual row origins on the physical-pixel grid. Otherwise
    // equivalent rows at different logical indices can round to different
    // raster positions during a source rebase.
    let row_step = (size::LOG_ROW * pixels_per_point).round() / pixels_per_point;
    let row_height = row_step;
    let scroll_area_salt = egui::Id::new(("pod-log-lines", window.id));
    if initial_spool_is_pending(window) {
        request_page_for_display_row(window, log_store, window.total_lines.saturating_sub(1));
        show_spooling_state(ui, window.total_lines);
        return components::scroll::both()
            .id_salt(scroll_area_salt)
            .show_rows(ui, row_height, 0, |_ui, _rows| {});
    }
    if log_is_unavailable(window) {
        show_unavailable_state(ui, window);
        return components::scroll::both()
            .id_salt(scroll_area_salt)
            .show_rows(ui, row_height, 0, |_ui, _rows| {});
    }

    let layout = RowLayout::new(ui, window, *display_options);
    let character_width = layout.character_width;
    let display_count = displayed_line_count(window);
    // `ScrollArea::id_salt` hashes the salt into an `IdSalt` before it
    // scopes it to this UI. Mirror both steps when reading its state.
    let scroll_id = ui.make_persistent_id(egui::IdSalt::new(scroll_area_salt));
    let scroll_state = egui::scroll_area::State::load(&ctx, scroll_id);
    let horizontal_offset = scroll_state.as_ref().map_or(0.0, |state| state.offset.x);
    // The virtual fragment renderer rounds its content origin to the
    // physical pixel grid. Store that same normalized value so a rebase
    // cannot turn a fractional wheel offset into a one-pixel horizontal text
    // jump on its next frame.
    let horizontal_offset = (horizontal_offset * pixels_per_point).round() / pixels_per_point;
    let vertical_offset = scroll_state.as_ref().map_or(0.0, |state| state.offset.y);
    window.visible_top_display_row = (vertical_offset / row_step).max(0.0).floor() as usize;
    window.visible_rows = (ui.available_height() / row_step).ceil() as usize;
    let viewport_width = ui.available_width();
    let caret_focus_id = caret_focus_id(window.id);
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
        if let Some(offset) =
            caret_horizontal_offset(window, &layout, horizontal_offset, viewport_width)
        {
            let vertical =
                caret_vertical_offset(window, vertical_offset, ui.available_height(), row_step);
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
    // clamped while the area is still measuring or loading its target page,
    // which made the toolbar arrows appear to do nothing.
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
        .or_else(|| tail_scroll_row.map(|row| egui::vec2(horizontal_offset, row as f32 * row_step)))
        .or(caret_scroll_offset);
    let mut scroll_area = components::scroll::both_clear_of_corner(ui)
        .id_salt(scroll_area_salt)
        .auto_shrink([false, false])
        // A wide page can arrive after placeholders have already been
        // rendered. Reserving both bars from the first frame keeps that
        // discovery from changing the vertical viewport.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .horizontal_scroll_offset(horizontal_offset)
        // A focused text caret is an explicit request to inspect the current
        // records. Do not let new tail records carry it out of view between
        // keyboard frames.
        .stick_to_bottom(
            window.tail.is_following() && requested_offset.is_none() && !caret_has_focus,
        );
    if let Some(offset) = requested_offset {
        scroll_area = scroll_area.scroll_offset(offset);
    }
    let mut saw_level = false;
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
                filter_matches: window.filter_is_active(),
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
                    if !window.filter_is_active() {
                        window
                            .live_rows
                            .get(&display_row)
                            .map(|row| (row, row.text.chars().count()))
                    } else {
                        None
                    }
                });
            if let Some((row, max_text_columns)) = cached_row {
                let level = parse_log_level(&row.text);
                saw_level |= level.is_some();
                let hidden = hidden_level_bytes(row, level);
                let prefix_width =
                    layout.prefix_width(row.source.is_some() && layout.source_width.is_some());
                // The badge replaces a leading level word, so the message starts
                // after it; columns still count from the start of the text.
                let fragment = visible_text_fragment(
                    &row.text[hidden..],
                    (horizontal_offset - prefix_width).max(0.0),
                    viewport_width,
                    character_width,
                );
                row_content_width = Some(
                    prefix_width + max_text_columns.saturating_sub(hidden) as f32 * character_width,
                );
                let byte_range =
                    fragment.byte_range.start + hidden..fragment.byte_range.end + hidden;
                let current = window.search.active_display_row == Some(display_row);
                let marks = MessageMarks {
                    style_spans: &row.style_spans,
                    matches: &row.match_ranges,
                    active: current.then(|| row.match_ranges.first().copied()).flatten(),
                    selection: window
                        .selection
                        .and_then(|selection| selection.range_for_row(display_row, row.text.len())),
                    render_ansi: display_options.render_ansi,
                };
                let spans = message_spans(&row.text, byte_range.clone(), &marks, &theme);
                let time = layout.time_text(row);
                let shown = layout
                    .line(window, row, time.as_deref(), level, spans, fragment.start_x)
                    .current(current)
                    .show(ui);
                let interaction_response = ui
                    .interact(
                        shown.response.rect,
                        egui::Id::new(("pod-log-line", window.id, display_row)),
                        egui::Sense::click_and_drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::Text);
                let interaction_response = match row.timestamp.as_deref().and_then(format_full_time)
                {
                    Some(full) => interaction_response.tooltip(full),
                    None => interaction_response,
                };
                ui.ctx()
                    .accesskit_node_builder(interaction_response.id, |builder| {
                        builder.set_label(format!("Pod log line {}", display_row + 1));
                    });
                selection_update = selection_position(
                    &ctx,
                    display_row,
                    &row.text,
                    &interaction_response,
                    shown.message_left - hidden as f32 * character_width,
                    fragment.start_x,
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
                        hidden,
                        shown.response.rect,
                        shown.message_left,
                    ));
                }
            } else {
                show_loading_row(ui, &layout, display_row, !window.filter_is_active());
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
                    Some(caret_paint.as_ref().map_or(0, |(text, ..)| {
                        character_column_at_byte(text, position.byte_offset)
                    }));
                window.ensure_caret_visible = false;
                if window.tail.release() {
                    ctx.request_repaint();
                }
            }
            if let Some((text, byte_range, hidden, response_rect, text_left)) = caret_paint {
                paint_log_caret(
                    ui,
                    &ctx,
                    window,
                    caret_focus_id,
                    display_row,
                    &text,
                    &byte_range,
                    hidden,
                    response_rect,
                    text_left,
                    character_width,
                    row_height,
                );
            }
        }
    });
    if saw_level {
        window.level_column = true;
    }
    // A short log leaves room for a note under its last line.
    let used = display_count as f32 * row_step;
    if used + row_step <= output.inner_rect.height() {
        let footer = egui::Rect::from_min_size(
            output.inner_rect.min + egui::vec2(0.0, used + space::S8),
            egui::vec2(output.inner_rect.width(), row_step),
        );
        show_log_footer(ui, window, footer);
    }
    if let Some(row) = requested_scroll_row {
        if display_row_is_visible(row, row_step, &output) {
            window.search.scroll_to_display_row = None;
        } else {
            // Keep repainting until egui has accepted the requested offset.
            // This also covers the frame that replaces a virtual placeholder
            // with the requested disk page.
            ctx.request_repaint();
        }
    }
    update_tail_follow_state(&ctx, window, &output, requested_offset.is_none());
    output
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

/// The placeholder of a row whose page has not been read from disk yet.
fn show_loading_row(
    ui: &mut egui::Ui,
    layout: &RowLayout,
    display_row: usize,
    display_row_is_line_index: bool,
) {
    let mut line = LogLine::new([CodeSpan::new("Loading…", CodeColor::Muted)]);
    if display_row_is_line_index && layout.show_numbers {
        line = line
            .number(display_row + 1)
            .number_width(layout.number_width);
    }
    line.show(ui);
}

/// Cover what lies outside the rounded bottom corners of `panel` with
/// `outside`: egui clips to rectangles only, so rows that run to the panel's
/// edge are rounded off afterwards.
fn cover_outside_bottom_corners(
    painter: &egui::Painter,
    panel: egui::Rect,
    outside: egui::Color32,
) {
    let radius = f32::from(components::design::radius::CARD);
    // A panel too small for its rounding has no corners to cover.
    if panel.width() >= 2.0 * radius && panel.height() >= 2.0 * radius {
        painter.add(egui::Shape::mesh(outside_bottom_corners(panel, outside)));
    }
}

/// The two slivers between the bottom corners of `panel` and its rounding.
fn outside_bottom_corners(panel: egui::Rect, outside: egui::Color32) -> egui::Mesh {
    use std::f32::consts::{FRAC_PI_2, PI};
    const SEGMENTS: usize = 24;
    let radius = f32::from(components::design::radius::CARD);
    let mut mesh = egui::Mesh::default();
    // Each corner is a fan from the square's corner over the quarter circle.
    for (corner, towards_centre, first_angle, direction) in [
        (panel.left_bottom(), egui::vec2(radius, -radius), PI, -1.0),
        (panel.right_bottom(), egui::vec2(-radius, -radius), 0.0, 1.0),
    ] {
        let centre = corner + towards_centre;
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(corner, outside);
        for step in 0..=SEGMENTS {
            let angle = first_angle + direction * FRAC_PI_2 * step as f32 / SEGMENTS as f32;
            mesh.colored_vertex(
                centre + radius * egui::vec2(angle.cos(), angle.sin()),
                outside,
            );
        }
        for step in 0..SEGMENTS as u32 {
            mesh.add_triangle(base, base + 1 + step, base + 2 + step);
        }
    }
    mesh
}

#[cfg(test)]
mod corner_tests {
    use super::*;

    fn triangle_area(mesh: &egui::Mesh) -> f32 {
        mesh.indices
            .chunks(3)
            .map(|triangle| {
                let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[triangle[i] as usize].pos);
                ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs() / 2.0
            })
            .sum()
    }

    #[test]
    fn the_corner_covers_stay_between_the_corners_and_their_rounding() {
        let panel = egui::Rect::from_min_max(egui::pos2(28.0, 200.0), egui::pos2(500.0, 400.0));
        let mesh = outside_bottom_corners(panel, egui::Color32::PLACEHOLDER);
        let radius = f32::from(components::design::radius::CARD);
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| panel.expand(1e-3).contains(vertex.pos)),
            "nothing is painted outside the panel"
        );
        let sliver = radius * radius * (1.0 - std::f32::consts::FRAC_PI_4);
        assert!(
            (triangle_area(&mesh) - 2.0 * sliver).abs() < 0.5,
            "two slivers of the square lie outside the quarter circles, got {}",
            triangle_area(&mesh)
        );
        // The middle of the bottom edge and the interior stay uncovered.
        for (point, covered) in [
            (egui::pos2(28.5, 399.5), true),
            (egui::pos2(499.5, 399.5), true),
            (egui::pos2(264.0, 399.5), false),
            (egui::pos2(28.0 + radius, 400.0 - radius), false),
        ] {
            let hit = mesh.indices.chunks(3).any(|triangle| {
                let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[triangle[i] as usize].pos);
                let sign = |p: egui::Pos2, q: egui::Pos2, r: egui::Pos2| {
                    (p.x - r.x) * (q.y - r.y) - (q.x - r.x) * (p.y - r.y)
                };
                let (d1, d2, d3) = (sign(point, a, b), sign(point, b, c), sign(point, c, a));
                !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
            });
            assert_eq!(hit, covered, "{point:?}");
        }
    }
}
