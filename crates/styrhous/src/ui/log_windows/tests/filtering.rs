use super::*;

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
    window.status =
        PodLogStatus::Failed("The Kubernetes API closed the log stream unexpectedly".to_owned());

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
