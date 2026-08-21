use super::*;

#[test]
fn keyboard_caret_moves_by_character_word_and_line() {
    let log_store = LogStoreService::default();
    let mut window = log_window(&["alpha beta", "xy", "012345"]);
    select_log_position(&mut window, 0, 5);

    assert!(move_log_caret(
        &mut window,
        &log_store,
        3,
        1,
        egui::Key::ArrowRight,
        egui::Modifiers::NONE,
        1.0,
    ));
    assert_eq!(window.selection.unwrap().focus.byte_offset, 6);

    assert!(move_log_caret(
        &mut window,
        &log_store,
        3,
        1,
        egui::Key::ArrowRight,
        egui::Modifiers::CTRL,
        2.0,
    ));
    assert_eq!(window.selection.unwrap().focus.byte_offset, 10);

    assert!(move_log_caret(
        &mut window,
        &log_store,
        3,
        1,
        egui::Key::ArrowRight,
        egui::Modifiers::NONE,
        3.0,
    ));
    assert_eq!(window.selection.unwrap().focus.display_row, 1);
    assert_eq!(window.selection.unwrap().focus.byte_offset, 0);

    assert!(move_log_caret(
        &mut window,
        &log_store,
        3,
        1,
        egui::Key::ArrowDown,
        egui::Modifiers::NONE,
        4.0,
    ));
    assert_eq!(window.selection.unwrap().focus.display_row, 2);
    assert_eq!(window.selection.unwrap().focus.byte_offset, 0);
}

#[test]
fn keyboard_caret_shift_extends_and_plain_arrow_collapses_selection() {
    let log_store = LogStoreService::default();
    let mut window = log_window(&["abcdef"]);
    select_log_position(&mut window, 0, 2);

    assert!(move_log_caret(
        &mut window,
        &log_store,
        1,
        1,
        egui::Key::ArrowRight,
        egui::Modifiers::SHIFT,
        1.0,
    ));
    assert_eq!(
        window.selection,
        Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 0,
                byte_offset: 2,
            },
            focus: LogTextPosition {
                display_row: 0,
                byte_offset: 3,
            },
        })
    );

    assert!(move_log_caret(
        &mut window,
        &log_store,
        1,
        1,
        egui::Key::ArrowLeft,
        egui::Modifiers::NONE,
        2.0,
    ));
    assert_eq!(
        window.selection,
        Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 0,
                byte_offset: 2,
            },
            focus: LogTextPosition {
                display_row: 0,
                byte_offset: 2,
            },
        })
    );
}

#[test]
fn keyboard_caret_moves_in_every_direction_and_preserves_vertical_column() {
    let log_store = LogStoreService::default();
    let mut window = log_window(&["abc", "d", "abcdef"]);
    select_log_position(&mut window, 1, 1);

    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowLeft,
        egui::Modifiers::NONE,
        1,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 1,
            byte_offset: 0
        }
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowLeft,
        egui::Modifiers::NONE,
        1,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 0,
            byte_offset: 3
        }
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowUp,
        egui::Modifiers::NONE,
        1,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 0,
            byte_offset: 3
        }
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowDown,
        egui::Modifiers::NONE,
        1,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 1,
            byte_offset: 1
        }
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowDown,
        egui::Modifiers::NONE,
        1,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 2,
            byte_offset: 3
        }
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowUp,
        egui::Modifiers::NONE,
        1,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 1,
            byte_offset: 1
        }
    );

    select_log_position(&mut window, 0, 3);
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowRight,
        egui::Modifiers::NONE,
        1,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 1,
            byte_offset: 0
        }
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowRight,
        egui::Modifiers::NONE,
        1,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 1,
            byte_offset: 1
        }
    );
}

#[test]
fn keyboard_caret_word_navigation_and_shift_control_selection_work_in_both_directions() {
    let log_store = LogStoreService::default();
    let mut window = log_window(&["alpha beta gamma"]);
    select_log_position(&mut window, 0, 11);

    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowLeft,
        egui::Modifiers::CTRL,
        1,
    );
    assert_eq!(caret_focus(&window).byte_offset, 6);
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowRight,
        egui::Modifiers::CTRL,
        1,
    );
    assert_eq!(caret_focus(&window).byte_offset, 10);

    select_log_position(&mut window, 0, 6);
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowRight,
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        1,
    );
    assert_eq!(
        window.selection.unwrap().normalized(),
        (
            LogTextPosition {
                display_row: 0,
                byte_offset: 6
            },
            LogTextPosition {
                display_row: 0,
                byte_offset: 10
            },
        )
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowLeft,
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
        1,
    );
    assert_eq!(
        window.selection,
        Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 0,
                byte_offset: 6
            },
            focus: LogTextPosition {
                display_row: 0,
                byte_offset: 6
            },
        })
    );
}
