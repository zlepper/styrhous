use super::*;
use crate::log_store::{LOG_PAGE_SIZE, LogPageRow, LogStoreConfig, LogStoreResult};
use crate::minimal_resource::PodLogContainer;
use crate::resource_table::ContainerKind;
use crate::worker::MockWorker;
use components::test_support::UiHarnessSnapshot;
use egui_kittest::{Harness, kittest::Queryable};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

fn log_window(lines: &[&str]) -> PodLogWindowState {
    let mut window = PodLogWindowState::new(
        1,
        1,
        "default".to_owned(),
        "api-0".to_owned(),
        PodLogContainer {
            name: "api".to_owned(),
            kind: ContainerKind::App,
            image: None,
        },
    );
    window.total_lines = lines.len();
    window.initial_page_loaded = true;
    window.store_opened = true;
    window.status = PodLogStatus::Following;
    window.insert_page(
        LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: 0,
        },
        lines
            .iter()
            .enumerate()
            .map(|(line_index, text)| {
                let parsed = crate::ansi::parse_kubernetes_log_line(text);
                LogPageRow {
                    display_row: line_index,
                    line_index,
                    timestamp: parsed.timestamp,
                    text: parsed.line.text,
                    style_spans: parsed.line.style_spans,
                    match_ranges: Vec::new(),
                }
            })
            .collect(),
    );
    window
}

fn fully_loaded_log_window(line_count: usize) -> PodLogWindowState {
    let mut window = log_window(&[]);
    window.total_lines = line_count;
    window.status = PodLogStatus::Finished;

    for page_start in (0..line_count).step_by(LOG_PAGE_SIZE) {
        let page_end = (page_start + LOG_PAGE_SIZE).min(line_count);
        window.insert_page(
            LogPageKey {
                generation: 0,
                filter_matches: false,
                page_start,
            },
            (page_start..page_end)
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
    }

    window
}

fn select_log_position(window: &mut PodLogWindowState, display_row: usize, byte_offset: usize) {
    let position = LogTextPosition {
        display_row,
        byte_offset,
    };
    window.selection = Some(LogTextSelection {
        anchor: position,
        focus: position,
    });
    window.caret_preferred_column = None;
}

fn move_key(
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    key: egui::Key,
    modifiers: egui::Modifiers,
    page_rows: usize,
) {
    let display_count = displayed_line_count(window);
    assert!(move_log_caret(
        window,
        log_store,
        display_count,
        page_rows,
        key,
        modifiers,
        1.0,
    ));
}

fn caret_focus(window: &PodLogWindowState) -> LogTextPosition {
    window.selection.expect("test positions a log caret").focus
}

fn show_wide_test_scroll_area(
    ui: &mut egui::Ui,
    wide_line: &str,
) -> egui::scroll_area::ScrollAreaOutput<()> {
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    components::scroll::both()
        .id_salt("wide-log-scroll-test")
        .auto_shrink([false, false])
        .show_rows(ui, row_height, 1, |ui, _| {
            ui.add(
                egui::Label::new(log_line_layout_job(
                    0,
                    None,
                    wide_line,
                    &[],
                    &[],
                    LogDisplayOptions::default(),
                ))
                .extend(),
            );
        })
}

mod caret_snapshots;
mod display;
mod filtering;
mod keyboard_basic;
mod keyboard_document;
mod layout;
mod navigation;
mod pointer;
mod rebase;

fn add_match_ranges(window: &mut PodLogWindowState, filter_matches: bool) {
    let key = LogPageKey {
        generation: 0,
        filter_matches,
        page_start: 0,
    };
    let matcher = regex::Regex::new("(?i)http").expect("valid test matcher");
    let page = window.pages.get_mut(&key).expect("test page exists");
    for row in &mut page.rows {
        row.match_ranges = matcher
            .find_iter(&row.text)
            .map(|range| (range.start(), range.end()))
            .collect();
    }
}

fn snapshot_window(window: PodLogWindowState, name: &str) {
    snapshot_window_with_display_options(window, name, LogDisplayOptions::default());
}

fn snapshot_window_with_display_options(
    window: PodLogWindowState,
    name: &str,
    display_options: LogDisplayOptions,
) {
    snapshot_window_after_horizontal_scroll_with_display_options(
        window,
        name,
        0.0,
        display_options,
        true,
    );
}

fn snapshot_initial_spool_window(
    window: PodLogWindowState,
    name: &str,
    display_options: LogDisplayOptions,
) {
    snapshot_window_after_horizontal_scroll_with_display_options(
        window,
        name,
        0.0,
        display_options,
        false,
    );
}

fn snapshot_window_after_horizontal_scroll(
    window: PodLogWindowState,
    name: &str,
    horizontal_offset: f32,
) {
    snapshot_window_after_horizontal_scroll_with_display_options(
        window,
        name,
        horizontal_offset,
        LogDisplayOptions::default(),
        true,
    );
}

fn snapshot_window_after_horizontal_scroll_with_display_options(
    window: PodLogWindowState,
    name: &str,
    horizontal_offset: f32,
    mut display_options: LogDisplayOptions,
    settle: bool,
) {
    let mut window = window;
    let log_store = LogStoreService::default();
    let mut close_requested = false;
    let mut harness = Harness::builder().build_ui(move |ctx| {
        show_log_window(
            ctx,
            &mut window,
            &mut display_options,
            &log_store,
            &mut close_requested,
        )
    });
    components::test_support::setup_egui(&mut harness);
    if settle {
        harness.run();
    } else {
        // The spinner repaints continuously. A fixed frame is sufficient
        // for this visual regression without asking the harness to settle.
        harness.step();
    }
    if horizontal_offset > 0.0 {
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(egui::pos2(400.0, 100.0)));
        harness.step();
        harness.input_mut().events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(-horizontal_offset, 0.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::default(),
        });
        harness.step();
        // The wheel event updates ScrollArea state during this frame. Draw
        // a couple more frames so the virtual text fragment observes the
        // resulting offset before snapshotting.
        harness.run_steps(2);
    }
    harness.ui_harness(name);
}

fn wait_for_store_result(
    service: &LogStoreService,
    matches: impl Fn(&LogStoreResult) -> bool,
) -> LogStoreResult {
    let start = Instant::now();
    loop {
        if let Some(result) = service.try_next_result()
            && matches(&result)
        {
            return result;
        }
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "timed out waiting for log-store result"
        );
        std::thread::yield_now();
    }
}
