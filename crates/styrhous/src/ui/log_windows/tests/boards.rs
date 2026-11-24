//! The log viewer on the design boards L01–L04
//! (`docs/design-oracles/harbour/src/L0*.html`), built the way a session
//! builds it: windows opened through the UI state, worker results for the
//! stream, store results for the lines, counts and the summary.
//! `scripts/design-oracles/compare.sh L01 L02 L03 L04` compares them with
//! their oracles.

use super::*;
use crate::log_store::{Problem, VolumeBucket};
use crate::worker::{PodLogSourceReconnecting, PodLogStreamFailed, PodLogStreamStarted};

/// The day of the design's log lines.
const DAY: &str = "2026-06-01";

fn target(pod: &str, container: &str) -> PodLogStreamTarget {
    log_target("kube-system", pod, container)
}

/// A window opened the way the application opens one.
fn open_window(
    targets: Vec<PodLogStreamTarget>,
    instance: PodLogInstance,
) -> (UiState, Vec<crate::worker::WorkerCommandBox>) {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.request_pod_log_window(1, targets, &mut commands);
    let window = state
        .log_windows
        .get_mut(&1)
        .expect("the window was opened");
    window.instance = instance;
    window.store_opened = true;
    (state, commands)
}

fn take_window(mut state: UiState) -> PodLogWindowState {
    state
        .log_windows
        .remove(&1)
        .expect("the window is still open")
}

/// The raw Kubernetes record of a design line.
fn raw_line(time: &str, level: &str, message: &str) -> String {
    format!("{DAY}T{time}Z {level} {message}")
}

fn page_row(
    index: usize,
    display_row: usize,
    raw: &str,
    source: Option<&PodLogStreamTarget>,
) -> LogPageRow {
    let parsed = crate::ansi::parse_kubernetes_log_line(raw);
    LogPageRow {
        display_row,
        line_index: index,
        timestamp: parsed.timestamp,
        source: source.map(PodLogStreamTarget::display_name),
        text: parsed.line.text,
        style_spans: parsed.line.style_spans,
        match_ranges: Vec::new(),
    }
}

/// Mark every case-insensitive occurrence of `query` the way the store's
/// matcher does.
fn mark_matches(rows: &mut [LogPageRow], query: &str) {
    let matcher = regex::Regex::new(&format!("(?i){}", regex::escape(query))).expect("a matcher");
    for row in rows {
        row.match_ranges = matcher
            .find_iter(&row.text)
            .map(|found| (found.start(), found.end()))
            .collect();
    }
}

fn bucket(lines: usize, problem: Option<Problem>) -> VolumeBucket {
    VolumeBucket {
        lines,
        errors: usize::from(problem == Some(Problem::Error)),
        warnings: usize::from(problem == Some(Problem::Warning)),
    }
}

fn buckets(bars: &[(usize, Option<Problem>)]) -> Vec<VolumeBucket> {
    bars.iter()
        .map(|(lines, problem)| bucket(*lines, *problem))
        .collect()
}

fn deliver(window: &mut PodLogWindowState, result: LogStoreResult) {
    let mut windows = std::collections::BTreeMap::from([(window.id, window.clone())]);
    apply_store_result(&mut windows, result);
    *window = windows.remove(&1).expect("the window is still there");
}

/// Render `window` as a whole window of `size`, with nothing under the
/// pointer, and compare it with the snapshot `name`.
fn snapshot_board(
    window: PodLogWindowState,
    display_options: LogDisplayOptions,
    size: egui::Vec2,
    prepare: impl FnOnce(&mut Harness<'_>),
    name: &str,
) {
    // A spinner repaints continuously, so the harness only runs a few frames.
    let (mut harness, _, _) = window_harness(
        window,
        display_options,
        size,
        Rc::new(LogStoreService::default()),
    );
    prepare(&mut harness);
    harness.event(egui::Event::PointerGone);
    harness.run_steps(2);
    harness.ui_harness(name);
}

const L01_LINES: [(&str, &str, &str); 33] = [
    ("15:22:10.000", "INFO", "server: listening on 0.0.0.0:8080"),
    (
        "15:22:12.137",
        "INFO",
        "database: connection pool initialised (size=20)",
    ),
    ("15:22:14.274", "INFO", "http: GET /healthz 200 2ms"),
    ("15:22:16.411", "INFO", "http: GET /v1/widgets 200 14ms"),
    (
        "15:22:18.548",
        "WARN",
        "cache: refreshing stale entry widgets:featured",
    ),
    ("15:22:20.685", "INFO", "cache: refresh complete in 412ms"),
    ("15:22:22.822", "INFO", "http: POST /v1/widgets 201 38ms"),
    ("15:22:24.959", "DEBUG", "metrics: flushed 18 samples"),
    ("15:22:26.096", "INFO", "http: GET /healthz 200 1ms"),
    ("15:22:28.233", "INFO", "worker: processed batch of 42 jobs"),
    (
        "15:22:30.370",
        "ERROR",
        "worker: job 9187 failed: context deadline exceeded (timeout=5s)",
    ),
    (
        "15:22:32.507",
        "INFO",
        "worker: retrying job 9187 in 2s (attempt 2/5)",
    ),
    (
        "15:22:34.644",
        "INFO",
        "http: GET /v1/widgets?page=2 200 9ms",
    ),
    (
        "15:22:36.781",
        "INFO",
        "worker: job 9187 succeeded on attempt 2",
    ),
    (
        "15:22:38.918",
        "WARN",
        "db: slow query 1.2s: SELECT * FROM widgets WHERE featured = true",
    ),
    (
        "15:22:40.055",
        "INFO",
        "http: DELETE /v1/widgets/77 204 6ms",
    ),
    ("15:22:42.192", "DEBUG", "gc: heap 48MiB → 31MiB in 3ms"),
    ("15:22:44.329", "INFO", "http: GET /healthz 200 1ms"),
    (
        "15:22:46.466",
        "INFO",
        "auth: token refreshed for service-account api",
    ),
    (
        "15:22:48.603",
        "INFO",
        "http: PATCH /v1/widgets/12 200 22ms",
    ),
    ("15:22:50.740", "INFO", "worker: processed batch of 40 jobs"),
    ("15:22:52.877", "INFO", "http: GET /v1/widgets 200 11ms"),
    (
        "15:22:54.014",
        "WARN",
        "rate-limit: client 10.244.0.17 at 92% of quota",
    ),
    ("15:22:56.151", "INFO", "http: GET /healthz 200 1ms"),
    ("15:22:58.288", "INFO", "metrics: flushed 21 samples"),
    ("15:23:00.425", "INFO", "http: POST /v1/orders 201 54ms"),
    (
        "15:23:02.562",
        "INFO",
        "events: published order.created id=ord_81hd",
    ),
    ("15:23:04.699", "INFO", "worker: processed batch of 38 jobs"),
    (
        "15:23:06.836",
        "INFO",
        "http: GET /v1/orders/ord_81hd 200 7ms",
    ),
    (
        "15:23:08.973",
        "ERROR",
        "payments: upstream returned 503 Service Unavailable",
    ),
    ("15:23:10.110", "WARN", "payments: circuit open for 30s"),
    ("15:23:12.247", "INFO", "http: GET /healthz 200 1ms"),
    ("15:23:14.384", "INFO", "http: GET /v1/widgets 200 10ms"),
];

const L01_BARS: [(usize, Option<Problem>); 180] = [
    (50, None),
    (69, None),
    (100, None),
    (1, None),
    (88, None),
    (44, None),
    (6, None),
    (31, None),
    (19, None),
    (69, None),
    (94, None),
    (44, None),
    (75, None),
    (19, None),
    (44, None),
    (1, None),
    (38, None),
    (81, None),
    (50, None),
    (31, None),
    (75, None),
    (31, None),
    (13, None),
    (25, None),
    (88, None),
    (25, None),
    (25, None),
    (1, None),
    (1, None),
    (38, None),
    (38, None),
    (31, None),
    (31, None),
    (56, None),
    (56, Some(Problem::Warning)),
    (38, None),
    (38, None),
    (31, None),
    (38, None),
    (75, None),
    (56, None),
    (1, None),
    (69, None),
    (81, None),
    (31, None),
    (25, None),
    (50, None),
    (13, None),
    (63, None),
    (56, None),
    (1, None),
    (63, None),
    (13, None),
    (56, None),
    (69, None),
    (56, None),
    (94, None),
    (63, None),
    (31, None),
    (94, None),
    (94, None),
    (31, None),
    (6, None),
    (50, None),
    (1, None),
    (69, None),
    (75, None),
    (1, None),
    (81, None),
    (69, None),
    (75, None),
    (1, None),
    (88, None),
    (6, None),
    (31, None),
    (38, None),
    (19, None),
    (44, None),
    (88, None),
    (69, None),
    (100, None),
    (69, None),
    (100, None),
    (50, None),
    (83, Some(Problem::Warning)),
    (19, None),
    (69, None),
    (56, None),
    (6, None),
    (81, None),
    (13, None),
    (38, None),
    (63, None),
    (100, None),
    (69, None),
    (25, None),
    (63, None),
    (50, None),
    (13, None),
    (56, None),
    (63, None),
    (56, None),
    (31, None),
    (13, None),
    (14, Some(Problem::Error)),
    (56, None),
    (94, None),
    (31, None),
    (6, None),
    (13, None),
    (75, None),
    (6, None),
    (44, None),
    (69, None),
    (50, None),
    (88, None),
    (81, None),
    (25, None),
    (6, None),
    (6, None),
    (94, None),
    (63, None),
    (38, None),
    (25, None),
    (14, Some(Problem::Warning)),
    (81, None),
    (19, None),
    (31, None),
    (81, None),
    (69, None),
    (25, None),
    (6, None),
    (81, None),
    (56, None),
    (25, None),
    (88, None),
    (31, None),
    (100, None),
    (88, None),
    (94, None),
    (63, None),
    (94, None),
    (50, None),
    (56, None),
    (94, None),
    (75, None),
    (25, None),
    (19, None),
    (75, None),
    (31, None),
    (90, Some(Problem::Error)),
    (63, None),
    (31, None),
    (13, None),
    (94, None),
    (50, None),
    (100, None),
    (100, None),
    (63, Some(Problem::Warning)),
    (13, None),
    (69, None),
    (6, None),
    (56, None),
    (69, None),
    (50, None),
    (94, None),
    (50, None),
    (56, None),
    (63, None),
    (31, None),
    (1, None),
    (94, None),
    (50, None),
    (63, None),
    (50, None),
    (88, None),
    (56, None),
    (100, None),
    (69, None),
    (69, None),
];

const L02_LINES: [(usize, &str, &str, &str, &str); 8] = [
    (
        3410,
        "15:22:22.118",
        "worker-0",
        "INFO",
        "worker: processed batch of 42 jobs",
    ),
    (
        3411,
        "15:22:23.420",
        "api-0",
        "ERROR",
        "worker: job 9187 failed: context deadline exceeded (timeout=5s)",
    ),
    (
        3412,
        "15:22:23.421",
        "worker-0",
        "INFO",
        "worker: retrying job 9187 in 2s (attempt 2/5)",
    ),
    (
        3414,
        "15:22:25.431",
        "worker-0",
        "INFO",
        "worker: job 9187 succeeded on attempt 2",
    ),
    (
        3421,
        "15:22:31.660",
        "api-0",
        "INFO",
        "worker: processed batch of 40 jobs",
    ),
    (
        3428,
        "15:22:40.020",
        "worker-0",
        "INFO",
        "worker: processed batch of 38 jobs",
    ),
    (
        3434,
        "15:22:47.045",
        "worker-0",
        "INFO",
        "worker: processed batch of 44 jobs",
    ),
    (
        3440,
        "15:22:55.210",
        "worker-0",
        "INFO",
        "worker: processed batch of 42 jobs",
    ),
];

const L02_BARS: [(usize, Option<Problem>); 180] = [
    (88, None),
    (69, None),
    (50, None),
    (25, None),
    (31, None),
    (1, None),
    (63, None),
    (100, None),
    (88, None),
    (13, None),
    (63, None),
    (6, None),
    (75, None),
    (31, None),
    (88, None),
    (81, None),
    (31, None),
    (31, None),
    (44, None),
    (6, None),
    (19, None),
    (25, None),
    (100, None),
    (13, None),
    (75, None),
    (19, None),
    (56, None),
    (38, None),
    (44, None),
    (81, None),
    (13, None),
    (50, None),
    (38, None),
    (75, None),
    (50, None),
    (63, None),
    (6, None),
    (38, None),
    (1, None),
    (81, None),
    (6, None),
    (75, None),
    (94, None),
    (25, None),
    (1, None),
    (44, None),
    (81, None),
    (19, None),
    (1, None),
    (19, None),
    (38, None),
    (38, None),
    (63, None),
    (1, None),
    (13, None),
    (25, None),
    (1, None),
    (100, None),
    (13, None),
    (94, None),
    (38, None),
    (81, None),
    (13, None),
    (75, None),
    (38, None),
    (13, None),
    (25, None),
    (31, None),
    (6, None),
    (6, None),
    (50, None),
    (25, None),
    (50, None),
    (6, None),
    (19, None),
    (75, None),
    (44, None),
    (31, None),
    (100, None),
    (6, None),
    (69, None),
    (100, None),
    (13, None),
    (69, None),
    (19, None),
    (69, None),
    (88, None),
    (38, None),
    (75, None),
    (38, None),
    (1, None),
    (75, None),
    (63, None),
    (1, None),
    (81, None),
    (19, None),
    (38, None),
    (44, None),
    (88, None),
    (50, None),
    (63, None),
    (13, None),
    (56, None),
    (56, None),
    (19, None),
    (100, None),
    (6, None),
    (1, None),
    (75, None),
    (81, None),
    (94, None),
    (69, None),
    (44, None),
    (13, None),
    (81, None),
    (44, None),
    (19, None),
    (75, None),
    (97, Some(Problem::Error)),
    (69, None),
    (69, None),
    (25, None),
    (56, None),
    (31, None),
    (50, None),
    (1, None),
    (6, None),
    (31, None),
    (50, None),
    (31, None),
    (19, None),
    (25, None),
    (1, None),
    (6, None),
    (6, None),
    (88, None),
    (38, None),
    (75, None),
    (56, None),
    (1, None),
    (69, None),
    (56, None),
    (25, None),
    (94, None),
    (50, None),
    (25, None),
    (13, None),
    (56, None),
    (19, None),
    (13, None),
    (81, None),
    (88, None),
    (25, None),
    (75, None),
    (31, None),
    (50, None),
    (94, None),
    (69, None),
    (19, None),
    (6, None),
    (50, None),
    (100, None),
    (25, None),
    (31, None),
    (13, None),
    (31, None),
    (44, None),
    (1, None),
    (38, None),
    (31, None),
    (94, None),
    (88, None),
    (75, None),
    (13, None),
    (31, None),
    (94, None),
    (50, None),
    (13, None),
    (1, None),
    (81, None),
];

const L04_BARS: [(usize, Option<Problem>); 80] = [
    (88, None),
    (88, None),
    (88, None),
    (100, None),
    (38, None),
    (31, None),
    (100, None),
    (94, None),
    (31, None),
    (19, None),
    (88, None),
    (56, None),
    (25, None),
    (13, None),
    (6, None),
    (75, None),
    (88, None),
    (31, None),
    (1, None),
    (100, None),
    (13, None),
    (6, None),
    (6, None),
    (38, None),
    (44, None),
    (1, None),
    (88, None),
    (63, None),
    (88, None),
    (38, None),
    (100, None),
    (44, None),
    (56, None),
    (94, None),
    (1, None),
    (13, None),
    (88, None),
    (50, None),
    (81, None),
    (13, None),
    (42, Some(Problem::Warning)),
    (63, None),
    (44, None),
    (100, None),
    (56, None),
    (1, None),
    (13, None),
    (19, None),
    (75, None),
    (19, None),
    (56, None),
    (75, None),
    (13, None),
    (1, None),
    (1, None),
    (38, None),
    (38, None),
    (6, None),
    (94, None),
    (75, None),
    (75, None),
    (81, None),
    (13, None),
    (38, None),
    (50, None),
    (63, None),
    (13, None),
    (56, None),
    (63, None),
    (1, None),
    (81, None),
    (19, None),
    (25, None),
    (44, None),
    (19, None),
    (1, None),
    (6, None),
    (88, None),
    (94, None),
    (31, None),
];

/// Fill the pages around the design's lines, so the window has a whole log
/// behind them: `real` holds the rows at their line indices, the rest are
/// plain lines.
fn load_pages(window: &mut PodLogWindowState, total: usize, real: Vec<LogPageRow>) {
    let page_size = window.page_size;
    let first = real.first().map_or(0, |row| row.line_index);
    let last = real.last().map_or(0, |row| row.line_index);
    for page_start in
        (first / page_size * page_size..=last / page_size * page_size).step_by(page_size)
    {
        let rows = (page_start..(page_start + page_size).min(total))
            .map(|index| {
                real.iter()
                    .find(|row| row.line_index == index)
                    .cloned()
                    .unwrap_or_else(|| {
                        page_row(
                            index,
                            index,
                            &raw_line("15:00:00.000", "INFO", "earlier line"),
                            None,
                        )
                    })
            })
            .collect();
        window.insert_page(
            LogPageKey {
                generation: 0,
                filter_matches: false,
                page_start,
            },
            rows,
        );
    }
}

fn updated(total_lines: usize, backfill_lines: Option<usize>) -> LogStoreResult {
    LogStoreResult::Updated {
        window_id: 1,
        total_lines,
        completed_search: None,
        appended_rows: Vec::new(),
        backfill_lines,
    }
}

/// Focus the search field the way a user does, with Command+F.
fn focus_search(harness: &mut Harness<'_>) {
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.run_steps(3);
}

#[test]
fn l01_logs_following_with_search() {
    let api = target("api-0", "api");
    let (mut state, mut commands) = open_window(vec![api.clone()], PodLogInstance::Current);
    PodLogStreamStarted { log_window_id: 1 }.apply(&mut state, &mut commands);
    let mut window = take_window(state);
    let first = 1284 - L01_LINES.len();
    let mut rows: Vec<_> = L01_LINES
        .iter()
        .enumerate()
        .map(|(offset, (time, level, message))| {
            page_row(
                first + offset,
                first + offset,
                &raw_line(time, level, message),
                None,
            )
        })
        .collect();
    mark_matches(&mut rows, "http");
    let current = first
        + L01_LINES
            .iter()
            .position(|(time, ..)| *time == "15:22:44.329")
            .expect("the board's current line");
    load_pages(&mut window, 1284, rows);
    deliver(&mut window, updated(1284, None));
    deliver(
        &mut window,
        LogStoreResult::Summary {
            window_id: 1,
            summary: std::sync::Arc::new(LogSummary {
                total_lines: 1284,
                errors: 2,
                warnings: 4,
                span_seconds: Some(900),
                bars: buckets(&L01_BARS),
                sources: vec![(api.display_name(), 1284)],
                problems: [
                    (0.11, Problem::Warning),
                    (0.30, Problem::Error),
                    (0.42, Problem::Warning),
                    (0.62, Problem::Warning),
                    (0.86, Problem::Error),
                    (0.88, Problem::Warning),
                ]
                .map(|(position, problem)| ((position * 1284.0) as usize, problem))
                .to_vec(),
                filtered_by: None,
            }),
        },
    );
    window.search.query = "http".to_owned();
    window.search.match_count = 23;
    window.search.active_match = Some(6);
    window.search.active_display_row = Some(current);
    // The board shows the log from a line on, not stuck to its end.
    window.tail.release();
    window.search.scroll_to_display_row = Some(first);
    snapshot_board(
        window,
        LogDisplayOptions {
            show_timestamps: true,
            ..LogDisplayOptions::default()
        },
        WIDE,
        focus_search,
        "boards/l01_logs_following_with_search",
    );
}

#[test]
fn l02_logs_two_sources_filtered_reconnecting() {
    let api = target("api-0", "api");
    let worker = target("worker-0", "worker");
    let (mut state, mut commands) =
        open_window(vec![api.clone(), worker.clone()], PodLogInstance::Current);
    PodLogStreamStarted { log_window_id: 1 }.apply(&mut state, &mut commands);
    for _ in 0..3 {
        PodLogSourceReconnecting {
            log_window_id: 1,
            target: worker.clone(),
            error: "the connection was reset".to_owned(),
        }
        .apply(&mut state, &mut commands);
    }
    let mut window = take_window(state);
    let mut rows: Vec<_> = L02_LINES
        .iter()
        .enumerate()
        .map(|(row, (number, time, source, level, message))| {
            let target = if *source == "api-0" { &api } else { &worker };
            let mut page = page_row(
                number - 1,
                row,
                &raw_line(time, level, message),
                Some(target),
            );
            page.display_row = row;
            page
        })
        .collect();
    mark_matches(&mut rows, "worker");
    window.insert_page(
        LogPageKey {
            generation: 0,
            filter_matches: true,
            page_start: 0,
        },
        rows,
    );
    window.search.query = "worker".to_owned();
    window.search.filter_matches = true;
    window.search.match_count = L02_LINES.len();
    window.search.active_match = Some(2);
    window.search.active_display_row = Some(2);
    deliver(&mut window, updated(8912, Some(2100)));
    deliver(
        &mut window,
        LogStoreResult::Summary {
            window_id: 1,
            summary: std::sync::Arc::new(LogSummary {
                total_lines: 8912,
                errors: 1,
                warnings: 0,
                span_seconds: Some(900),
                bars: buckets(&L02_BARS),
                sources: vec![(api.display_name(), 4410), (worker.display_name(), 4502)],
                problems: vec![(1, Problem::Error)],
                filtered_by: Some((0, L02_LINES.len())),
            }),
        },
    );
    window.tail.release();
    snapshot_board(
        window,
        LogDisplayOptions {
            show_line_numbers: true,
            show_timestamps: true,
            ..LogDisplayOptions::default()
        },
        WIDE,
        focus_search,
        "boards/l02_logs_two_sources_filtered_reconnecting",
    );
}

#[test]
fn l03_logs_previous_instance_unavailable() {
    let api = target("api-0", "api");
    let (mut state, mut commands) = open_window(vec![api], PodLogInstance::Previous);
    PodLogStreamFailed {
        log_window_id: 1,
        error: "The container hasn’t restarted, so Kubernetes has no logs retained from a \
                previous start."
            .to_owned(),
    }
    .apply(&mut state, &mut commands);
    snapshot_board(
        take_window(state),
        LogDisplayOptions {
            show_timestamps: true,
            ..LogDisplayOptions::default()
        },
        WIDE,
        |_| {},
        "boards/l03_logs_previous_instance_unavailable",
    );
}

#[test]
fn l04_logs_spooling_in_a_narrow_window() {
    let pod = target(
        "kube-controller-manager-kind-control-plane",
        "kube-controller-manager",
    );
    let (mut state, mut commands) = open_window(vec![pod.clone()], PodLogInstance::Current);
    PodLogStreamStarted { log_window_id: 1 }.apply(&mut state, &mut commands);
    let mut window = take_window(state);
    deliver(&mut window, updated(18_400, None));
    deliver(
        &mut window,
        LogStoreResult::Summary {
            window_id: 1,
            summary: std::sync::Arc::new(LogSummary {
                total_lines: 18_400,
                errors: 0,
                warnings: 1,
                span_seconds: Some(400),
                bars: buckets(&L04_BARS),
                sources: vec![(pod.display_name(), 18_400)],
                problems: Vec::new(),
                filtered_by: None,
            }),
        },
    );
    assert!(initial_spool_is_pending(&window));
    snapshot_board(
        window,
        LogDisplayOptions::default(),
        NARROW,
        |_| {},
        "boards/l04_logs_spooling_in_a_narrow_window",
    );
}
