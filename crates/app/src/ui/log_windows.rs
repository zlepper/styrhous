use super::state::{LogPageKey, PodLogStatus, PodLogWindowState, UiState};
use crate::log_store::{LOG_PAGE_SIZE, LogStoreService};
use crate::worker::WorkerCommand;
use components::colors::{SUCCESS, TABLE_BORDER, TOOLBAR_BACKGROUND, gray};
use components::{TailwindSearchInput, icons};
use std::time::{Duration, Instant};

const TOOLBAR_TEXT_SIZE: f32 = 16.0;

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
        let Some(window) = ui_state.log_windows.get_mut(&id) else {
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
                show_log_window(window_ctx, window, log_store, &mut close_requested);
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

fn show_log_window(
    ctx: &egui::Context,
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    _close_requested: &mut bool,
) {
    sync_search(ctx, window, log_store);
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
                    egui::RichText::new(status_label(window))
                        .size(TOOLBAR_TEXT_SIZE)
                        .color(gray::_600),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    show_log_search_controls(ctx, ui, window, log_store)
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
                    if let Some(row) = window.pages.get(&key).and_then(|page| {
                        page.rows.iter().find(|row| row.display_row == display_row)
                    }) {
                        ui.add(
                            egui::Label::new(log_line_layout_job(
                                row.line_index,
                                &row.text,
                                &row.match_ranges,
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
                            let filter = filter_button(ui, filter_is_active(window));
                            (previous_line, previous_match, next_match, next_line, filter)
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
    line: &str,
    ranges: &[(usize, usize)],
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
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
    let highlighted = egui::TextFormat {
        font_id: egui::FontId::monospace(14.0),
        color: egui::Color32::from_rgb(254, 243, 199),
        background: egui::Color32::from_rgb(120, 53, 15),
        ..Default::default()
    };
    job.append(&format!("{line_index:>6}  "), 0.0, number);
    let mut cursor = 0;
    for &(start, end) in ranges {
        if start > cursor {
            job.append(&line[cursor..start], 0.0, text.clone());
        }
        job.append(&line[start..end], 0.0, highlighted.clone());
        cursor = end;
    }
    if cursor < line.len() {
        job.append(&line[cursor..], 0.0, text);
    }
    job
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
        PodLogStatus::Failed(_) => egui::Color32::from_rgb(185, 28, 28),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_store::{LogPageRow, LogStoreConfig, LogStoreResult};
    use crate::minimal_resource::PodLogContainer;
    use crate::resource_table::ContainerKind;
    use egui_kittest::Harness;

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
                .map(|(line_index, text)| LogPageRow {
                    display_row: line_index,
                    line_index,
                    text: (*text).to_owned(),
                    match_ranges: Vec::new(),
                })
                .collect(),
        );
        window
    }

    #[test]
    fn layout_highlights_only_matching_segments() {
        let job = log_line_layout_job(4, "http http", &[(0, 4), (5, 9)]);
        assert_eq!(job.sections.len(), 4);
        assert_eq!(job.text, "     4  http http");
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
            "2026-08-08T15:22:21.687Z  WARN  cache: refreshing stale entry widgets:featured",
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
                    LogPageRow {
                        display_row,
                        line_index,
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
        let log_store = LogStoreService::default();
        let mut close_requested = false;
        let mut harness = Harness::builder()
            .with_size(super::super::APP_SNAPSHOT_SIZE)
            .build(move |ctx| show_log_window(ctx, &mut window, &log_store, &mut close_requested));
        components::test_support::setup_egui(&harness.ctx);
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
