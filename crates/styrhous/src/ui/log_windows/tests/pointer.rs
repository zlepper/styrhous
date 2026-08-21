use super::*;

#[test]
fn clicking_a_log_row_focuses_the_keyboard_caret() {
    let window = Rc::new(RefCell::new(log_window(&["clickable log line"])));
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
    harness.get_by_label("clickable log line").click();
    harness.step();
    let clicked_position = window
        .borrow()
        .selection
        .expect("clicking a log row places the caret")
        .focus;

    harness.key_press(egui::Key::ArrowRight);
    harness.step();
    let moved_position = window
        .borrow()
        .selection
        .expect("focused log caret remains present")
        .focus;

    assert_eq!(moved_position.display_row, clicked_position.display_row);
    assert!(moved_position.byte_offset > clicked_position.byte_offset);
}

#[test]
fn hovering_a_log_row_uses_a_text_cursor() {
    let window = Rc::new(RefCell::new(log_window(&["hoverable log line"])));
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
    let hover_position = harness.get_by_label("hoverable log line").rect().center();
    harness.event(egui::Event::PointerMoved(hover_position));
    harness.step();

    assert_eq!(
        harness.output().platform_output.cursor_icon,
        egui::CursorIcon::Text
    );
}

#[test]
fn focused_log_caret_moves_between_rows_in_the_ui() {
    let window = Rc::new(RefCell::new(log_window(&["first row", "second row"])));
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
    harness.get_by_label("first row").click();
    harness.step();
    harness.key_press(egui::Key::ArrowDown);
    harness.step();

    assert_eq!(caret_focus(&window.borrow()).display_row, 1);
}

#[test]
fn clicking_a_prefixed_log_row_places_the_caret_in_its_message() {
    let window = Rc::new(RefCell::new(log_window(&[
        "2026-08-11T10:00:00Z focused caret line",
    ])));
    let window_for_ui = window.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions {
        show_line_numbers: true,
        show_timestamps: true,
        ..LogDisplayOptions::default()
    }));
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
    let label = harness.get_by_label_contains("focused caret line");
    let character_width = label.rect().width()
        / "       0  2026-08-11T10:00:00Z  focused caret line"
            .chars()
            .count() as f32;
    let prefix_width = "       0  2026-08-11T10:00:00Z  ".chars().count() as f32 * character_width;
    let clicked_column = 7;
    let click_position = egui::pos2(
        label.rect().left() + prefix_width + clicked_column as f32 * character_width,
        label.rect().center().y,
    );
    harness.event(egui::Event::PointerMoved(click_position));
    harness.event(egui::Event::PointerButton {
        pos: click_position,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.event(egui::Event::PointerButton {
        pos: click_position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();

    let position = caret_focus(&window.borrow());
    assert_eq!(position.display_row, 0);
    assert_eq!(position.byte_offset, clicked_column);
}

#[test]
fn dragging_right_to_left_in_a_prefixed_log_row_keeps_the_caret_at_the_released_message_column() {
    let timestamp = "2026-08-11T10:00:00Z";
    // The marker text makes the visible endpoint unambiguous: the white
    // caret must be immediately before `RELEASE`, not inside the range.
    let message = format!(
        "--RELEASE--selected-text--ANCHOR--{}",
        "after-selection-".repeat(128)
    );
    let line = format!("{timestamp} {message}");
    let window = Rc::new(RefCell::new(log_window(&[&line])));
    let window_for_ui = window.clone();
    let character_width = Rc::new(RefCell::new(0.0));
    let character_width_for_ui = character_width.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions {
        show_line_numbers: true,
        ..LogDisplayOptions::default()
    }));
    let display_options_for_ui = display_options.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        *character_width_for_ui.borrow_mut() =
            ctx.fonts_mut(|fonts| fonts.glyph_width(&egui::FontId::monospace(LOG_FONT_SIZE), '0'));
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
    let label = harness.get_by_label_contains("--RELEASE--selected-text--ANCHOR--");
    let character_width = *character_width.borrow();
    let start_column = message.find("ANCHOR").expect("anchor marker exists");
    let moved_column = message
        .find("selected-text")
        .expect("selection marker exists");
    let end_column = message.find("RELEASE").expect("release marker exists");
    let start = egui::pos2(
        label.rect().left() + start_column as f32 * character_width,
        label.rect().center().y,
    );
    let moved = egui::pos2(
        label.rect().left() + moved_column as f32 * character_width,
        label.rect().center().y,
    );
    let end = egui::pos2(
        label.rect().left() + end_column as f32 * character_width,
        label.rect().center().y,
    );

    harness.event(egui::Event::PointerMoved(start));
    harness.event(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    harness.event(egui::Event::PointerMoved(moved));
    harness.step();
    harness.event(egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();

    assert_eq!(
        window.borrow().selection,
        Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 0,
                byte_offset: start_column,
            },
            focus: LogTextPosition {
                display_row: 0,
                byte_offset: end_column,
            },
        })
    );
    // The release event uses the pointer position from the event itself;
    // moving the cursor away only prevents its image from hiding the
    // caret in the visual fixture.
    harness.event(egui::Event::PointerMoved(egui::pos2(400.0, 200.0)));
    harness.step();
    harness.ui_harness(
        "pod_logs/pod_log_viewer_prefixed_drag_selection_snapshot/prefixed_drag_selection",
    );
}

#[test]
fn pressing_a_second_log_row_immediately_replaces_the_previous_caret() {
    let window = Rc::new(RefCell::new(log_window(&[
        "first log row",
        "second log row",
    ])));
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
    harness.get_by_label("first log row").click();
    harness.step();
    let second = harness.get_by_label("second log row").rect().center();
    harness.event(egui::Event::PointerMoved(second));
    harness.event(egui::Event::PointerButton {
        pos: second,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();

    let selection = window
        .borrow()
        .selection
        .expect("pressing a row positions the caret immediately");
    assert_eq!(selection.anchor, selection.focus);
    assert_eq!(selection.focus.display_row, 1);
}

#[test]
fn dragging_between_prefixed_log_rows_tracks_the_message_columns() {
    let first_timestamp = "2026-08-11T10:00:00Z";
    let second_timestamp = "2026-08-11T10:00:01Z";
    let first_message = "first selected row";
    let second_message = "second selected row";
    let first_line = format!("{first_timestamp} {first_message}");
    let second_line = format!("{second_timestamp} {second_message}");
    let window = Rc::new(RefCell::new(log_window(&[&first_line, &second_line])));
    let window_for_ui = window.clone();
    let character_width = Rc::new(RefCell::new(0.0));
    let character_width_for_ui = character_width.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions {
        show_line_numbers: true,
        show_timestamps: true,
        ..LogDisplayOptions::default()
    }));
    let display_options_for_ui = display_options.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        *character_width_for_ui.borrow_mut() =
            ctx.fonts_mut(|fonts| fonts.glyph_width(&egui::FontId::monospace(LOG_FONT_SIZE), '0'));
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
    let character_width = *character_width.borrow();
    let first_prefix = format!("{:>6}  {first_timestamp}  ", 0);
    let second_prefix = format!("{:>6}  {second_timestamp}  ", 1);
    let first_label_text = format!("{first_prefix}{first_message}");
    let second_label_text = format!("{second_prefix}{second_message}");
    let first_label = harness.get_by_label(&first_label_text);
    let second_label = harness.get_by_label(&second_label_text);
    let start_column = 2;
    let end_column = 9;
    let start = egui::pos2(
        first_label.rect().left()
            + first_prefix.chars().count() as f32 * character_width
            + start_column as f32 * character_width,
        first_label.rect().center().y,
    );
    let end = egui::pos2(
        second_label.rect().left()
            + second_prefix.chars().count() as f32 * character_width
            + end_column as f32 * character_width,
        second_label.rect().center().y,
    );

    harness.event(egui::Event::PointerMoved(start));
    harness.event(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    harness.event(egui::Event::PointerMoved(end));
    harness.step();
    harness.event(egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();

    assert_eq!(
        window.borrow().selection,
        Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 0,
                byte_offset: start_column,
            },
            focus: LogTextPosition {
                display_row: 1,
                byte_offset: end_column,
            },
        })
    );
}

#[test]
fn clicking_a_horizontally_scrolled_log_row_uses_the_original_text_column() {
    let line = "x".repeat(2_000);
    let window = Rc::new(RefCell::new(log_window(&[&line])));
    let window_for_ui = window.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
    let display_options_for_ui = display_options.clone();
    let scroll_state = Rc::new(RefCell::new(None));
    let scroll_state_for_ui = scroll_state.clone();
    let viewport = Rc::new(RefCell::new(egui::Rect::NOTHING));
    let viewport_for_ui = viewport.clone();
    let character_width = Rc::new(RefCell::new(0.0));
    let character_width_for_ui = character_width.clone();
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        *character_width_for_ui.borrow_mut() =
            ctx.fonts_mut(|fonts| fonts.glyph_width(&egui::FontId::monospace(LOG_FONT_SIZE), '0'));
        let output = show_log_window_with_scroll_state(
            ctx,
            &mut window_for_ui.borrow_mut(),
            &mut display_options_for_ui.borrow_mut(),
            &log_store,
            &mut close_requested,
        );
        *scroll_state_for_ui.borrow_mut() = Some(output.state);
        *viewport_for_ui.borrow_mut() = output.inner_rect;
    });
    components::test_support::setup_egui(&mut harness);
    harness.run_steps(2);
    harness.event(egui::Event::PointerMoved(egui::pos2(400.0, 100.0)));
    harness.step();
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(-500.0, 0.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    harness.run_steps(2);

    let viewport = *viewport.borrow();
    let scroll_offset = scroll_state
        .borrow()
        .as_ref()
        .expect("the log scroll area was rendered")
        .offset;
    assert!(
        scroll_offset.x > 0.0,
        "the log view was horizontally scrolled"
    );
    let click_position = egui::pos2(viewport.left() + 160.0, viewport.top() + 8.0);
    let expected_column = ((scroll_offset.x + click_position.x - viewport.left())
        / *character_width.borrow())
    .round() as usize;
    harness.event(egui::Event::PointerMoved(click_position));
    harness.event(egui::Event::PointerButton {
        pos: click_position,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.event(egui::Event::PointerButton {
        pos: click_position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();

    assert_eq!(
        caret_focus(&window.borrow()),
        LogTextPosition {
            display_row: 0,
            byte_offset: expected_column,
        }
    );
}
