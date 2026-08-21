use super::*;

#[test]
fn keyboard_caret_supports_line_document_page_and_select_all_navigation() {
    let log_store = LogStoreService::default();
    let mut window = log_window(&["zero", "one", "two", "three", "four", "five"]);
    select_log_position(&mut window, 3, 2);

    move_key(
        &mut window,
        &log_store,
        egui::Key::Home,
        egui::Modifiers::NONE,
        2,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 3,
            byte_offset: 0
        }
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::End,
        egui::Modifiers::NONE,
        2,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 3,
            byte_offset: 5
        }
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::PageUp,
        egui::Modifiers::NONE,
        2,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 1,
            byte_offset: 3
        }
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::PageDown,
        egui::Modifiers::NONE,
        2,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 3,
            byte_offset: 5
        }
    );

    select_log_position(&mut window, 3, 2);
    move_key(
        &mut window,
        &log_store,
        egui::Key::Home,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        2,
    );
    assert_eq!(
        window.selection.unwrap().normalized(),
        (
            LogTextPosition {
                display_row: 0,
                byte_offset: 0
            },
            LogTextPosition {
                display_row: 3,
                byte_offset: 2
            },
        )
    );

    select_log_position(&mut window, 2, 1);
    move_key(
        &mut window,
        &log_store,
        egui::Key::End,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        2,
    );
    assert_eq!(
        window.selection.unwrap().normalized(),
        (
            LogTextPosition {
                display_row: 2,
                byte_offset: 1
            },
            LogTextPosition {
                display_row: 5,
                byte_offset: 4
            },
        )
    );

    select_log_position(&mut window, 3, 2);
    move_key(
        &mut window,
        &log_store,
        egui::Key::PageDown,
        egui::Modifiers::SHIFT,
        2,
    );
    assert_eq!(
        window.selection.unwrap().normalized(),
        (
            LogTextPosition {
                display_row: 3,
                byte_offset: 2
            },
            LogTextPosition {
                display_row: 5,
                byte_offset: 2
            },
        )
    );

    select_log_position(&mut window, 2, 1);
    move_key(
        &mut window,
        &log_store,
        egui::Key::PageUp,
        egui::Modifiers::SHIFT,
        2,
    );
    assert_eq!(
        window.selection.unwrap().normalized(),
        (
            LogTextPosition {
                display_row: 0,
                byte_offset: 1
            },
            LogTextPosition {
                display_row: 2,
                byte_offset: 1
            },
        )
    );

    select_log_position(&mut window, 2, 1);
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowUp,
        egui::Modifiers::COMMAND,
        2,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 0,
            byte_offset: 0
        }
    );
    select_log_position(&mut window, 2, 1);
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowDown,
        egui::Modifiers::COMMAND,
        2,
    );
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 5,
            byte_offset: 4
        }
    );

    select_log_position(&mut window, 2, 1);
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowUp,
        egui::Modifiers::SHIFT,
        2,
    );
    assert_eq!(
        window.selection.unwrap().normalized(),
        (
            LogTextPosition {
                display_row: 1,
                byte_offset: 1
            },
            LogTextPosition {
                display_row: 2,
                byte_offset: 1
            },
        )
    );
    move_key(
        &mut window,
        &log_store,
        egui::Key::ArrowDown,
        egui::Modifiers::SHIFT,
        2,
    );
    assert_eq!(
        window.selection,
        Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 2,
                byte_offset: 1
            },
            focus: LogTextPosition {
                display_row: 2,
                byte_offset: 1
            },
        })
    );

    select_log_position(&mut window, 2, 1);
    move_key(
        &mut window,
        &log_store,
        egui::Key::A,
        egui::Modifiers::COMMAND,
        2,
    );
    assert_eq!(
        window.selection.unwrap().normalized(),
        (
            LogTextPosition {
                display_row: 0,
                byte_offset: 0
            },
            LogTextPosition {
                display_row: 5,
                byte_offset: 4
            },
        )
    );
}

#[test]
fn keyboard_caret_ignores_typing() {
    let log_store = LogStoreService::default();
    let mut window = log_window(&["readonly"]);
    select_log_position(&mut window, 0, 2);

    assert!(!move_log_caret(
        &mut window,
        &log_store,
        1,
        1,
        egui::Key::A,
        egui::Modifiers::NONE,
        1.0,
    ));
    assert_eq!(
        caret_focus(&window),
        LogTextPosition {
            display_row: 0,
            byte_offset: 2
        }
    );
}

#[test]
fn keyboard_caret_waits_for_an_unloaded_target_page() {
    let log_store = LogStoreService::default();
    let context = egui::Context::default();
    let mut window = log_window(&["start"]);
    window.total_lines = LOG_PAGE_SIZE + 1;
    let total_lines = window.total_lines;
    select_log_position(&mut window, 0, 0);

    assert!(move_log_caret(
        &mut window,
        &log_store,
        total_lines,
        1,
        egui::Key::End,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        1.0,
    ));
    assert_eq!(
        window.pending_caret,
        Some(PendingLogCaret {
            display_row: LOG_PAGE_SIZE,
            character_column: usize::MAX,
            anchor: Some(LogTextPosition {
                display_row: 0,
                byte_offset: 0,
            }),
        })
    );

    window.insert_page(
        LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: LOG_PAGE_SIZE,
        },
        vec![LogPageRow {
            display_row: LOG_PAGE_SIZE,
            line_index: LOG_PAGE_SIZE,
            timestamp: None,
            text: "destination".to_owned(),
            style_spans: Vec::new(),
            match_ranges: Vec::new(),
        }],
    );
    resolve_pending_caret(&mut window, &log_store, total_lines, &context);

    assert_eq!(window.pending_caret, None);
    assert_eq!(
        window.selection,
        Some(LogTextSelection {
            anchor: LogTextPosition {
                display_row: 0,
                byte_offset: 0,
            },
            focus: LogTextPosition {
                display_row: LOG_PAGE_SIZE,
                byte_offset: "destination".len(),
            },
        })
    );
}
