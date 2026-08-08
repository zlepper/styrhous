use super::state::{PodLogStatus, PodLogWindowState, UiState};
use crate::worker::WorkerCommand;
use components::colors::{SUCCESS, TABLE_BORDER, TOOLBAR_BACKGROUND, gray};
use components::{TailwindSearchInput, icons};

const TOOLBAR_TEXT_SIZE: f32 = 16.0;

/// Render native, independent Pod log windows and stop streams for windows
/// closed by the user.
pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    let ids = ui_state.log_windows.keys().copied().collect::<Vec<_>>();
    for id in ids {
        let Some(window) = ui_state.log_windows.get_mut(&id) else {
            continue;
        };
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
                show_log_window(window_ctx, window, &mut close_requested);
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
        commands_to_send.push(WorkerCommand::StopPodLogStream {
            cluster_key,
            log_window_id: id,
        });
    }
}

fn show_log_window(
    ctx: &egui::Context,
    window: &mut PodLogWindowState,
    _close_requested: &mut bool,
) {
    egui::TopBottomPanel::top("pod-log-header")
        .exact_height(64.0)
        .frame(
            egui::Frame::new()
                .fill(TOOLBAR_BACKGROUND)
                .stroke(egui::Stroke::new(1.0, TABLE_BORDER))
                .inner_margin(egui::Margin::symmetric(24, 12)),
        )
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Logs")
                        .size(TOOLBAR_TEXT_SIZE)
                        .strong()
                        .color(gray::_900),
                );
                ui.add_space(18.0);
                ui.label(
                    egui::RichText::new(&window.pod_name)
                        .size(TOOLBAR_TEXT_SIZE)
                        .strong()
                        .color(gray::_900),
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!("Container: {}", window.container.name))
                        .size(TOOLBAR_TEXT_SIZE)
                        .color(gray::_600),
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("●")
                        .size(TOOLBAR_TEXT_SIZE)
                        .color(status_color(&window.status)),
                );
                ui.label(
                    egui::RichText::new(status_label(&window.status))
                        .size(TOOLBAR_TEXT_SIZE)
                        .color(gray::_600),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    show_log_search_controls(ui, window);
                });
            });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(10, 10, 11))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            refresh_log_search(window);
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            let row_step = row_height + ui.spacing().item_spacing.y;
            let display_line_count = displayed_line_count(window);
            let requested_offset = window
                .search
                .scroll_to_line
                .take()
                .and_then(|line| display_row_for_line(window, line))
                .map(|row| egui::vec2(0.0, row as f32 * row_step));
            let mut scroll_area = egui::ScrollArea::both()
                .id_salt(("pod-log-lines", window.id))
                .auto_shrink([false, false])
                .stick_to_bottom(requested_offset.is_none());
            if let Some(offset) = requested_offset {
                scroll_area = scroll_area.scroll_offset(offset);
            }
            let matcher = log_matcher(&window.search).ok().flatten();
            scroll_area.show_rows(ui, row_height, display_line_count, |ui, rows| {
                for display_row in rows {
                    let line_index = line_for_display_row(window, display_row);
                    let line = &window.lines[line_index];
                    ui.add(
                        egui::Label::new(log_line_layout_job(line_index, line, matcher.as_ref()))
                            .extend()
                            .selectable(true),
                    );
                }
            });
        });
}

fn show_log_search_controls(ui: &mut egui::Ui, window: &mut PodLogWindowState) {
    refresh_log_search(window);
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
        reset_log_search(window);
        refresh_log_search(window);
    }

    ui.add_space(8.0);
    let navigation = ui
        .allocate_ui_with_layout(
            egui::vec2(158.0, 34.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .stroke(egui::Stroke::new(1.0, gray::_300))
                    .corner_radius(egui::CornerRadius::same(6))
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
                            let filter = filter_button(ui, window.search.filter_matches);
                            (previous_line, previous_match, next_match, next_line, filter)
                        })
                        .inner
                    })
                    .inner
            },
        )
        .inner;
    if navigation.1 {
        advance_log_match(window, false);
    }
    if navigation.2 {
        advance_log_match(window, true);
    }
    if navigation.0 {
        advance_log_line(window, false);
    }
    if navigation.3 {
        advance_log_line(window, true);
    }
    if navigation.4 {
        window.search.filter_matches = !window.search.filter_matches;
    }
}

fn navigation_button(ui: &mut egui::Ui, icon: egui::Image<'static>, label: &str) -> bool {
    let response = ui.add_sized(
        egui::Vec2::splat(28.0),
        egui::Button::image(
            icon.fit_to_exact_size(egui::Vec2::splat(14.0))
                .tint(gray::_700),
        )
        .frame(false),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label.to_owned())
    });
    response.on_hover_text(label).clicked()
}

fn filter_button(ui: &mut egui::Ui, active: bool) -> bool {
    let response = ui.add_sized(
        egui::Vec2::splat(28.0),
        egui::Button::image(
            icons::funnel_icon()
                .fit_to_exact_size(egui::Vec2::splat(14.0))
                .tint(gray::_700),
        )
        .frame(false),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Checkbox,
            ui.is_enabled(),
            "Filter to matching lines",
        )
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
    response
        .on_hover_text("Show only lines matching the search")
        .clicked()
}

fn reset_log_search(window: &mut PodLogWindowState) {
    window.search.matching_lines.clear();
    window.search.indexed_line_count = 0;
    window.search.active_match = 0;
    window.search.active_line = None;
    window.search.scroll_to_line = None;
    window.search.error = None;
}

fn refresh_log_search(window: &mut PodLogWindowState) {
    if window.search.indexed_line_count > window.lines.len() {
        reset_log_search(window);
    }
    if window.search.query.is_empty() {
        window.search.matching_lines.clear();
        window.search.indexed_line_count = window.lines.len();
        window.search.active_match = 0;
        window.search.error = None;
        return;
    }

    let matcher = match log_matcher(&window.search) {
        Ok(Some(regex)) => regex,
        Ok(None) => return,
        Err(error) => {
            window.search.matching_lines.clear();
            window.search.indexed_line_count = window.lines.len();
            window.search.active_match = 0;
            window.search.error = Some(error.to_string());
            return;
        }
    };
    for line_index in window.search.indexed_line_count..window.lines.len() {
        let line = &window.lines[line_index];
        if matcher.is_match(line) {
            window.search.matching_lines.push(line_index);
        }
    }
    window.search.indexed_line_count = window.lines.len();
    window.search.error = None;
    if window.search.active_match >= window.search.matching_lines.len() {
        window.search.active_match = 0;
    }
}

fn log_matcher(
    search: &super::state::LogSearchState,
) -> Result<Option<regex::Regex>, regex::Error> {
    if search.query.is_empty() {
        return Ok(None);
    }
    let pattern = if search.regex_mode {
        search.query.clone()
    } else {
        regex::escape(&search.query)
    };
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .map(Some)
}

fn displayed_line_count(window: &PodLogWindowState) -> usize {
    if window.search.filter_matches && !window.search.query.is_empty() {
        window.search.matching_lines.len()
    } else {
        window.lines.len()
    }
}

fn line_for_display_row(window: &PodLogWindowState, display_row: usize) -> usize {
    if window.search.filter_matches && !window.search.query.is_empty() {
        window.search.matching_lines[display_row]
    } else {
        display_row
    }
}

fn display_row_for_line(window: &PodLogWindowState, line_index: usize) -> Option<usize> {
    if window.search.filter_matches && !window.search.query.is_empty() {
        window
            .search
            .matching_lines
            .iter()
            .position(|matching_line| *matching_line == line_index)
    } else {
        (line_index < window.lines.len()).then_some(line_index)
    }
}

fn log_line_layout_job(
    line_index: usize,
    line: &str,
    matcher: Option<&regex::Regex>,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let line_number_format = egui::TextFormat {
        font_id: egui::FontId::monospace(14.0),
        color: egui::Color32::from_rgb(156, 163, 175),
        ..Default::default()
    };
    let text_format = egui::TextFormat {
        font_id: egui::FontId::monospace(14.0),
        color: egui::Color32::from_rgb(229, 231, 235),
        ..Default::default()
    };
    let match_format = egui::TextFormat {
        font_id: egui::FontId::monospace(14.0),
        color: egui::Color32::from_rgb(254, 243, 199),
        background: egui::Color32::from_rgb(120, 53, 15),
        ..Default::default()
    };
    job.append(&format!("{line_index:>6}  "), 0.0, line_number_format);

    let mut cursor = 0;
    if let Some(matcher) = matcher {
        for matched in matcher.find_iter(line) {
            if matched.start() > cursor {
                job.append(&line[cursor..matched.start()], 0.0, text_format.clone());
            }
            job.append(
                &line[matched.start()..matched.end()],
                0.0,
                match_format.clone(),
            );
            cursor = matched.end();
        }
    }
    if cursor < line.len() {
        job.append(&line[cursor..], 0.0, text_format);
    }
    job
}

fn advance_log_match(window: &mut PodLogWindowState, forward: bool) {
    let matches = &window.search.matching_lines;
    if matches.is_empty() {
        return;
    }
    window.search.active_match = if forward {
        (window.search.active_match + 1) % matches.len()
    } else {
        (window.search.active_match + matches.len() - 1) % matches.len()
    };
    window.search.scroll_to_line = Some(matches[window.search.active_match]);
    window.search.active_line = window.search.scroll_to_line;
}

fn advance_log_line(window: &mut PodLogWindowState, forward: bool) {
    let displayed_line_count = displayed_line_count(window);
    if displayed_line_count == 0 {
        return;
    }
    let current_display_row = window
        .search
        .active_line
        .and_then(|line| display_row_for_line(window, line));
    let next_display_row = match (current_display_row, forward) {
        (Some(row), true) => (row + 1) % displayed_line_count,
        (Some(row), false) => (row + displayed_line_count - 1) % displayed_line_count,
        (None, true) => 0,
        (None, false) => displayed_line_count - 1,
    };
    let line = line_for_display_row(window, next_display_row);
    window.search.active_line = Some(line);
    window.search.scroll_to_line = Some(line);
}

fn status_label(status: &PodLogStatus) -> String {
    match status {
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
        PodLogStatus::Failed(_) => egui::Color32::from_rgb(185, 28, 28),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minimal_resource::PodLogContainer;
    use crate::resource_table::ContainerKind;
    use egui_kittest::Harness;

    fn log_window(lines: &[&str]) -> PodLogWindowState {
        PodLogWindowState {
            id: 1,
            cluster_key: 1,
            namespace: "default".to_owned(),
            pod_name: "api-0".to_owned(),
            container: PodLogContainer {
                name: "api".to_owned(),
                kind: ContainerKind::App,
            },
            lines: lines.iter().map(ToString::to_string).collect(),
            status: PodLogStatus::Following,
            close_requested: false,
            search: Default::default(),
        }
    }

    #[test]
    fn search_tracks_new_matching_log_lines_and_navigates_them() {
        let mut window = log_window(&["starting api", "request complete", "API ready"]);
        window.search.query = "api".to_owned();

        refresh_log_search(&mut window);
        assert_eq!(window.search.matching_lines, vec![0, 2]);

        window.lines.push("api shutting down".to_owned());
        refresh_log_search(&mut window);
        assert_eq!(window.search.matching_lines, vec![0, 2, 3]);

        advance_log_match(&mut window, true);
        assert_eq!(window.search.active_match, 1);
        assert_eq!(window.search.scroll_to_line, Some(2));
        assert_eq!(window.search.active_line, Some(2));
    }

    #[test]
    fn invalid_log_regex_reports_an_error_without_matches() {
        let mut window = log_window(&["request complete"]);
        window.search.query = "[".to_owned();
        window.search.regex_mode = true;

        refresh_log_search(&mut window);

        assert!(window.search.matching_lines.is_empty());
        assert!(window.search.error.is_some());
    }

    #[test]
    fn line_navigation_respects_the_filtered_or_unfiltered_display() {
        let mut window = log_window(&["api started", "worker started", "api ready"]);
        window.search.query = "api".to_owned();
        refresh_log_search(&mut window);

        advance_log_line(&mut window, true);
        assert_eq!(window.search.scroll_to_line, Some(0));
        advance_log_line(&mut window, true);
        assert_eq!(window.search.scroll_to_line, Some(1));

        window.search.filter_matches = true;
        advance_log_line(&mut window, true);
        assert_eq!(window.search.scroll_to_line, Some(0));
        advance_log_line(&mut window, true);
        assert_eq!(window.search.scroll_to_line, Some(2));
    }

    #[test]
    fn log_layout_highlights_each_matching_text_segment() {
        let matcher = regex::RegexBuilder::new("http")
            .case_insensitive(true)
            .build()
            .unwrap();

        let layout = log_line_layout_job(12, "http HTTP status", Some(&matcher));

        assert_eq!(layout.sections.len(), 5);
        assert_eq!(
            layout
                .sections
                .iter()
                .filter(|section| section.format.background != egui::Color32::TRANSPARENT)
                .count(),
            2
        );
    }

    #[test]
    fn pod_log_viewer_snapshot() {
        let mut window = log_window(&[
            "2026-08-08T15:22:17.143Z  INFO  server: listening on 0.0.0.0:8080",
            "2026-08-08T15:22:17.145Z  INFO  database: connection pool initialized",
            "2026-08-08T15:22:18.021Z  INFO  http: GET /healthz 200 2ms",
            "2026-08-08T15:22:19.403Z  INFO  http: GET /v1/widgets 200 14ms",
            "2026-08-08T15:22:21.687Z  WARN  cache: refreshing stale entry widgets:featured",
            "2026-08-08T15:22:22.004Z  INFO  cache: refresh complete",
            "2026-08-08T15:22:24.631Z  INFO  http: POST /v1/widgets 201 38ms",
            "2026-08-08T15:22:26.144Z  INFO  metrics: flushed 18 samples",
            "2026-08-08T15:22:29.711Z  INFO  http: GET /healthz 200 1ms",
            "2026-08-08T15:22:31.218Z  INFO  worker: processed batch of 42 jobs",
        ]);
        window.search.query = "http".to_owned();
        refresh_log_search(&mut window);
        let mut close_requested = false;
        let mut harness = Harness::builder()
            .with_size(super::super::APP_SNAPSHOT_SIZE)
            .build(move |ctx| show_log_window(ctx, &mut window, &mut close_requested));
        components::test_support::setup_egui(&harness.ctx);

        harness.run();
        harness.snapshot("pod_logs/viewer");
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
        refresh_log_search(&mut window);
        let mut close_requested = false;
        let mut harness = Harness::builder()
            .with_size(super::super::APP_SNAPSHOT_SIZE)
            .build(move |ctx| show_log_window(ctx, &mut window, &mut close_requested));
        components::test_support::setup_egui(&harness.ctx);

        harness.run();
        harness.snapshot("pod_logs/filter_active");
    }
}
