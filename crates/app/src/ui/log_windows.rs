use super::state::{LogDisplayOptions, LogPageKey, PodLogStatus, PodLogWindowState, UiState};
use crate::ansi::AnsiStyleSpan;
use crate::log_store::LogStoreService;
use crate::worker::WorkerCommand;
use anstyle::{Ansi256Color, AnsiColor, Color, Effects, RgbColor, Style};
use components::colors::{SUCCESS, TABLE_BORDER, TOOLBAR_BACKGROUND, gray};
use components::design::{radius, spacing, status, surface, typography};
use components::{PointingHand, TailwindSearchInput, icons};
use std::time::{Duration, Instant};

/// Render native, independent Pod log windows and stop both the Kubernetes
/// stream and the independent disk store when a window is closed.
pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    log_store: &LogStoreService,
    commands_to_send: &mut Vec<WorkerCommand>,
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
            |window_ctx, _| {
                close_requested = window_ctx.input(|input| input.viewport().close_requested());
                show_log_window(
                    window_ctx,
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
        commands_to_send.push(WorkerCommand::StopPodLogStream {
            cluster_key,
            log_window_id: id,
        });
    }
}

pub(super) fn show_log_window(
    ctx: &egui::Context,
    window: &mut PodLogWindowState,
    display_options: &mut LogDisplayOptions,
    log_store: &LogStoreService,
    _close_requested: &mut bool,
) {
    sync_search(ctx, window, log_store);
    egui::TopBottomPanel::top("pod-log-header")
        .exact_height(52.0)
        .frame(
            egui::Frame::new()
                .fill(TOOLBAR_BACKGROUND)
                .stroke(egui::Stroke::new(1.0, TABLE_BORDER))
                .inner_margin(egui::Margin::symmetric(
                    spacing::XL as i8,
                    spacing::SM as i8,
                )),
        )
        .show(ctx, |ui| {
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
                    show_log_search_controls(ctx, ui, window, display_options, log_store)
                });
            });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(surface::TERMINAL_BACKGROUND)
                .inner_margin(egui::Margin::same(spacing::LG as i8)),
        )
        .show(ctx, |ui| {
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            let row_step = row_height + ui.spacing().item_spacing.y;
            let display_count = displayed_line_count(window);
            let requested_offset = window
                .search
                .scroll_to_display_row
                .take()
                .map(|row| egui::vec2(0.0, row as f32 * row_step));
            let mut scroll_area = egui::ScrollArea::both()
                .id_salt(("pod-log-lines", window.id))
                .auto_shrink([false, false])
                .stick_to_bottom(requested_offset.is_none());
            if let Some(offset) = requested_offset {
                scroll_area = scroll_area.scroll_offset(offset);
            }
            scroll_area.show_rows(ui, row_height, display_count, |ui, rows| {
                for display_row in rows {
                    request_page_for_display_row(window, log_store, display_row);
                    let page_start = display_row / window.page_size * window.page_size;
                    let key = LogPageKey {
                        generation: window.search.generation,
                        filter_matches: filter_is_active(window),
                        page_start,
                    };
                    let row_offset = display_row - page_start;
                    if let Some(row) = window
                        .pages
                        .get(&key)
                        .and_then(|page| page.rows.get(row_offset))
                    {
                        ui.add(
                            egui::Label::new(log_line_layout_job(
                                row.line_index,
                                row.timestamp.as_deref(),
                                &row.text,
                                &row.style_spans,
                                &row.match_ranges,
                                *display_options,
                            ))
                            .extend()
                            .selectable(true),
                        );
                    } else {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Loading…")
                                    .monospace()
                                    .color(gray::_500),
                            )
                            .extend(),
                        );
                    }
                }
            });
        });
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
                            let previous_line = navigation_button(
                                ui,
                                icons::arrow_up_icon(),
                                "Previous displayed line",
                            );
                            ui.separator();
                            let previous_match = navigation_button(
                                ui,
                                icons::arrow_left_icon(),
                                "Previous matching line",
                            );
                            ui.separator();
                            let next_match = navigation_button(
                                ui,
                                icons::arrow_right_icon(),
                                "Next matching line",
                            );
                            ui.separator();
                            let next_line = navigation_button(
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

fn navigation_button(ui: &mut egui::Ui, icon: egui::Image<'static>, label: &str) -> bool {
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
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label.to_owned())
    });
    response.on_hover_text(label).clicked()
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

fn log_line_layout_job(
    line_index: usize,
    timestamp: Option<&str>,
    line: &str,
    style_spans: &[AnsiStyleSpan],
    ranges: &[(usize, usize)],
    display_options: LogDisplayOptions,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap = egui::text::TextWrapping::no_max_width();
    let number = egui::TextFormat {
        font_id: egui::FontId::monospace(14.0),
        color: egui::Color32::from_rgb(156, 163, 175),
        ..Default::default()
    };
    let text = egui::TextFormat {
        font_id: egui::FontId::monospace(14.0),
        color: egui::Color32::from_rgb(229, 231, 235),
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
            format.background = egui::Color32::from_rgb(120, 53, 15);
        }
        job.append(&line[start..end], 0.0, format);
    }
    job
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
    if !window.search.query.is_empty() && !window.search.search_complete {
        return format!("Searching… {} matches", window.search.match_count);
    }
    match &window.status {
        PodLogStatus::Connecting => "Connecting…".to_owned(),
        PodLogStatus::Following => "Following".to_owned(),
        PodLogStatus::Finished => "Stream finished".to_owned(),
        PodLogStatus::Failed(error) => format!("Stream failed: {error}"),
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
    use egui_kittest::{Harness, kittest::Queryable};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn log_window(lines: &[&str]) -> PodLogWindowState {
        let mut window = PodLogWindowState {
            id: 1,
            cluster_key: 1,
            namespace: "default".to_owned(),
            pod_name: "api-0".to_owned(),
            container: PodLogContainer {
                name: "api".to_owned(),
                kind: ContainerKind::App,
            },
            total_lines: lines.len(),
            pages: Default::default(),
            page_order: Default::default(),
            page_cache_bytes: 0,
            page_cache_limit: PodLogWindowState::DEFAULT_PAGE_CACHE_LIMIT,
            page_size: LOG_PAGE_SIZE,
            pending_pages: Default::default(),
            store_opened: true,
            status: PodLogStatus::Following,
            close_requested: false,
            search: Default::default(),
        };
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
        let _ = context.run(input(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                first_scroll_output = Some(show_wide_test_scroll_area(ui, &wide_line));
            });
        });

        let first_output = first_scroll_output.expect("scroll area was rendered");
        assert!(first_output.content_size.x > first_output.inner_rect.width());

        let mut scroll_input = input();
        scroll_input.events = vec![
            egui::Event::PointerMoved(first_output.inner_rect.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(-120.0, 0.0),
                modifiers: egui::Modifiers::default(),
            },
        ];
        let mut second_scroll_output = None;
        let _ = context.run(scroll_input, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                second_scroll_output = Some(show_wide_test_scroll_area(ui, &wide_line));
            });
        });

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
        egui::ScrollArea::both()
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
        let mut harness = Harness::builder().build(move |ctx| {
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
        snapshot_window(window, "pod_logs/viewer");
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
        snapshot_window(window, "pod_logs/filter_active");
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

        snapshot_window(window, "pod_logs/stream_failed");
    }

    #[test]
    fn pod_log_viewer_invalid_regex_snapshot() {
        let mut window = log_window(&["api ready", "worker ready"]);
        window.search.query = "[".to_owned();
        window.search.regex_mode = true;
        window.search.error = Some("unclosed character class".to_owned());

        snapshot_window(window, "pod_logs/invalid_regex");
    }

    fn add_match_ranges(window: &mut PodLogWindowState, filter_matches: bool) {
        let key = LogPageKey {
            generation: 0,
            filter_matches,
            page_start: 0,
        };
        let page = window.pages.get_mut(&key).expect("test page exists");
        for row in &mut page.rows {
            row.match_ranges = regex::Regex::new("(?i)http")
                .expect("valid test matcher")
                .find_iter(&row.text)
                .map(|range| (range.start(), range.end()))
                .collect();
        }
    }

    fn snapshot_window(window: PodLogWindowState, name: &str) {
        let mut window = window;
        let mut display_options = LogDisplayOptions::default();
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder().build(move |ctx| {
            show_log_window(
                ctx,
                &mut window,
                &mut display_options,
                &log_store,
                &mut close_requested,
            )
        });
        components::test_support::setup_egui(&mut harness);
        harness.run();
        harness.snapshot(name);
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
