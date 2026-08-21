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

    window.borrow_mut().search.active_display_row = Some(0);
    harness.get_by_label("Previous displayed line").click();
    harness.run_steps(2);
    assert_eq!(window.borrow().search.active_display_row, Some(511));
    assert!(
        harness.get_by_label("line 511").rect().intersects(
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
