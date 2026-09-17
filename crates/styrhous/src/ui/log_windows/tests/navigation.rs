use super::*;

#[test]
fn completed_fully_loaded_logs_do_not_oscillate_at_the_bottom() {
    let window = Rc::new(RefCell::new(fully_loaded_log_window(10_000)));
    let window_for_ui = window.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
    let display_options_for_ui = display_options.clone();
    let scroll_state = Rc::new(RefCell::new(None));
    let scroll_state_for_ui = scroll_state.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        *scroll_state_for_ui.borrow_mut() = Some(show_log_window_with_scroll_state(
            ctx,
            &mut window_for_ui.borrow_mut(),
            &mut display_options_for_ui.borrow_mut(),
            &log_store,
            &mut close_requested,
        ));
    });
    components::test_support::setup_egui(&mut harness);
    harness.run();

    let bottom_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    assert!(bottom_offset > 0.0);

    for _ in 0..5 {
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(egui::pos2(400.0, 100.0)));
        harness.input_mut().events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        });
        harness.step();
        let offset = scroll_state
            .borrow()
            .as_ref()
            .expect("log scroll area was rendered")
            .state
            .offset
            .y;
        assert_eq!(offset, bottom_offset);
    }
}

#[test]
fn live_tail_rows_render_after_scrolling_back_to_bottom_before_page_load() {
    let mut window = fully_loaded_log_window(LOG_PAGE_SIZE * 2);
    window.status = PodLogStatus::Following;
    let windows = Rc::new(RefCell::new(std::collections::BTreeMap::from([(
        1, window,
    )])));
    let windows_for_ui = windows.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
    let display_options_for_ui = display_options.clone();
    let scroll_state = Rc::new(RefCell::new(None));
    let scroll_state_for_ui = scroll_state.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        let mut windows = windows_for_ui.borrow_mut();
        let window = windows.get_mut(&1).expect("log window exists");
        *scroll_state_for_ui.borrow_mut() = Some(show_log_window_with_scroll_state(
            ctx,
            window,
            &mut display_options_for_ui.borrow_mut(),
            &log_store,
            &mut close_requested,
        ));
    });
    components::test_support::setup_egui(&mut harness);
    harness.run_steps(2);

    let initial_scroll = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    let scroll_position = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .inner_rect
        .center();
    harness.event(egui::Event::PointerMoved(scroll_position));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 10_000.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(2);
    let scrolled_away_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    assert!(scrolled_away_offset < initial_scroll);
    assert!(!windows.borrow()[&1].tail.is_following());

    crate::ui::log_state::apply_store_result(
        &mut windows.borrow_mut(),
        LogStoreResult::Updated {
            window_id: 1,
            total_lines: LOG_PAGE_SIZE * 2 + 1,
            completed_search: None,
            appended_rows: vec![LogPageRow {
                display_row: LOG_PAGE_SIZE * 2,
                line_index: LOG_PAGE_SIZE * 2,
                timestamp: None,
                source: None,
                text: "live now".to_owned(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            }],
            backfill_lines: None,
        },
    );
    assert_eq!(
        windows.borrow()[&1].live_rows[&(LOG_PAGE_SIZE * 2)].text,
        "live now"
    );
    harness.run_steps(2);
    let held_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    assert!(
        (held_offset - scrolled_away_offset).abs() <= TAIL_BOTTOM_TOLERANCE_POINTS,
        "a released tail must not move when new rows arrive"
    );

    harness.event(egui::Event::PointerMoved(scroll_position));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -10_000.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(2);

    let reattached_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    assert!(windows.borrow()[&1].tail.is_following());
    assert!(
        harness.get_by_label("live now").rect().intersects(
            scroll_state
                .borrow()
                .as_ref()
                .expect("log scroll area was rendered")
                .inner_rect
        )
    );

    crate::ui::log_state::apply_store_result(
        &mut windows.borrow_mut(),
        LogStoreResult::Updated {
            window_id: 1,
            total_lines: LOG_PAGE_SIZE * 2 + 2,
            completed_search: None,
            appended_rows: vec![LogPageRow {
                display_row: LOG_PAGE_SIZE * 2 + 1,
                line_index: LOG_PAGE_SIZE * 2 + 1,
                timestamp: None,
                source: None,
                text: "still following".to_owned(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            }],
            backfill_lines: None,
        },
    );
    harness.run_steps(2);

    assert!(
        scroll_state
            .borrow()
            .as_ref()
            .expect("log scroll area was rendered")
            .state
            .offset
            .y
            > reattached_offset,
        "an attached tail must advance as new rows arrive"
    );
    assert!(
        harness.get_by_label("still following").rect().intersects(
            scroll_state
                .borrow()
                .as_ref()
                .expect("log scroll area was rendered")
                .inner_rect
        )
    );
}

#[test]
fn tail_lock_toggle_reattaches_a_scrolled_away_live_viewer() {
    let mut window = fully_loaded_log_window(LOG_PAGE_SIZE * 2);
    window.status = PodLogStatus::Following;
    let windows = Rc::new(RefCell::new(std::collections::BTreeMap::from([(
        1, window,
    )])));
    let windows_for_ui = windows.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
    let display_options_for_ui = display_options.clone();
    let scroll_state = Rc::new(RefCell::new(None));
    let scroll_state_for_ui = scroll_state.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        let mut windows = windows_for_ui.borrow_mut();
        let window = windows.get_mut(&1).expect("log window exists");
        *scroll_state_for_ui.borrow_mut() = Some(show_log_window_with_scroll_state(
            ctx,
            window,
            &mut display_options_for_ui.borrow_mut(),
            &log_store,
            &mut close_requested,
        ));
    });
    components::test_support::setup_egui(&mut harness);
    harness.run_steps(2);

    let initial_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    let scroll_position = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .inner_rect
        .center();
    harness.event(egui::Event::PointerMoved(scroll_position));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 10_000.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(2);
    assert!(!windows.borrow()[&1].tail.is_following());

    windows
        .borrow_mut()
        .get_mut(&1)
        .expect("log window exists")
        .search
        .scroll_to_display_row = Some(0);
    harness.get_by_label("Lock to bottom").click();
    harness.run_steps(2);

    let locked_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    assert!(windows.borrow()[&1].tail.is_following());
    assert!(locked_offset >= initial_offset);
    assert_eq!(
        windows.borrow()[&1].search.scroll_to_display_row,
        None,
        "explicitly locking to the tail must supersede pending search navigation"
    );

    crate::ui::log_state::apply_store_result(
        &mut windows.borrow_mut(),
        LogStoreResult::Updated {
            window_id: 1,
            total_lines: LOG_PAGE_SIZE * 2 + 1,
            completed_search: None,
            appended_rows: vec![LogPageRow {
                display_row: LOG_PAGE_SIZE * 2,
                line_index: LOG_PAGE_SIZE * 2,
                timestamp: None,
                source: None,
                text: "after toggle".to_owned(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            }],
            backfill_lines: None,
        },
    );
    harness.run_steps(2);

    assert!(
        scroll_state
            .borrow()
            .as_ref()
            .expect("log scroll area was rendered")
            .state
            .offset
            .y
            > locked_offset
    );
    assert!(
        harness.get_by_label("after toggle").rect().intersects(
            scroll_state
                .borrow()
                .as_ref()
                .expect("log scroll area was rendered")
                .inner_rect
        )
    );

    let following_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    harness.get_by_label("Lock to bottom").click();
    harness.run_steps(2);
    assert!(!windows.borrow()[&1].tail.is_following());
    harness.event(egui::Event::PointerGone);
    harness.run_steps(2);
    harness.ui_harness("pod_logs/pod_log_viewer_tail_lock_toggle_snapshot/unlocked");

    crate::ui::log_state::apply_store_result(
        &mut windows.borrow_mut(),
        LogStoreResult::Updated {
            window_id: 1,
            total_lines: LOG_PAGE_SIZE * 2 + 2,
            completed_search: None,
            appended_rows: vec![LogPageRow {
                display_row: LOG_PAGE_SIZE * 2 + 1,
                line_index: LOG_PAGE_SIZE * 2 + 1,
                timestamp: None,
                source: None,
                text: "after unlock".to_owned(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            }],
            backfill_lines: None,
        },
    );
    harness.run_steps(2);
    let unlocked_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    assert!(
        (unlocked_offset - following_offset).abs() <= TAIL_BOTTOM_TOLERANCE_POINTS,
        "manually disabling the lock must hold the viewport as rows arrive"
    );

    harness.get_by_label("Lock to bottom").click();
    harness.run_steps(2);
    let reattached_output = scroll_state.borrow();
    let reattached_output = reattached_output
        .as_ref()
        .expect("log scroll area was rendered");
    assert!(windows.borrow()[&1].tail.is_following());
    assert!(
        (reattached_output.content_size.y
            - reattached_output.inner_rect.height()
            - reattached_output.state.offset.y)
            <= TAIL_BOTTOM_TOLERANCE_POINTS
    );
}

#[test]
fn downward_scroll_attaches_when_a_live_row_arrives_in_the_same_frame() {
    let mut window = fully_loaded_log_window(LOG_PAGE_SIZE * 2);
    window.status = PodLogStatus::Following;
    let windows = Rc::new(RefCell::new(std::collections::BTreeMap::from([(
        1, window,
    )])));
    let windows_for_ui = windows.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
    let display_options_for_ui = display_options.clone();
    let scroll_state = Rc::new(RefCell::new(None));
    let scroll_state_for_ui = scroll_state.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        let mut windows = windows_for_ui.borrow_mut();
        let window = windows.get_mut(&1).expect("log window exists");
        *scroll_state_for_ui.borrow_mut() = Some(show_log_window_with_scroll_state(
            ctx,
            window,
            &mut display_options_for_ui.borrow_mut(),
            &log_store,
            &mut close_requested,
        ));
    });
    components::test_support::setup_egui(&mut harness);
    harness.run_steps(2);

    let scroll_position = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .inner_rect
        .center();
    harness.event(egui::Event::PointerMoved(scroll_position));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 20.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(2);
    assert!(!windows.borrow()[&1].tail.is_following());

    let (scroll_id, previous_maximum_offset) = {
        let output = scroll_state.borrow();
        let output = output.as_ref().expect("log scroll area was rendered");
        (
            output.id,
            output.content_size.y - output.inner_rect.height(),
        )
    };
    let mut persisted_scroll_state =
        egui::scroll_area::State::load(&harness.ctx, scroll_id).expect("scroll state is stored");
    persisted_scroll_state.offset.y = previous_maximum_offset - 1.0;
    persisted_scroll_state.store(&harness.ctx, scroll_id);

    crate::ui::log_state::apply_store_result(
        &mut windows.borrow_mut(),
        LogStoreResult::Updated {
            window_id: 1,
            total_lines: LOG_PAGE_SIZE * 2 + 3,
            completed_search: None,
            appended_rows: (0..3)
                .map(|offset| LogPageRow {
                    display_row: LOG_PAGE_SIZE * 2 + offset,
                    line_index: LOG_PAGE_SIZE * 2 + offset,
                    timestamp: None,
                    source: None,
                    text: format!("same frame {offset}"),
                    style_spans: Vec::new(),
                    match_ranges: Vec::new(),
                })
                .collect(),
            backfill_lines: None,
        },
    );
    harness.event(egui::Event::PointerMoved(scroll_position));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -24.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
    harness.step();
    assert!(windows.borrow()[&1].tail.is_following());

    harness.run_steps(2);
    let race_output = scroll_state.borrow();
    let race_output = race_output.as_ref().expect("log scroll area was rendered");
    assert!(
        (race_output.content_size.y - race_output.inner_rect.height() - race_output.state.offset.y)
            <= TAIL_BOTTOM_TOLERANCE_POINTS
    );
    assert!(
        harness.get_by_label("same frame 2").rect().intersects(
            scroll_state
                .borrow()
                .as_ref()
                .expect("log scroll area was rendered")
                .inner_rect
        )
    );
}

#[test]
fn programmatic_scroll_near_the_tail_does_not_attach_following() {
    let context = egui::Context::default();
    let mut window = fully_loaded_log_window(LOG_PAGE_SIZE * 2);
    assert!(!window.tail.toggle());
    window
        .tail
        .observe_scroll(300.0, 280.0, true, TAIL_BOTTOM_TOLERANCE_POINTS);
    window
        .tail
        .observe_scroll(400.0, 285.0, true, TAIL_BOTTOM_TOLERANCE_POINTS);
    let mut state = egui::scroll_area::State::default();
    state.offset = egui::vec2(0.0, 285.0);
    let output = egui::scroll_area::ScrollAreaOutput {
        inner: (),
        id: egui::Id::new("programmatic-scroll-near-tail"),
        state,
        content_size: egui::vec2(300.0, 500.0),
        inner_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 100.0)),
    };

    update_tail_follow_state(&context, &mut window, &output, false);

    assert!(!window.tail.is_following());
    assert!(!window.tail.scroll_requested());
    window
        .tail
        .observe_scroll(400.0, 310.0, true, TAIL_BOTTOM_TOLERANCE_POINTS);
    assert!(
        !window.tail.is_following(),
        "a programmatic navigation must not make a later wheel event rejoin a stale tail"
    );
}

#[test]
fn scrolling_down_without_reaching_a_changed_tail_keeps_manual_unlock() {
    let context = egui::Context::default();
    let mut window = fully_loaded_log_window(LOG_PAGE_SIZE * 2);
    assert!(!window.tail.toggle());
    window
        .tail
        .observe_scroll(300.0, 280.0, true, TAIL_BOTTOM_TOLERANCE_POINTS);
    let mut state = egui::scroll_area::State::default();
    state.offset = egui::vec2(0.0, 285.0);
    let output = egui::scroll_area::ScrollAreaOutput {
        inner: (),
        id: egui::Id::new("manual-unlock-near-changed-tail"),
        state,
        content_size: egui::vec2(300.0, 440.0),
        inner_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 100.0)),
    };

    update_tail_follow_state(&context, &mut window, &output, true);

    assert!(!window.tail.is_following());
    assert!(!window.tail.scroll_requested());
}

#[test]
fn following_viewer_rejoins_after_a_live_batch_and_downward_scroll() {
    let context = egui::Context::default();
    let mut window = fully_loaded_log_window(LOG_PAGE_SIZE * 2);
    let mut settled_state = egui::scroll_area::State::default();
    settled_state.offset = egui::vec2(0.0, 300.0);
    let settled_output = egui::scroll_area::ScrollAreaOutput {
        inner: (),
        id: egui::Id::new("following-live-batch-settled"),
        state: settled_state,
        content_size: egui::vec2(300.0, 400.0),
        inner_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 100.0)),
    };
    update_tail_follow_state(&context, &mut window, &settled_output, false);

    let mut batch_state = egui::scroll_area::State::default();
    batch_state.offset = egui::vec2(0.0, 330.0);
    let batch_output = egui::scroll_area::ScrollAreaOutput {
        inner: (),
        id: egui::Id::new("following-live-batch-downward-scroll"),
        state: batch_state,
        content_size: egui::vec2(300.0, 460.0),
        inner_rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 100.0)),
    };
    update_tail_follow_state(&context, &mut window, &batch_output, true);

    assert!(window.tail.is_following());
    assert!(
        window.tail.scroll_requested(),
        "reaching the former tail must request the newest tail after a multi-row batch"
    );
}

#[test]
fn new_log_window_requests_initial_tail_attachment() {
    let window = log_window(&["first record"]);

    assert!(window.tail.is_following());
    assert!(window.tail.scroll_requested());
}

#[test]
fn displayed_line_navigation_scrolls_the_viewer_and_snapshots_the_destination() {
    let window = Rc::new(RefCell::new(fully_loaded_log_window(512)));
    let window_for_ui = window.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
    let display_options_for_ui = display_options.clone();
    let scroll_state = Rc::new(RefCell::new(None));
    let scroll_state_for_ui = scroll_state.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        *scroll_state_for_ui.borrow_mut() = Some(show_log_window_with_scroll_state(
            ctx,
            &mut window_for_ui.borrow_mut(),
            &mut display_options_for_ui.borrow_mut(),
            &log_store,
            &mut close_requested,
        ));
    });
    components::test_support::setup_egui(&mut harness);
    harness.run_steps(2);

    window.borrow_mut().search.active_display_row = Some(511);
    harness.get_by_label("Previous displayed line").click();
    harness.run_steps(2);
    assert_eq!(window.borrow().search.active_display_row, Some(510));
    assert!(
        !window.borrow().tail.is_following(),
        "navigating to a row already visible at the bottom must release tail following"
    );
    let navigation_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    let mut updated_windows = std::collections::BTreeMap::from([(1, window.borrow().clone())]);
    crate::ui::log_state::apply_store_result(
        &mut updated_windows,
        LogStoreResult::Updated {
            window_id: 1,
            total_lines: 513,
            completed_search: None,
            appended_rows: vec![LogPageRow {
                display_row: 512,
                line_index: 512,
                timestamp: None,
                source: None,
                text: "after visible navigation".to_owned(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            }],
            backfill_lines: None,
        },
    );
    *window.borrow_mut() = updated_windows
        .remove(&1)
        .expect("updated log window exists");
    harness.run_steps(2);
    let offset_after_live_row = scroll_state
        .borrow()
        .as_ref()
        .expect("log scroll area was rendered")
        .state
        .offset
        .y;
    assert!(
        (offset_after_live_row - navigation_offset).abs() <= TAIL_BOTTOM_TOLERANCE_POINTS,
        "incoming rows must not move the viewport after visible-bottom navigation"
    );

    window.borrow_mut().search.active_display_row = Some(0);
    harness.get_by_label("Previous displayed line").click();
    harness.run_steps(2);
    assert_eq!(window.borrow().search.active_display_row, Some(512));
    assert!(
        harness
            .get_by_label("after visible navigation")
            .rect()
            .intersects(
                scroll_state
                    .borrow()
                    .as_ref()
                    .expect("log scroll area was rendered")
                    .inner_rect
            ),
        "wrapped previous navigation must move the viewport"
    );

    window.borrow_mut().search.active_display_row = Some(399);
    harness.get_by_label("Next displayed line").click();
    harness.run_steps(2);

    let scroll_state = scroll_state.borrow();
    let output = scroll_state.as_ref().expect("log scroll area was rendered");
    assert_eq!(window.borrow().search.active_display_row, Some(400));
    assert_eq!(window.borrow().search.scroll_to_display_row, None);
    assert!(
        output.state.offset.y > 0.0,
        "navigation must move the viewport"
    );
    assert!(
        harness
            .get_by_label("line 400")
            .rect()
            .intersects(output.inner_rect),
        "the requested line must be visible after navigation"
    );
    harness.ui_harness(
        "pod_logs/pod_log_viewer_displayed_line_navigation_snapshot/next_displayed_line",
    );
}

#[test]
fn resolved_match_navigation_scrolls_the_viewer_and_snapshots_the_destination() {
    let window = Rc::new(RefCell::new(fully_loaded_log_window(512)));
    let window_for_ui = window.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
    let display_options_for_ui = display_options.clone();
    let scroll_state = Rc::new(RefCell::new(None));
    let scroll_state_for_ui = scroll_state.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        *scroll_state_for_ui.borrow_mut() = Some(show_log_window_with_scroll_state(
            ctx,
            &mut window_for_ui.borrow_mut(),
            &mut display_options_for_ui.borrow_mut(),
            &log_store,
            &mut close_requested,
        ));
    });
    components::test_support::setup_egui(&mut harness);
    harness.run_steps(2);

    {
        let mut window = window.borrow_mut();
        let page = window
            .pages
            .get_mut(&LogPageKey {
                generation: 0,
                filter_matches: false,
                page_start: LOG_PAGE_SIZE,
            })
            .expect("target page is loaded");
        let target = &mut page.rows[400 - LOG_PAGE_SIZE];
        target.text = "needle line 400".to_owned();
        target.match_ranges = vec![(0, "needle".len())];
        window.search.query = "needle".to_owned();
        window.search.match_count = 2;
        window.search.active_match = Some(0);
    }
    harness.get_by_label("Previous matching line").click();
    harness.step();
    assert_eq!(window.borrow().search.active_match, Some(1));
    window.borrow_mut().search.active_match = Some(0);
    harness.get_by_label("Next matching line").click();
    harness.step();
    assert_eq!(window.borrow().search.active_match, Some(1));
    // The store resolves the selected match asynchronously. State-level
    // coverage below verifies that this result maps unfiltered matches to
    // their source line and filtered matches to their match row.
    window.borrow_mut().search.active_display_row = Some(400);
    window.borrow_mut().search.scroll_to_display_row = Some(400);
    harness.run_steps(2);

    let scroll_state = scroll_state.borrow();
    let output = scroll_state.as_ref().expect("log scroll area was rendered");
    assert_eq!(window.borrow().search.active_match, Some(1));
    assert_eq!(window.borrow().search.active_display_row, Some(400));
    assert_eq!(window.borrow().search.scroll_to_display_row, None);
    assert!(
        output.state.offset.y > 0.0,
        "match navigation must move the viewport"
    );
    assert!(
        harness
            .get_by_label("needle line 400")
            .rect()
            .intersects(output.inner_rect),
        "the resolved matching line must be visible"
    );
    harness
        .ui_harness("pod_logs/pod_log_viewer_match_navigation_snapshot/resolved_match_destination");
}

#[test]
fn log_navigation_wraps_at_both_ends() {
    let log_store = LogStoreService::default();
    let mut window = log_window(&["zero", "one", "two"]);
    window.search.match_count = 3;

    advance_log_line(&mut window, false);
    assert_eq!(window.search.active_display_row, Some(2));
    advance_log_line(&mut window, true);
    assert_eq!(window.search.active_display_row, Some(0));

    advance_log_match(&mut window, &log_store, false);
    assert_eq!(window.search.active_match, Some(2));
    advance_log_match(&mut window, &log_store, true);
    assert_eq!(window.search.active_match, Some(0));
}

#[test]
fn loading_a_wide_page_does_not_move_the_bottom_offset() {
    let wide_line = "x".repeat(4 * 1024);
    let mut window = log_window(&[]);
    window.total_lines = LOG_PAGE_SIZE * 2;
    window.status = PodLogStatus::Finished;
    window.insert_page(
        LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: LOG_PAGE_SIZE,
        },
        (LOG_PAGE_SIZE..window.total_lines)
            .map(|line_index| LogPageRow {
                display_row: line_index,
                line_index,
                timestamp: None,
                source: None,
                text: wide_line.clone(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            })
            .collect(),
    );
    window.search.scroll_to_display_row = Some(window.total_lines - 1);

    let context = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    let mut display_options = LogDisplayOptions::default();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut render = |window: &mut PodLogWindowState| {
        let mut scroll_state = None;
        let mut output = context.run_ui(input.clone(), |ctx| {
            scroll_state = Some(show_log_window_with_scroll_state(
                ctx,
                window,
                &mut display_options,
                &log_store,
                &mut close_requested,
            ));
        });
        output.textures_delta.clear();
        scroll_state.expect("log scroll area was rendered")
    };

    let _ = render(&mut window);
    let loaded_offset = render(&mut window);
    window.pages.clear();
    window.page_order.clear();
    window.page_cache_bytes = 0;
    let loading_offset = render(&mut window);

    assert_eq!(loading_offset.inner_rect, loaded_offset.inner_rect);
    assert_eq!(loading_offset.content_size, loaded_offset.content_size);
    assert_eq!(loading_offset.state.offset.y, loaded_offset.state.offset.y);
}

#[test]
fn loading_a_narrow_page_does_not_move_the_bottom_offset() {
    let mut window = log_window(&[]);
    window.total_lines = LOG_PAGE_SIZE * 2;
    window.status = PodLogStatus::Finished;
    window.insert_page(
        LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: LOG_PAGE_SIZE,
        },
        (LOG_PAGE_SIZE..window.total_lines)
            .map(|line_index| LogPageRow {
                display_row: line_index,
                line_index,
                timestamp: None,
                source: None,
                text: format!("line {line_index}"),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            })
            .collect(),
    );
    window.search.scroll_to_display_row = Some(window.total_lines - 1);

    let context = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    let mut display_options = LogDisplayOptions::default();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut render = |window: &mut PodLogWindowState| {
        let mut scroll_state = None;
        let mut output = context.run_ui(input.clone(), |ctx| {
            scroll_state = Some(show_log_window_with_scroll_state(
                ctx,
                window,
                &mut display_options,
                &log_store,
                &mut close_requested,
            ));
        });
        output.textures_delta.clear();
        scroll_state.expect("log scroll area was rendered")
    };

    let _ = render(&mut window);
    let loaded_offset = render(&mut window);
    window.pages.clear();
    window.page_order.clear();
    window.page_cache_bytes = 0;
    let loading_offset = render(&mut window);

    assert_eq!(loading_offset.inner_rect, loaded_offset.inner_rect);
    assert_eq!(loading_offset.content_size, loaded_offset.content_size);
    assert_eq!(loading_offset.state.offset.y, loaded_offset.state.offset.y);
}

#[test]
fn first_unfiltered_page_ends_the_initial_spool_state() {
    let mut window = log_window(&[]);
    window.total_lines = 1;
    window.initial_page_loaded = false;

    assert!(initial_spool_is_pending(&window));
    window.insert_page(
        LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: 0,
        },
        vec![LogPageRow {
            display_row: 0,
            line_index: 0,
            timestamp: None,
            source: None,
            text: "first line".to_owned(),
            style_spans: Vec::new(),
            match_ranges: Vec::new(),
        }],
    );

    assert!(!initial_spool_is_pending(&window));
}

#[test]
fn first_wide_page_does_not_change_the_vertical_viewport() {
    let wide_line = "x".repeat(4 * 1024);
    let mut window = log_window(&[]);
    window.total_lines = LOG_PAGE_SIZE * 2;
    window.status = PodLogStatus::Finished;
    window.search.scroll_to_display_row = Some(window.total_lines - 1);

    let context = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    let mut display_options = LogDisplayOptions::default();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut render = |window: &mut PodLogWindowState| {
        let mut scroll_state = None;
        let mut output = context.run_ui(input.clone(), |ctx| {
            scroll_state = Some(show_log_window_with_scroll_state(
                ctx,
                window,
                &mut display_options,
                &log_store,
                &mut close_requested,
            ));
        });
        output.textures_delta.clear();
        scroll_state.expect("log scroll area was rendered")
    };

    let _ = render(&mut window);
    let loading_offset = render(&mut window);
    window.insert_page(
        LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: LOG_PAGE_SIZE,
        },
        (LOG_PAGE_SIZE..window.total_lines)
            .map(|line_index| LogPageRow {
                display_row: line_index,
                line_index,
                timestamp: None,
                source: None,
                text: wide_line.clone(),
                style_spans: Vec::new(),
                match_ranges: Vec::new(),
            })
            .collect(),
    );
    let loaded_offset = render(&mut window);

    assert_eq!(loaded_offset.inner_rect, loading_offset.inner_rect);
    assert_eq!(loaded_offset.content_size.y, loading_offset.content_size.y);
    assert_eq!(loaded_offset.state.offset.y, loading_offset.state.offset.y);
}
