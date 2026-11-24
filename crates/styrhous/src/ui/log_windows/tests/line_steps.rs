//! The previous / next line buttons beside the search field and the time
//! zone the log shows its times in.

use super::*;
use components::design::size;
use egui_kittest::kittest::NodeT;

const EAST: fn() -> time::UtcOffset = || time::UtcOffset::from_hms(2, 0, 0).unwrap();

fn active_row(window: &Shared<PodLogWindowState>) -> Option<usize> {
    window.borrow().search.active_display_row
}

#[test]
fn the_line_buttons_step_through_the_lines_and_wrap_at_the_ends() {
    let (mut harness, window, _) = window_harness(
        fully_loaded_log_window(512),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    assert!(window.borrow().tail.is_following());

    harness.get_by_label("Next line").click();
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(0), "from nothing, the first line");
    assert!(
        !window.borrow().tail.is_following(),
        "stepping lines leaves the end of the log"
    );
    harness.get_by_label("line 0");

    harness.get_by_label("Next line").click();
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(1));

    harness.get_by_label("Previous line").click();
    harness.run_steps(2);
    harness.get_by_label("Previous line").click();
    harness.run_steps(2);
    assert_eq!(
        active_row(&window),
        Some(511),
        "before the first is the last"
    );
    harness.get_by_label("line 511");

    harness.get_by_label("Next line").click();
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(0), "after the last is the first");
}

#[test]
fn previous_line_from_nothing_starts_at_the_last_line() {
    let (mut harness, window, _) = window_harness(
        fully_loaded_log_window(512),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("Previous line").click();
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(511));
}

#[test]
fn alt_up_and_down_step_lines_like_the_buttons() {
    let (mut harness, window, _) = window_harness(
        fully_loaded_log_window(512),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    window.borrow_mut().search.active_display_row = Some(399);
    harness.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowDown);
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(400));
    harness.get_by_label("line 400");

    harness.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowUp);
    harness.run_steps(2);
    harness.key_press_modifiers(egui::Modifiers::ALT, egui::Key::ArrowUp);
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(398));
}

#[test]
fn only_the_matching_lines_are_stepped_through_when_the_log_is_filtered() {
    let mut window = fully_loaded_log_window(512);
    window.search.query = "line".to_owned();
    window.search.filter_matches = true;
    window.search.search_complete = true;
    window.search.match_count = 3;
    let (mut harness, window, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    for expected in [0, 1, 2, 0] {
        harness.get_by_label("Next line").click();
        harness.run_steps(2);
        assert_eq!(active_row(&window), Some(expected));
    }
}

#[test]
fn the_line_buttons_name_their_shortcuts() {
    let (mut harness, _, _) = window_harness(
        fully_loaded_log_window(512),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("Next line").hover();
    harness.run_steps(30);
    harness.get_by_label("Next line (Alt+Down)");
    harness.ui_harness("pod_logs/line_step_buttons/next_line_tooltip");

    harness.get_by_label("Previous line").hover();
    harness.run_steps(30);
    harness.get_by_label("Previous line (Alt+Up)");
}

#[test]
fn the_line_buttons_are_off_with_a_reason_when_there_is_nothing_to_step() {
    for (window, reason) in [
        (log_window(&[]), "There are no lines to step through"),
        (
            {
                let mut failed = log_window(&[]);
                failed.status = PodLogStatus::Failed("no logs retained".into());
                failed
            },
            "The log is unavailable",
        ),
    ] {
        let (mut harness, window, _) = window_harness(
            window,
            LogDisplayOptions::default(),
            WIDE,
            Rc::new(LogStoreService::default()),
        );
        for name in ["Previous line", "Next line"] {
            assert!(harness.get_by_label(name).accesskit_node().is_disabled());
        }
        harness.get_by_label("Next line").hover();
        harness.run_steps(30);
        harness.get_by_label(reason);
        harness.get_by_label("Next line").click();
        harness.run_steps(2);
        assert_eq!(active_row(&window), None, "a disabled button does nothing");
    }
}

#[test]
fn the_narrow_toolbar_keeps_the_line_buttons_beside_the_view_options() {
    let (mut harness, window, _) = window_harness(
        fully_loaded_log_window(512),
        LogDisplayOptions::default(),
        NARROW,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("View options");
    let previous = harness.get_by_label("Previous line").rect();
    let next = harness.get_by_label("Next line").rect();
    assert!(previous.right() <= next.left());
    assert_eq!(previous.center().y, next.center().y);
    harness.get_by_label("Next line").click();
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(0));
}

fn timed_window() -> PodLogWindowState {
    log_window(&[
        "2026-08-08T15:22:17.143Z INFO server: ready",
        "2026-08-08T23:30:00.000Z INFO server: late",
    ])
}

#[test]
fn row_times_and_their_hover_read_in_the_local_zone_with_the_offset() {
    let clock = WallClock::Fixed(time::OffsetDateTime::UNIX_EPOCH.to_offset(EAST()));
    let (mut harness, _, _) = window_harness_at(
        timed_window(),
        LogDisplayOptions {
            show_timestamps: true,
            ..LogDisplayOptions::default()
        },
        WIDE,
        Rc::new(LogStoreService::default()),
        clock,
    );
    harness.get_by_label("17:22:17.143");
    harness.get_by_label("01:30:00.000");
    assert!(harness.query_by_label("15:22:17.143").is_none());

    harness.get_by_label("Pod log line 1").hover();
    harness.run_steps(30);
    harness.get_by_label("2026-08-08 17:22:17.143 +02:00");
    harness.get_by_label("Pod log line 2").hover();
    harness.run_steps(30);
    harness.get_by_label("2026-08-09 01:30:00.000 +02:00");
}

#[test]
fn the_strip_says_the_times_are_local() {
    let (harness, _, _) = window_harness(
        timed_window(),
        LogDisplayOptions {
            show_timestamps: true,
            ..LogDisplayOptions::default()
        },
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("local time");
}

#[test]
fn escape_leaves_the_search_field_and_keeps_the_search() {
    let (mut harness, window, _) = window_harness(
        log_window(&["one", "two", "one more"]),
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.step();
    harness.event(egui::Event::Text("one".into()));
    harness.run_steps(6);
    {
        let mut window = window.borrow_mut();
        window.search.search_complete = true;
        window.search.match_count = 2;
        window.search.active_match = Some(1);
        window.search.filter_matches = true;
    }
    harness.run_steps(2);
    assert!(harness.ctx.memory(|memory| memory.focused()).is_some());

    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(
        harness.ctx.memory(|memory| memory.focused()).is_none(),
        "Esc takes the focus away"
    );
    let window = window.borrow();
    assert_eq!(window.search.query, "one");
    assert_eq!(window.search.match_count, 2);
    assert_eq!(window.search.active_match, Some(1));
    assert!(window.search.filter_matches);
}

#[test]
fn a_five_digit_count_neither_wraps_nor_overlaps_the_query() {
    for width in [WIDE, NARROW] {
        let mut window = fully_loaded_log_window(512);
        window.search.query = "http".to_owned();
        window.search.search_complete = true;
        window.search.match_count = 5678;
        window.search.active_match = Some(1233);
        let (harness, _, _) = window_harness(
            window,
            LogDisplayOptions::default(),
            width,
            Rc::new(LogStoreService::default()),
        );
        let query = harness.get_by_label("Search logs").rect();
        let count = harness.get_by_label("1,234 / 5,678").rect();
        let clear = harness.get_by_label("Clear search").rect();
        assert!(
            count.height() < 20.0,
            "the count stays on one line: {count:?}"
        );
        assert!(
            query.right() <= count.left(),
            "the query ends before the count: {query:?} {count:?}"
        );
        assert!(
            count.right() <= clear.left(),
            "the count ends before the clear button: {count:?} {clear:?}"
        );
        assert!(
            query.width() >= size::LOG_SEARCH_QUERY_MIN,
            "the query keeps its minimum width: {query:?}"
        );
    }
}

#[test]
fn stepping_lines_does_not_wrap_while_a_filtering_search_is_still_running() {
    let mut window = fully_loaded_log_window(512);
    window.search.query = "line".to_owned();
    window.search.filter_matches = true;
    window.search.search_complete = false;
    window.search.match_count = 3;
    let (mut harness, window, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    // More matches may still arrive, so the last known one is not the end.
    for expected in [0, 1, 2, 2, 2] {
        harness.get_by_label("Next line").click();
        harness.run_steps(2);
        assert_eq!(active_row(&window), Some(expected));
    }
    for expected in [1, 0, 0] {
        harness.get_by_label("Previous line").click();
        harness.run_steps(2);
        assert_eq!(active_row(&window), Some(expected));
    }

    // Once the search has finished the ends are known and stepping wraps.
    window.borrow_mut().search.search_complete = true;
    harness.run_steps(2);
    harness.get_by_label("Previous line").click();
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(2));
}

#[test]
fn stepping_lines_still_wraps_during_an_unfiltered_search() {
    let mut window = fully_loaded_log_window(512);
    window.search.query = "line".to_owned();
    window.search.filter_matches = false;
    window.search.search_complete = false;
    let (mut harness, window, _) = window_harness(
        window,
        LogDisplayOptions::default(),
        WIDE,
        Rc::new(LogStoreService::default()),
    );
    harness.get_by_label("Previous line").click();
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(511));
    harness.get_by_label("Next line").click();
    harness.run_steps(2);
    assert_eq!(active_row(&window), Some(0), "all lines are shown: wrap");
}
