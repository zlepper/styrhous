use super::*;

#[test]
fn pod_log_viewer_keyboard_caret_after_arrow_down_snapshot() {
    let window = Rc::new(RefCell::new(log_window(&[
        "first row",
        "second row",
        "third row",
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
    harness.get_by_label("first row").click();
    harness.step();
    harness.key_press(egui::Key::ArrowDown);
    harness.step();
    harness.ui_harness("pod_logs/pod_log_viewer_keyboard_caret_after_arrow_down_snapshot/keyboard_caret_after_arrow_down");
}

#[test]
fn pod_log_viewer_keyboard_caret_after_arrow_up_snapshot() {
    let window = Rc::new(RefCell::new(log_window(&[
        "first row",
        "second row",
        "third row",
    ])));
    let window_for_ui = window.clone();
    let display_options = Rc::new(RefCell::new(LogDisplayOptions::default()));
    let display_options_for_ui = display_options.clone();
    let caret_has_focus = Rc::new(RefCell::new(false));
    let caret_has_focus_for_ui = caret_has_focus.clone();
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
        *caret_has_focus_for_ui.borrow_mut() =
            ctx.memory(|memory| memory.has_focus(egui::Id::new(("pod-log-caret", 1))));
    });
    components::test_support::setup_egui(&mut harness);
    harness.run_steps(2);
    harness.get_by_label("second row").click();
    harness.step();
    harness.key_press(egui::Key::ArrowUp);
    harness.step();
    harness.run_steps(2);

    assert_eq!(caret_focus(&window.borrow()).display_row, 0);
    assert!(*caret_has_focus.borrow(), "ArrowUp must retain caret focus");
    harness.ui_harness("pod_logs/pod_log_viewer_keyboard_caret_after_arrow_up_snapshot/keyboard_caret_after_arrow_up");
}

#[test]
fn pod_log_viewer_keyboard_caret_snapshot() {
    let window = Rc::new(RefCell::new(log_window(&[
        "2026-08-11T10:00:00Z first readonly line",
        "2026-08-11T10:00:01Z focused caret line",
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
        / "       1  2026-08-11T10:00:01Z  focused caret line"
            .chars()
            .count() as f32;
    let prefix_width = "       1  2026-08-11T10:00:01Z  ".chars().count() as f32 * character_width;
    let click_position = egui::pos2(
        label.rect().left() + prefix_width + "focused ".chars().count() as f32 * character_width,
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
    harness.ui_harness("pod_logs/pod_log_viewer_keyboard_caret_snapshot/keyboard_caret");
}
