use super::*;
use crate::log_store::{LOG_PAGE_SIZE, LogPageRow, LogStoreConfig, LogStoreResult};
use crate::resource_table::ContainerKind;
use crate::ui::log_state::apply_store_result;
use crate::worker::{MockWorker, PodLogStreamTarget};
use components::test_support::UiHarnessSnapshot;
use egui_kittest::{Harness, kittest::Queryable};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

fn log_window(lines: &[&str]) -> PodLogWindowState {
    let mut window = PodLogWindowState::new(
        1,
        1,
        vec![PodLogStreamTarget {
            namespace: "default".to_owned(),
            pod_name: "api-0".to_owned(),
            container: "api".to_owned(),
            kind: ContainerKind::App,
        }],
    )
    .expect("test log window has one source");
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
                    source: None,
                    text: parsed.line.text,
                    style_spans: parsed.line.style_spans,
                    match_ranges: Vec::new(),
                }
            })
            .collect(),
    );
    window
}

#[test]
fn switching_log_instance_keeps_the_viewport_and_rejects_old_results() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    let target = PodLogStreamTarget {
        namespace: "default".into(),
        pod_name: "api-0".into(),
        container: "api".into(),
        kind: ContainerKind::App,
    };
    state.request_pod_log_window(7, vec![target.clone()], &mut commands);
    commands.clear();
    let old = state.log_windows.get_mut(&1).unwrap();
    old.total_lines = 4;
    old.search.query = "panic".into();
    old.search.filter_matches = true;
    old.selection = Some(LogTextSelection {
        anchor: LogTextPosition {
            display_row: 0,
            byte_offset: 0,
        },
        focus: LogTextPosition {
            display_row: 0,
            byte_offset: 1,
        },
    });

    let store = LogStoreService::default();
    state.switch_log_window_instance(1, PodLogInstance::Previous, &store, &mut commands);
    assert!(!state.log_windows.contains_key(&1));
    let previous = &state.log_windows[&2];
    assert_eq!(previous.viewport_id, 1);
    assert_eq!(previous.instance, PodLogInstance::Previous);
    assert_eq!(previous.total_lines, 0);
    assert_eq!(previous.search.query, "panic");
    assert!(previous.search.filter_matches);
    assert!(previous.selection.is_none());
    assert!(
        commands[0]
            .as_ref()
            .as_any()
            .downcast_ref::<StopPodLogStream>()
            .is_some()
    );
    let start = commands[1]
        .as_ref()
        .as_any()
        .downcast_ref::<StartPodLogStream>()
        .unwrap();
    assert_eq!(start.log_window_id, 2);
    assert_eq!(start.instance, PodLogInstance::Previous);
    assert_eq!(start.targets, vec![target]);

    PodLogStreamStarted { log_window_id: 1 }.apply(&mut state, &mut commands);
    PodLogStreamEnded { log_window_id: 1 }.apply(&mut state, &mut commands);
    apply_store_result(
        &mut state.log_windows,
        LogStoreResult::Updated {
            window_id: 1,
            total_lines: 99,
            completed_search: None,
            appended_rows: Vec::new(),
            backfill_lines: None,
        },
    );
    assert_eq!(state.log_windows[&2].total_lines, 0);
    PodLogStreamStarted { log_window_id: 2 }.apply(&mut state, &mut commands);
    assert_eq!(state.log_windows[&2].status, PodLogStatus::Loading);

    commands.clear();
    state.switch_log_window_instance(2, PodLogInstance::Current, &store, &mut commands);
    assert_eq!(state.log_windows[&3].viewport_id, 1);
    assert_eq!(state.log_windows[&3].instance, PodLogInstance::Current);
    assert_eq!(
        commands[1]
            .as_ref()
            .as_any()
            .downcast_ref::<StartPodLogStream>()
            .unwrap()
            .instance,
        PodLogInstance::Current
    );
}

#[test]
fn log_instance_selector_requests_previous() {
    let window = Rc::new(RefCell::new(log_window(&["one line"])));
    let rendered_window = window.clone();
    let log_store = LogStoreService::default();
    let mut harness = Harness::builder().build_ui(move |ui| {
        show_log_window(
            ui,
            &mut rendered_window.borrow_mut(),
            &mut LogDisplayOptions::default(),
            &log_store,
            &mut false,
        );
    });
    components::test_support::setup_egui(&mut harness);
    harness.run();
    let previous = harness.get_by_label("Previous");
    previous.click();
    harness.run_steps(2);
    assert_eq!(
        window.borrow().requested_instance,
        Some(PodLogInstance::Previous)
    );
}

#[test]
fn requested_log_instance_is_switched_by_the_window_render_flow() {
    let state = Rc::new(RefCell::new(UiState::default()));
    let commands = Rc::new(RefCell::new(Vec::new()));
    let log_store = Rc::new(LogStoreService::default());
    state.borrow_mut().request_pod_log_window(
        7,
        vec![PodLogStreamTarget {
            namespace: "default".into(),
            pod_name: "api-0".into(),
            container: "api".into(),
            kind: ContainerKind::App,
        }],
        &mut commands.borrow_mut(),
    );
    commands.borrow_mut().clear();
    let rendered_state = state.clone();
    let rendered_commands = commands.clone();
    let rendered_store = log_store.clone();
    let mut harness = Harness::builder().build_ui(move |ui| {
        show(
            ui.ctx(),
            &mut rendered_state.borrow_mut(),
            &rendered_store,
            &mut rendered_commands.borrow_mut(),
        );
    });
    components::test_support::setup_egui(&mut harness);
    harness.run();
    assert!(state.borrow().log_windows[&1].store_opened);

    harness.get_by_label("Previous").click();
    harness.run_steps(3);

    let state = state.borrow();
    assert!(!state.log_windows.contains_key(&1));
    assert_eq!(state.log_windows[&2].viewport_id, 1);
    assert!(state.log_windows[&2].store_opened);
    let commands = commands.borrow();
    assert!(commands[0].as_ref().as_any().is::<StopPodLogStream>());
    assert_eq!(
        commands[1]
            .as_ref()
            .as_any()
            .downcast_ref::<StartPodLogStream>()
            .unwrap()
            .instance,
        PodLogInstance::Previous
    );
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
                    source: None,
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
