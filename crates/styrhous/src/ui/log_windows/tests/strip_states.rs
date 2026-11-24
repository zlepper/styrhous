//! What the volume strip says in each state of the log, how dragging its
//! window scrolls the log, when it asks the store for a fresh summary, and
//! what the window shows of a current instance that failed.

use super::*;
use crate::log_store::{LogSummary, VolumeBucket};
use crate::ui::log_windows::strip::{
    drag_window, request_summary_if_stale, strip_caption, strip_meta,
};
use std::sync::Arc;

fn summary(errors: usize, warnings: usize, span_seconds: Option<u64>) -> Option<Arc<LogSummary>> {
    Some(Arc::new(LogSummary {
        errors,
        warnings,
        span_seconds,
        ..LogSummary::default()
    }))
}

fn times_shown() -> LogDisplayOptions {
    LogDisplayOptions {
        show_timestamps: true,
        ..LogDisplayOptions::default()
    }
}

#[test]
fn the_strip_caption_follows_the_state_of_the_log() {
    let mut window = log_window(&["one"]);
    assert_eq!(strip_caption(&window), "Volume", "nothing counted yet");

    window.summary = summary(0, 0, Some(40));
    assert_eq!(strip_caption(&window), "Volume · 40 s");
    window.summary = summary(1, 1, Some(900));
    assert_eq!(
        strip_caption(&window),
        "Volume · 15 min · 1 error · 1 warning"
    );
    window.summary = summary(2, 4, Some(900));
    assert_eq!(
        strip_caption(&window),
        "Volume · 15 min · 2 errors · 4 warnings"
    );
    window.summary = summary(0, 3, None);
    assert_eq!(strip_caption(&window), "Volume · 3 warnings");

    // A filtered view counts its matches instead of the problems.
    window.search.query = "worker".to_owned();
    window.search.filter_matches = true;
    window.search.search_complete = true;
    window.search.match_count = 23;
    window.summary = summary(2, 4, Some(900));
    assert_eq!(strip_caption(&window), "Volume · 15 min · 23 matches");

    let mut spooling = log_window(&[]);
    spooling.total_lines = 5_000;
    spooling.initial_page_loaded = false;
    assert_eq!(strip_caption(&spooling), "Volume · spooling");

    let mut previous = log_window(&[]);
    previous.instance = PodLogInstance::Previous;
    previous.status = PodLogStatus::Failed("no logs retained".into());
    assert_eq!(strip_caption(&previous), "Volume · previous instance");
}

#[test]
fn the_strip_meta_names_the_match_the_backfill_and_the_time_zone() {
    let mut window = log_window(&["one"]);
    assert_eq!(strip_meta(&window, &LogDisplayOptions::default()), None);
    assert_eq!(
        strip_meta(&window, &times_shown()).as_deref(),
        Some("local time"),
        "times are local"
    );

    window.search.query = "one".to_owned();
    window.search.search_complete = true;
    window.search.match_count = 23;
    assert_eq!(
        strip_meta(&window, &times_shown()).as_deref(),
        Some("23 matches · local time")
    );
    window.search.active_match = Some(6);
    assert_eq!(
        strip_meta(&window, &times_shown()).as_deref(),
        Some("7 of 23 matches · local time")
    );
    window.search.error = Some("bad pattern".to_owned());
    assert_eq!(strip_meta(&window, &LogDisplayOptions::default()), None);
    window.search.error = None;

    // A filtered view says how many matches it holds in the caption.
    window.search.filter_matches = true;
    assert_eq!(strip_meta(&window, &LogDisplayOptions::default()), None);
    window.search.filter_matches = false;

    window.backfill_lines = Some(1_250_000);
    assert_eq!(
        strip_meta(&window, &times_shown()).as_deref(),
        Some("7 of 23 matches · backfill 1.2M · local time")
    );

    assert_eq!(
        strip_meta(&log_window(&[]), &times_shown()),
        None,
        "an empty log has nothing to say"
    );
}

fn four_bars() -> LogSummary {
    LogSummary {
        total_lines: 400,
        bars: (0..4)
            .map(|_| VolumeBucket {
                lines: 100,
                ..VolumeBucket::default()
            })
            .collect(),
        ..LogSummary::default()
    }
}

#[test]
fn dragging_the_strip_window_scrolls_the_log_by_the_bars_moved() {
    let mut window = fully_loaded_log_window(400);
    window.visible_rows = 20;
    window.visible_top_display_row = 100;
    window.tail.release();
    window.search.active_display_row = Some(105);

    drag_window(&mut window, &four_bars(), 1.0);
    assert_eq!(window.search.scroll_to_display_row, Some(200));
    assert_eq!(window.search.active_display_row, None);

    drag_window(&mut window, &four_bars(), -9.0);
    assert_eq!(
        window.search.scroll_to_display_row,
        Some(0),
        "dragging past the start stops at the first line"
    );
    drag_window(&mut window, &four_bars(), 99.0);
    assert_eq!(
        window.search.scroll_to_display_row,
        Some(399),
        "and past the end at the last"
    );
}

#[test]
fn dragging_the_strip_window_stops_following_the_tail() {
    let mut window = fully_loaded_log_window(400);
    window.visible_rows = 20;
    window.status = PodLogStatus::Following;
    assert!(window.tail.is_following());
    drag_window(&mut window, &four_bars(), 1.0);
    assert!(!window.tail.is_following());
}

#[test]
fn dragging_the_strip_window_needs_bars_and_a_visible_part_of_the_log() {
    let mut window = fully_loaded_log_window(400);
    drag_window(&mut window, &LogSummary::default(), 1.0);
    assert_eq!(window.search.scroll_to_display_row, None, "no bars");

    // A filtered view whose matches are not loaded has no part on screen.
    window.search.query = "line".to_owned();
    window.search.filter_matches = true;
    window.search.match_count = 10;
    drag_window(&mut window, &four_bars(), 1.0);
    assert_eq!(window.search.scroll_to_display_row, None, "nothing loaded");
}

/// The window's summary request after asking at frame time `time`.
fn ask_at(
    window: &mut PodLogWindowState,
    store: &LogStoreService,
    time: f64,
) -> Option<SummaryRequest> {
    let ctx = egui::Context::default();
    ctx.begin_pass(egui::RawInput {
        time: Some(time),
        ..Default::default()
    });
    request_summary_if_stale(&ctx, window, store, 10);
    ctx.end_pass().textures_delta.clear();
    window.summary_request
}

#[test]
fn summaries_are_asked_for_when_the_log_changes_but_not_too_often() {
    let store = LogStoreService::default();
    store.open(1);
    let mut window = log_window(&["one", "two"]);
    window.total_lines = 5;

    let first = ask_at(&mut window, &store, 10.0).expect("the first question is asked");
    assert_eq!((first.at, first.total_lines), (10.0, 5));

    window.total_lines = 6;
    assert_eq!(
        ask_at(&mut window, &store, 10.2).map(|request| request.at),
        Some(10.0),
        "an unanswered question is waited for"
    );
    window.summary_request.as_mut().unwrap().answered = true;
    window.total_lines = 7;
    assert_eq!(
        ask_at(&mut window, &store, 10.3).map(|request| request.at),
        Some(10.0),
        "even an answered one is not repeated within half a second"
    );
    assert_eq!(
        ask_at(&mut window, &store, 10.5).map(|request| (request.at, request.total_lines)),
        Some((10.5, 7)),
        "after half a second the changed log is asked about"
    );

    // The same question is not asked twice.
    window.summary_request.as_mut().unwrap().answered = true;
    assert_eq!(
        ask_at(&mut window, &store, 20.0).map(|request| request.at),
        Some(10.5)
    );

    // An unanswered question is asked again once it has timed out.
    window.total_lines = 8;
    assert_eq!(
        ask_at(&mut window, &store, 21.0).map(|request| request.at),
        Some(21.0)
    );
    window.total_lines = 9;
    assert_eq!(
        ask_at(&mut window, &store, 22.9).map(|request| request.at),
        Some(21.0),
        "still waiting for the answer"
    );
    assert_eq!(
        ask_at(&mut window, &store, 23.1).map(|request| (request.at, request.total_lines)),
        Some((23.1, 9)),
        "two seconds without an answer is long enough"
    );

    let mut closed = log_window(&["one"]);
    closed.store_opened = false;
    assert_eq!(
        ask_at(&mut closed, &store, 30.0),
        None,
        "nothing is asked of a store that is not open"
    );
}

#[test]
fn a_current_instance_that_failed_shows_why_and_has_no_other_to_offer() {
    let mut window = log_window(&[]);
    window.status = PodLogStatus::Failed("connection refused".into());
    let (mut harness, window, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("Couldn’t read the logs of api-0");
    harness.get_by_label("connection refused");
    harness.get_by_label("Stream failed");
    assert!(harness.query_by_label("Show current logs").is_none());
    assert_eq!(window.borrow().requested_instance, None);
    harness.run_steps(2);
    assert_eq!(window.borrow().requested_instance, None);
}

#[test]
fn several_sources_that_failed_are_counted_together() {
    let mut window = log_window(&[]);
    window.targets = vec![
        log_target("default", "api-0", "api"),
        log_target("default", "api-1", "api"),
    ];
    window.status = PodLogStatus::Failed("forbidden".into());
    let (harness, _, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("Couldn’t read the logs of 2 sources");
    harness.get_by_label("forbidden");
}
