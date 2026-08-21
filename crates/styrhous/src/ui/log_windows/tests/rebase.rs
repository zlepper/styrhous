use super::*;

#[test]
fn pod_log_viewer_rebase_keeps_scrolled_wide_text_in_place() {
    let live_lines = (0..LOG_PAGE_SIZE)
        .map(|line_index| format!("record {line_index:03} :: ").repeat(32))
        .collect::<Vec<_>>();
    let live_line_refs = live_lines.iter().map(String::as_str).collect::<Vec<_>>();

    // The worker only controls stream lifecycle now; the storage service
    // owns log data. Drive both boundaries explicitly so this test fixes
    // the exact frame in which the source swap is rendered.
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        1,
        "api-0".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let mut worker = MockWorker {
        results: VecDeque::from([
            Box::new(crate::worker::PodLogStreamStarted { log_window_id: 1 })
                as crate::worker::WorkerResultBox,
        ]),
        commands: Vec::new(),
    };
    let _ = state.update(&mut worker);
    state.log_windows.insert(1, log_window(&live_line_refs));

    let state = Rc::new(RefCell::new(state));
    let rendered_scroll_offset = Rc::new(RefCell::new(egui::Vec2::ZERO));
    let rendered_scroll_id = Rc::new(RefCell::new(None));
    let state_for_ui = state.clone();
    let rendered_scroll_offset_for_ui = rendered_scroll_offset.clone();
    let rendered_scroll_id_for_ui = rendered_scroll_id.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        let mut state = state_for_ui.borrow_mut();
        let UiState {
            log_windows,
            log_display_options,
            ..
        } = &mut *state;
        let window = log_windows.get_mut(&1).expect("log window exists");
        let output = show_log_window_with_scroll_state(
            ctx,
            window,
            log_display_options,
            &log_store,
            &mut close_requested,
        );
        *rendered_scroll_offset_for_ui.borrow_mut() = output.state.offset;
        *rendered_scroll_id_for_ui.borrow_mut() = Some(output.id);
    });
    components::test_support::setup_egui(&mut harness);
    harness.run_steps(2);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(400.0, 180.0)));
    harness.step();
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::Vec2::ZERO,
        phase: egui::TouchPhase::Start,
        modifiers: egui::Modifiers::default(),
    });
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(-600.0, -700.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(3);

    // Snap the fixture to a physical-pixel x offset before capturing the
    // two frames. Real scroll input can end between pixels; this isolates
    // source-rebase behavior from wheel-event rounding.
    let scroll_id = rendered_scroll_id
        .borrow()
        .expect("log scroll area was rendered");
    let mut scroll_state = egui::scroll_area::State::load(&harness.ctx, scroll_id)
        .expect("log scroll state was persisted");
    scroll_state.offset.x = 600.0;
    scroll_state.store(&harness.ctx, scroll_id);
    harness.run_steps(2);

    let before_offset = *rendered_scroll_offset.borrow();
    let old_visible_row = state.borrow().log_windows[&1].visible_top_display_row;
    assert!(
        before_offset.x > 0.0,
        "the test must exercise horizontal scroll"
    );
    assert!(
        old_visible_row > 0,
        "the test must exercise vertical scroll"
    );
    harness.ui_harness(
        "pod_logs/pod_log_viewer_rebase_keeps_scrolled_wide_text_in_place/rebase_before",
    );

    // A full history request returned 100 older records plus the complete
    // initial tail. The visible tail record therefore moves down by 100
    // logical rows, but its rendered position must not move.
    state
        .borrow_mut()
        .apply_log_store_result(LogStoreResult::Rebased {
            window_id: 1,
            total_lines: LOG_PAGE_SIZE + 100,
            history_lines: LOG_PAGE_SIZE + 100,
            live_start: LOG_PAGE_SIZE,
        });
    state
        .borrow_mut()
        .apply_log_store_result(LogStoreResult::PageLoaded {
            window_id: 1,
            generation: 0,
            filter_matches: false,
            page_start: LOG_PAGE_SIZE,
            total_rows: LOG_PAGE_SIZE + 100,
            rows: (LOG_PAGE_SIZE..LOG_PAGE_SIZE + 100)
                .map(|display_row| {
                    let live_index = display_row - 100;
                    LogPageRow {
                        display_row,
                        line_index: display_row,
                        timestamp: None,
                        text: live_lines[live_index].clone(),
                        style_spans: Vec::new(),
                        match_ranges: Vec::new(),
                    }
                })
                .collect(),
        });
    harness.run_steps(3);

    let after_offset = *rendered_scroll_offset.borrow();
    let expected_row = old_visible_row + 100;
    assert_eq!(
        state.borrow().log_windows[&1].visible_top_display_row,
        expected_row,
        "the rendered anchor must refer to the same record after rebasing"
    );
    assert!(
        (after_offset.x - before_offset.x).abs() <= 0.1,
        "rebasing must preserve the horizontal scroll offset: before={before_offset:?}, after={after_offset:?}"
    );
    harness.ui_harness(
        "pod_logs/pod_log_viewer_rebase_keeps_scrolled_wide_text_in_place/rebase_after",
    );

    // Supply the newly prepended page, then scroll up into it. This
    // verifies that the post-rebase cache can move in the opposite
    // direction without losing the horizontal position or showing the
    // old tail at the wrong logical index.
    state
        .borrow_mut()
        .apply_log_store_result(LogStoreResult::PageLoaded {
            window_id: 1,
            generation: 0,
            filter_matches: false,
            page_start: 0,
            total_rows: LOG_PAGE_SIZE + 100,
            rows: (0..LOG_PAGE_SIZE)
                .map(|display_row| {
                    let text = if display_row < 100 {
                        format!("history {display_row:03} :: ").repeat(32)
                    } else {
                        live_lines[display_row - 100].clone()
                    };
                    LogPageRow {
                        display_row,
                        line_index: display_row,
                        timestamp: None,
                        text,
                        style_spans: Vec::new(),
                        match_ranges: Vec::new(),
                    }
                })
                .collect(),
        });
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(400.0, 180.0)));
    harness.step();
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 10_000.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(3);
    let history_scroll_offset = *rendered_scroll_offset.borrow();
    assert!(
        state.borrow().log_windows[&1].visible_top_display_row < 100,
        "upward scrolling must reach the prepended history segment"
    );
    assert!(
        (history_scroll_offset.x - before_offset.x).abs() <= 0.1,
        "upward scrolling through history must retain the horizontal offset"
    );
    harness.ui_harness("pod_logs/pod_log_viewer_rebase_keeps_scrolled_wide_text_in_place/rebase_history_after_scroll");
}
