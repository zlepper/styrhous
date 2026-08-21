use super::*;

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
