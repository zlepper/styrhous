//! The parts of the log window around its lines: the toolbar, the header
//! actions, the volume strip, the problem ticks and the level badges.

use super::*;
use crate::log_store::{LogSummary, Problem, VolumeBucket};

#[test]
fn typed_searches_start_after_the_debounce_on_the_frame_clock() {
    let (mut harness, window, _) = window_harness(
        log_window(&["one", "two"]),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.step();
    harness.event(egui::Event::Text("on".into()));
    harness.step();
    assert!(
        window.borrow().search.search_deadline.is_some(),
        "typing schedules the search"
    );
    assert!(!window.borrow().search.query.is_empty());

    // Each frame is 50 ms of the frame clock: the 150 ms debounce has not
    // passed after two more frames, and has after the third, however fast the
    // test machine is.
    harness.step();
    harness.step();
    assert!(
        window.borrow().search.search_deadline.is_some(),
        "100 ms into the debounce"
    );
    harness.run_steps(3);
    assert!(
        window.borrow().search.search_deadline.is_none(),
        "the search started once 150 ms of frames passed"
    );
    assert!(window.borrow().search.generation > 0);
}

#[test]
fn container_start_scrolls_to_the_first_line_and_stops_following() {
    let window = fully_loaded_log_window(512);
    let (mut harness, window, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    assert!(window.borrow().tail.is_following());
    harness.get_by_label("Container start").click();
    harness.run_steps(3);
    assert!(!window.borrow().tail.is_following());
    assert_eq!(window.borrow().visible_top_display_row, 0);
    harness.get_by_label("line 0");
}

#[test]
fn clicking_a_problem_tick_jumps_to_its_row() {
    let mut window = fully_loaded_log_window(512);
    window.summary = Some(std::sync::Arc::new(LogSummary {
        total_lines: 512,
        problems: vec![(100, Problem::Error), (400, Problem::Warning)],
        ..LogSummary::default()
    }));
    let (mut harness, window, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    let track = harness.get_by_label("2 problems").rect();
    // The tick of row 400 sits 400/512 of the way down the track.
    let click = egui::pos2(
        track.center().x,
        track.top() + track.height() * 400.0 / 512.0,
    );
    harness.event(egui::Event::PointerMoved(click));
    harness.event(egui::Event::PointerButton {
        pos: click,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.event(egui::Event::PointerButton {
        pos: click,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(4);
    assert_eq!(window.borrow().search.active_display_row, Some(400));
    assert!(!window.borrow().tail.is_following());
    harness.get_by_label("line 400");
}

#[test]
fn a_narrow_window_folds_the_view_toggles_into_a_menu() {
    let (mut harness, _, display_options) = window_harness(
        log_window(&["one line"]),
        LogDisplayOptions::default(),
        NARROW,
        Rc::new(LogStoreService::default()),
    );
    assert!(harness.query_by_label("Show log line numbers").is_none());
    assert!(harness.query_by_label("Container start").is_none());
    harness.get_by_label("View options").click();
    harness.run_steps(3);
    harness.get_by_label("Line numbers").click();
    harness.run_steps(3);
    assert!(display_options.borrow().show_line_numbers);
}

#[test]
fn a_wide_window_shows_the_toggles_and_the_search_field_fills_the_toolbar() {
    let (harness, _, _) = window_harness(
        log_window(&["one line"]),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    let search = harness.get_by_label("Search logs").rect();
    let follow = harness.get_by_label("Lock to bottom").rect();
    let only = harness.get_by_label("Filter to matching lines").rect();
    assert!(search.right() < only.left(), "the search field leads");
    assert!(only.right() < follow.left());
    assert!(
        (80.0..=640.0).contains(&search.width()),
        "the field takes what the toggles leave: {}",
        search.width()
    );
}

#[test]
fn an_unavailable_previous_instance_offers_the_current_one() {
    let mut window = log_window(&[]);
    window.instance = PodLogInstance::Previous;
    window.status = PodLogStatus::Failed("no logs retained".into());
    let (mut harness, window, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("No previous logs for api-0");
    harness.get_by_label("Show current logs").click();
    harness.run_steps(2);
    assert_eq!(
        window.borrow().requested_instance,
        Some(PodLogInstance::Current)
    );
}

#[test]
fn a_levels_badge_stands_in_for_its_word_in_the_message() {
    let (harness, _, _) = window_harness(
        log_window(&[
            "2026-08-08T15:22:17.143Z INFO server: ready",
            "2026-08-08T15:22:18.000Z plain text without a level",
        ]),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("INFO");
    harness.get_by_label("server: ready");
    assert!(harness.query_by_label("INFO server: ready").is_none());
    let with = harness.get_by_label("server: ready").rect().left();
    let without = harness
        .get_by_label("plain text without a level")
        .rect()
        .left();
    assert_eq!(with, without, "lines without a level keep the column");
}

#[test]
fn a_match_in_the_level_word_keeps_the_word_visible() {
    let mut window = log_window(&["2026-08-08T15:22:17.143Z INFO server: ready"]);
    window.search.query = "info".to_owned();
    let matcher = regex::Regex::new("(?i)info").unwrap();
    for page in window.pages.values_mut() {
        for row in &mut page.rows {
            row.match_ranges = matcher
                .find_iter(&row.text)
                .map(|found| (found.start(), found.end()))
                .collect();
        }
    }
    let (harness, _, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("INFO server: ready");
}

#[test]
fn line_numbers_of_seven_digits_and_more_fit_beside_the_time() {
    let mut window = fully_loaded_log_window(300);
    window.total_lines = 12_345_678;
    window.initial_page_loaded = true;
    let (mut harness, _, _) = window_harness(
        window,
        LogDisplayOptions {
            show_line_numbers: true,
            show_timestamps: true,
            ..LogDisplayOptions::default()
        },
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.run_steps(2);
    harness.ui_harness("pod_logs/pod_log_viewer_seven_digit_line_numbers/numbers");
    let layout = Rc::new(RefCell::new(None));
    let seen = layout.clone();
    let window = fully_loaded_log_window(1);
    let mut check = Harness::builder().build_ui(move |ui| {
        let mut window = window.clone();
        window.total_lines = 12_345_678;
        *seen.borrow_mut() = Some(RowLayout::new(
            ui,
            &window,
            LogDisplayOptions {
                show_line_numbers: true,
                ..LogDisplayOptions::default()
            },
        ));
    });
    components::test_support::setup_egui(&mut check);
    check.run();
    let layout = layout.borrow().expect("the layout was measured");
    assert!(layout.number_width >= 8.0 * layout.character_width - 0.01);
}

#[test]
fn the_window_asks_the_store_for_its_summary_and_shows_the_answer() {
    let log_store = Rc::new(LogStoreService::default());
    log_store.open(1);
    assert!(log_store.append_records(
        1,
        vec![
            crate::log_store::LogRecord::from("2026-08-10T10:00:00Z INFO started"),
            crate::log_store::LogRecord::from("2026-08-10T10:00:30Z ERROR boom"),
            crate::log_store::LogRecord::from("2026-08-10T10:01:00Z WARN slow"),
        ],
    ));
    let mut window = log_window(&[]);
    window.total_lines = 0;
    let (mut harness, window, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        log_store.clone(),
    );
    // The application feeds store results to the windows; do the same.
    let start = Instant::now();
    loop {
        while let Some(result) = log_store.try_next_result() {
            let mut windows = std::collections::BTreeMap::from([(1, window.borrow().clone())]);
            apply_store_result(&mut windows, result);
            *window.borrow_mut() = windows.remove(&1).expect("the window is still there");
        }
        if window
            .borrow()
            .summary
            .as_ref()
            .is_some_and(|summary| summary.errors == 1)
        {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "no summary arrived"
        );
        harness.step();
    }
    let window = window.borrow();
    let summary = window.summary.as_ref().expect("the summary arrived");
    assert_eq!((summary.errors, summary.warnings), (1, 1));
    assert_eq!(summary.span_seconds, Some(60));
}

#[test]
fn reconnect_attempts_are_counted_until_the_source_recovers() {
    let target = PodLogStreamTarget {
        namespace: "default".into(),
        pod_name: "api-0".into(),
        container: "api".into(),
        kind: ContainerKind::App,
    };
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.request_pod_log_window(7, vec![target.clone()], &mut commands);
    for _ in 0..3 {
        PodLogSourceReconnecting {
            log_window_id: 1,
            target: target.clone(),
            error: "reset".into(),
        }
        .apply(&mut state, &mut commands);
    }
    assert_eq!(state.log_windows[&1].source_reconnect_attempts[&target], 3);
    PodLogSourceRecovered {
        log_window_id: 1,
        target: target.clone(),
    }
    .apply(&mut state, &mut commands);
    assert!(state.log_windows[&1].source_reconnect_attempts.is_empty());
}

#[test]
fn the_summary_of_a_store_result_replaces_the_windows() {
    let mut window = log_window(&["one"]);
    let mut windows = std::collections::BTreeMap::from([(1, window.clone())]);
    apply_store_result(
        &mut windows,
        LogStoreResult::Summary {
            window_id: 1,
            summary: std::sync::Arc::new(LogSummary {
                total_lines: 1,
                bars: vec![VolumeBucket {
                    lines: 1,
                    errors: 0,
                    warnings: 0,
                }],
                ..LogSummary::default()
            }),
        },
    );
    window = windows.remove(&1).unwrap();
    assert_eq!(window.summary.expect("the summary is kept").bars.len(), 1);
}

#[test]
fn clicking_a_row_with_a_level_badge_maps_to_the_original_text_column() {
    let (mut harness, window, _) = window_harness(
        log_window(&["2026-08-08T15:22:17.143Z INFO server: ready"]),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.run_steps(2);
    let message = harness.get_by_label("server: ready").rect();
    let character_width = message.width() / "server: ready".chars().count() as f32;
    let click = egui::pos2(message.left() + 3.0 * character_width, message.center().y);
    harness.event(egui::Event::PointerMoved(click));
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: click,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run_steps(3);
    // "INFO " is hidden behind the badge: the third message column is the
    // eighth column of the text.
    assert_eq!(
        window
            .borrow()
            .selection
            .expect("a caret")
            .focus
            .byte_offset,
        "INFO ".len() + 3
    );
}

#[test]
fn problem_stepping_wraps_and_starts_from_the_top_of_the_screen() {
    let mut window = log_window(&["a", "b", "c", "d"]);
    assert!(!step_problem(&mut window, true), "no summary, no problems");
    window.summary = Some(std::sync::Arc::new(LogSummary {
        total_lines: 4,
        problems: vec![(1, Problem::Warning), (3, Problem::Error)],
        ..LogSummary::default()
    }));
    window.visible_top_display_row = 1;
    assert!(step_problem(&mut window, true));
    assert_eq!(
        window.search.active_display_row,
        Some(1),
        "from the top, inclusive"
    );
    assert!(step_problem(&mut window, true));
    assert_eq!(window.search.active_display_row, Some(3));
    assert!(step_problem(&mut window, true));
    assert_eq!(window.search.active_display_row, Some(1), "wraps");
    window.search.active_display_row = None;
    window.visible_top_display_row = 0;
    assert!(step_problem(&mut window, false));
    assert_eq!(window.search.active_display_row, Some(3));

    // A summary of another search says nothing about this view.
    window.search.query = "x".into();
    window.search.filter_matches = true;
    assert!(!step_problem(&mut window, true));
}

#[test]
fn hovering_a_row_shows_its_full_timestamp() {
    let (mut harness, _, _) = window_harness(
        log_window(&[
            "2026-08-08T15:22:17.143Z INFO server: ready",
            "no timestamp on this one",
        ]),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    // The time column is off by default; the tooltip does not depend on it.
    harness.get_by_label("Pod log line 2").hover();
    harness.run_steps(30);
    assert!(
        harness.query_by_label_contains("2026-08-08").is_none(),
        "a row without a timestamp has no tooltip"
    );
    harness.get_by_label("Pod log line 1").hover();
    harness.run_steps(30);
    harness.get_by_label("2026-08-08 15:22:17.143 +00:00");
}

#[test]
fn the_lines_run_to_the_panel_edge_and_the_scroll_bar_clears_the_problem_track() {
    let mut window = fully_loaded_log_window(512);
    window.summary = Some(std::sync::Arc::new(LogSummary {
        total_lines: 512,
        problems: vec![(100, Problem::Error), (400, Problem::Warning)],
        ..LogSummary::default()
    }));
    let (harness, _, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    let panel_bottom = WIDE.y - size::FLOATING_TOOLBAR_RESERVE;
    let bars: Vec<_> = harness
        .query_all_by_role(egui::accesskit::Role::ScrollBar)
        .map(|bar| bar.rect())
        .collect();
    let horizontal = bars
        .iter()
        .find(|bar| bar.width() > bar.height())
        .expect("a horizontal scroll bar");
    assert_eq!(
        horizontal.bottom(),
        panel_bottom,
        "no inset between the rows and the panel's bottom edge, so a partly visible last row is cut off by the panel like the board's"
    );
    let track = harness.get_by_label("2 problems").rect();
    assert!(
        track.bottom() <= horizontal.top(),
        "the problem track ends at {} above the scroll bar at {}",
        track.bottom(),
        horizontal.top()
    );
    assert_eq!(track.bottom(), panel_bottom - size::LOG_TICKS_INSET);
}

#[test]
fn the_header_controls_stand_a_header_gap_apart() {
    let (harness, _, _) = window_harness(
        log_window(&["one"]),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    let export = harness.get_by_label("Export logs").rect();
    let start = harness.get_by_label("Container start").rect();
    let current = harness.get_by_label("Current").rect();
    assert_eq!(start.left() - export.right(), 14.0);
    // The instance control is a well with 2 points of padding round its
    // options.
    assert_eq!(current.left() - start.right(), 14.0 + 2.0);
}
