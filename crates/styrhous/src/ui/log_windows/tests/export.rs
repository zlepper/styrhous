use super::*;
use crate::log_store::LogStoreResult;
use crate::ui::log_windows::export::LogExportStatus;
use crate::ui::save_file_dialog::{SaveFileAnswer, SaveFileChoice};
use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::NodeT;
use std::path::PathBuf;

const LINES: [&str; 3] = [
    "2026-08-08T15:22:17.143Z  INFO  server: listening on 0.0.0.0:8080",
    "2026-08-08T15:22:18.201Z  WARN  cache: warming took 1.2s",
    "2026-08-08T15:22:19.007Z  INFO  server: ready",
];

type WindowHarness = Harness<'static, (PodLogWindowState, LogDisplayOptions)>;

fn window_harness(window: PodLogWindowState, display_options: LogDisplayOptions) -> WindowHarness {
    let log_store = LogStoreService::default();
    let mut harness = Harness::builder().build_ui_state(
        move |ui, (window, display_options): &mut (PodLogWindowState, LogDisplayOptions)| {
            show_log_window(ui, window, display_options, &log_store, &mut false);
        },
        (window, display_options),
    );
    components::test_support::setup_egui(&mut harness);
    harness.run();
    harness
}

fn checkbox_state(harness: &WindowHarness, label: &str) -> (bool, bool) {
    let checkbox = harness.get_by_role_and_label(Role::CheckBox, label);
    let node = checkbox.accesskit_node();
    (node.toggled() == Some(Toggled::True), node.is_disabled())
}

fn open_popover(harness: &mut WindowHarness) {
    harness.get_by_label("Export logs").click();
    harness.run();
    harness.event(egui::Event::PointerGone);
    harness.run();
}

#[test]
fn export_options_start_from_what_the_window_shows() {
    let mut window = log_window(&LINES);
    window.search.query = "server".to_owned();
    window.search.filter_matches = true;
    let mut harness = window_harness(
        window,
        LogDisplayOptions {
            show_timestamps: true,
            ..LogDisplayOptions::default()
        },
    );

    open_popover(&mut harness);
    assert_eq!(
        checkbox_state(&harness, "Only lines matching the filter"),
        (true, false)
    );
    assert_eq!(
        checkbox_state(&harness, "Include source labels"),
        (false, false)
    );
    assert_eq!(
        checkbox_state(&harness, "Include timestamps"),
        (true, false)
    );
    assert_eq!(
        checkbox_state(&harness, "Keep ANSI colour codes"),
        (false, false)
    );
    harness.ui_harness("pod_logs/export_options_start_from_what_the_window_shows/export_options");

    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.query_by_label("Export options").is_none());

    // A filter that is off, or whose pattern is invalid, cannot be followed.
    for disable in [
        (|window: &mut PodLogWindowState| window.search.filter_matches = false)
            as fn(&mut PodLogWindowState),
        |window| {
            window.search.regex_mode = true;
            window.search.query = "(unclosed".to_owned();
            window.search.error = Some("unclosed group".to_owned());
        },
    ] {
        disable(&mut harness.state_mut().0);
        harness.run();
        open_popover(&mut harness);
        assert_eq!(
            checkbox_state(&harness, "Only lines matching the filter"),
            (false, true)
        );
        harness.key_press(egui::Key::Escape);
        harness.run();
    }
}

#[test]
fn export_outcomes_are_shown_in_the_window_header() {
    let mut window = log_window(&LINES);
    window.export.status = Some(LogExportStatus::Done {
        path: PathBuf::from("/home/user/logs/api-0-api-20261122-101500.log"),
        lines: 1_204,
    });
    let mut harness = window_harness(window, LogDisplayOptions::default());
    harness.get_by_label("Exported 1204 lines to api-0-api-20261122-101500.log");
    harness.ui_harness("pod_logs/export_outcomes_are_shown_in_the_window_header/exported");

    harness.state_mut().0.export.status = Some(LogExportStatus::Failed(
        "Could not write to /mnt/readonly: Read-only file system (os error 30)".to_owned(),
    ));
    harness.run();
    harness.ui_harness("pod_logs/export_outcomes_are_shown_in_the_window_header/failed");

    harness.state_mut().0.export.status = Some(LogExportStatus::Writing {
        export_id: 1,
        path: PathBuf::from("/tmp/api.log"),
    });
    harness.run();
    harness.get_by_label("Exporting to api.log…");
    assert!(
        harness
            .get_by_label("Export logs")
            .accesskit_node()
            .is_disabled(),
        "one export is written at a time per window"
    );
}

#[test]
fn a_long_export_error_is_truncated_before_the_window_status() {
    let mut window = log_window(&LINES);
    let error = format!(
        "Could not save /home/user/{}/api.log: No space left on device (os error 28)",
        "nested/".repeat(20)
    );
    window.export.status = Some(LogExportStatus::Failed(error.clone()));
    let mut harness = window_harness(window, LogDisplayOptions::default());
    // The title and the state stay readable, left of the error.
    let title = harness.get_by_label("api-0").rect();
    assert!(title.width() > 30.0, "the title is not squeezed: {title:?}");
    let status = harness.get_by_label("Following").rect();
    let error_label = harness
        .get_by_label(&format!("Export failed: {error}"))
        .rect();
    assert!(status.right() < error_label.left());
    harness
        .ui_harness("pod_logs/a_long_export_error_is_truncated_before_the_window_status/truncated");
}

#[test]
fn only_the_awaited_answer_and_result_settle_an_export() {
    let log_store = LogStoreService::default();
    let mut window = log_window(&LINES);
    window.export.status = Some(LogExportStatus::ChoosingFile {
        export_id: 2,
        options: crate::log_store::LogExportOptions {
            filter: None,
            format: Default::default(),
        },
    });
    let stale_answer = SaveFileAnswer {
        window_id: window.id,
        export_id: 1,
        choice: SaveFileChoice::Chosen(PathBuf::from("/tmp/stale.log")),
    };
    assert!(
        window
            .answer_save_request(stale_answer, &log_store)
            .is_none()
    );
    assert!(matches!(
        window.export.status,
        Some(LogExportStatus::ChoosingFile { export_id: 2, .. })
    ));

    window.export.status = Some(LogExportStatus::Writing {
        export_id: 2,
        path: PathBuf::from("/tmp/current.log"),
    });
    let stale_result = LogStoreResult::Exported {
        window_id: window.id,
        export_id: 1,
        path: PathBuf::from("/tmp/stale.log"),
        lines: 3,
    };
    assert!(window.settle_export(&stale_result).is_none());
    assert!(window.export.writing_export_id() == Some(2));
}
