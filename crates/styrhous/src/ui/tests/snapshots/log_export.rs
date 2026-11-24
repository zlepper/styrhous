//! Exporting a log window to a file (GitHub issue #90), from the button
//! through the save dialog and the on-disk spool to the written file.

use super::*;
use crate::log_store::LogRecord;
use crate::resource_table::ContainerKind;
use crate::ui::save_file_dialog::SaveFileChoice;
use crate::ui::save_file_dialog::test_support::MockSaveFileDialog;
use crate::worker::{PodLogInstance, PodLogStreamTarget};
use egui::accesskit::Role;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const LINES: [&str; 3] = [
    "2026-08-10T12:00:00Z \u{1b}[32mINFO\u{1b}[0m server ready",
    "2026-08-10T12:00:01Z WARN slow request",
    "2026-08-10T12:00:02Z INFO shutting down",
];

fn target(pod_name: &str) -> PodLogStreamTarget {
    PodLogStreamTarget {
        namespace: "kube-system".to_owned(),
        pod_name: pod_name.to_owned(),
        container: "server".to_owned(),
        kind: ContainerKind::App,
    }
}

/// A log window for `targets`, each source holding [`LINES`], with exports
/// answered by `dialog`.
fn log_window_harness(
    dialog: &MockSaveFileDialog,
    targets: Vec<PodLogStreamTarget>,
) -> (AppHarness, u64) {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().set_save_file_dialog(dialog.clone());
    let mut state = oracle_resource_table_state();
    state.request_pod_log_window(2, targets.clone(), &mut Vec::new());
    let window_id = *state.log_windows.keys().next().expect("log window opened");
    harness.state_mut().ui_state = state;
    harness.run_steps(2);
    let records = targets
        .iter()
        .flat_map(|target| {
            LINES
                .iter()
                .map(|line| LogRecord::from_source((*line).to_owned(), target.display_name()))
        })
        .collect::<Vec<_>>();
    let total = records.len();
    assert!(harness.state().log_store.append_records(window_id, records));
    run_until(&mut harness, "the records to reach the window", |app| {
        app.ui_state.log_windows[&window_id].total_lines == total
    });
    (harness, window_id)
}

/// Steps the app (exports write on a background thread) until `done`.
#[track_caller]
fn run_until(harness: &mut AppHarness, what: &str, done: impl Fn(&MyEguiApp<MockWorker>) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(harness.state()) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.event(egui::Event::PointerGone);
    harness.run_steps(2);
}

fn history(harness: &AppHarness) -> Vec<(String, String, Option<String>)> {
    harness.state().ui_state.clusters[&2]
        .operation_history
        .iter()
        .map(|entry| {
            (
                entry.title.clone(),
                entry.target.clone(),
                entry.details.clone(),
            )
        })
        .collect()
}

fn open_export_popover(harness: &mut AppHarness) {
    harness.get_by_label("Export logs").click();
    harness.run_steps(2);
    harness.event(egui::Event::PointerGone);
    harness.run_steps(2);
}

fn checkbox<'a>(harness: &'a AppHarness, label: &'a str) -> egui_kittest::Node<'a> {
    harness.get_by_role_and_label(Role::CheckBox, label)
}

fn is_checked(harness: &AppHarness, label: &str) -> bool {
    checkbox(harness, label).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

/// Exports with the popover's choices as they are and waits for the outcome.
fn save(harness: &mut AppHarness) {
    let recorded = history(harness).len();
    harness.get_by_label("Save…").click();
    run_until(harness, "the export outcome", |app| {
        app.ui_state.clusters[&2].operation_history.len() > recorded
    });
}

fn exported(path: &Path) -> String {
    std::fs::read_to_string(path).expect("export written")
}

/// `<prefix>-YYYYMMDD-HHMMSS.log`
#[track_caller]
fn assert_suggested_name(name: &str, prefix: &str) {
    let stamp = name
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.strip_suffix(".log"))
        .unwrap_or_else(|| panic!("{name} does not start with {prefix}"));
    assert!(
        stamp.len() == 15
            && stamp
                .char_indices()
                .all(|(index, character)| if index == 8 {
                    character == '-'
                } else {
                    character.is_ascii_digit()
                }),
        "{name}"
    );
}

#[test]
fn exporting_a_log_window_writes_the_chosen_lines_to_the_chosen_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("api.log");
    let dialog = MockSaveFileDialog::answering(SaveFileChoice::Chosen(path.clone()));
    let (mut harness, window_id) = log_window_harness(&dialog, vec![target("api-0")]);

    open_export_popover(&mut harness);
    assert!(
        checkbox(&harness, "Only lines matching the filter")
            .accesskit_node()
            .is_disabled()
            && !is_checked(&harness, "Only lines matching the filter"),
        "without an active filter every line is exported"
    );
    checkbox(&harness, "Include timestamps").click();
    harness.run_steps(2);
    save(&mut harness);

    let requests = dialog.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].window_id, window_id);
    assert_suggested_name(&requests[0].suggested_name, "api-0-server");
    assert_eq!(
        exported(&path),
        "2026-08-10T12:00:00Z INFO server ready\n\
         2026-08-10T12:00:01Z WARN slow request\n\
         2026-08-10T12:00:02Z INFO shutting down\n"
    );
    assert_eq!(
        history(&harness),
        vec![(
            "Logs exported".to_owned(),
            "api.log".to_owned(),
            Some(format!("3 lines saved to {}", path.display()))
        )]
    );
    harness.get_by_label("Exported 3 lines to api.log");

    // A second export starts from the window's defaults again.
    open_export_popover(&mut harness);
    assert!(!is_checked(&harness, "Include timestamps"));
    checkbox(&harness, "Keep ANSI colour codes").click();
    harness.run_steps(2);
    save(&mut harness);
    assert_eq!(
        exported(&path),
        "\u{1b}[32mINFO\u{1b}[0m server ready\nWARN slow request\nINFO shutting down\n",
        "the export replaced the earlier file, keeping the colour codes"
    );
    assert_eq!(history(&harness).len(), 2);
}

#[test]
fn exporting_only_the_filtered_lines_follows_the_window_filter() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("warnings.log");
    let dialog = MockSaveFileDialog::answering(SaveFileChoice::Chosen(path.clone()));
    let (mut harness, _) = log_window_harness(&dialog, vec![target("api-0")]);
    harness.get_by_label("Search logs").click();
    harness.run_steps(2);
    harness.event(egui::Event::Text("warn".to_owned()));
    harness.run_steps(2);
    harness.get_by_label("Filter to matching lines").click();
    harness.run_steps(2);

    open_export_popover(&mut harness);
    assert!(is_checked(&harness, "Only lines matching the filter"));
    save(&mut harness);

    assert_eq!(exported(&path), "WARN slow request\n");
    harness.get_by_label("Exported 1 line to warnings.log");
}

#[test]
fn exporting_an_interleaved_window_labels_each_line_with_its_source() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interleaved.log");
    let dialog = MockSaveFileDialog::answering(SaveFileChoice::Chosen(path.clone()));
    let (mut harness, _) = log_window_harness(&dialog, vec![target("api-0"), target("api-1")]);

    open_export_popover(&mut harness);
    assert!(
        is_checked(&harness, "Include source labels"),
        "interleaved windows show their sources"
    );
    save(&mut harness);

    assert_suggested_name(
        &dialog.requests()[0].suggested_name,
        "interleaved-2-sources",
    );
    let contents = exported(&path);
    for pod in ["api-0", "api-1"] {
        assert!(
            contents.contains(&format!("[kube-system/{pod} · server] WARN slow request")),
            "{contents}"
        );
    }
}

#[test]
fn previous_container_exports_are_named_as_such() {
    let directory = tempfile::tempdir().unwrap();
    let dialog = MockSaveFileDialog::answering(SaveFileChoice::Chosen(
        directory.path().join("previous.log"),
    ));
    let (mut harness, window_id) = log_window_harness(&dialog, vec![target("api-0")]);
    harness.get_by_label("Previous").click();
    run_until(&mut harness, "the switch to previous logs", |app| {
        !app.ui_state.log_windows.contains_key(&window_id)
    });
    let window = harness
        .state_mut()
        .ui_state
        .log_windows
        .values_mut()
        .next()
        .unwrap();
    assert_eq!(window.instance, PodLogInstance::Previous);

    open_export_popover(&mut harness);
    harness.get_by_label("Save…").click();
    harness.run_steps(2);
    assert_suggested_name(
        &dialog.requests()[0].suggested_name,
        "api-0-server-previous",
    );
}

#[test]
fn cancelling_the_save_dialog_writes_nothing() {
    let dialog = MockSaveFileDialog::answering(SaveFileChoice::Cancelled);
    let (mut harness, window_id) = log_window_harness(&dialog, vec![target("api-0")]);
    open_export_popover(&mut harness);
    harness.get_by_label("Save…").click();
    harness.run_steps(4);

    assert_eq!(dialog.requests().len(), 1, "the dialog was asked");
    assert!(
        harness.state().ui_state.log_windows[&window_id]
            .export
            .status
            .is_none()
    );
    assert!(history(&harness).is_empty());
    assert!(
        !harness
            .get_by_label("Export logs")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn an_unavailable_save_dialog_is_reported() {
    let dialog = MockSaveFileDialog::answering(SaveFileChoice::Unavailable(
        crate::ui::save_file_dialog::SAVE_DIALOG_UNAVAILABLE.to_owned(),
    ));
    let (mut harness, _) = log_window_harness(&dialog, vec![target("api-0")]);
    open_export_popover(&mut harness);
    save(&mut harness);

    assert_eq!(
        history(&harness),
        vec![(
            "Couldn’t export logs".to_owned(),
            "Log export".to_owned(),
            Some(crate::ui::save_file_dialog::SAVE_DIALOG_UNAVAILABLE.to_owned())
        )]
    );
    assert!(
        harness
            .query_by_label_contains("Export failed: No save dialog could be opened")
            .is_some()
    );
}

#[test]
fn a_failed_export_is_shown_in_the_window_and_the_history() {
    let directory = tempfile::tempdir().unwrap();
    let path: PathBuf = directory.path().join("missing").join("api.log");
    let dialog = MockSaveFileDialog::answering(SaveFileChoice::Chosen(path.clone()));
    let (mut harness, _) = log_window_harness(&dialog, vec![target("api-0")]);
    open_export_popover(&mut harness);
    save(&mut harness);

    let history = history(&harness);
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].0, "Couldn’t export logs");
    assert!(
        history[0]
            .2
            .as_deref()
            .is_some_and(|details| details.starts_with("Could not write to")),
        "{history:?}"
    );
    assert!(!path.exists());
    assert!(
        harness
            .query_by_label_contains("Export failed: Could not write to")
            .is_some()
    );
}

#[test]
fn an_export_is_still_reported_after_its_window_closes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("closed.log");
    let dialog = MockSaveFileDialog::answering(SaveFileChoice::Cancelled);
    let (mut harness, window_id) = log_window_harness(&dialog, vec![target("api-0")]);
    // The export snapshot keeps writing after the window closes; only the
    // outcome needs a window to report to.
    let window = harness
        .state_mut()
        .ui_state
        .log_windows
        .get_mut(&window_id)
        .unwrap();
    window.export.status = Some(crate::ui::log_windows::LogExportStatus::Writing {
        export_id: 7,
        path: path.clone(),
    });
    window.close_requested = true;
    harness.run_steps(2);
    assert!(harness.state().ui_state.log_windows.is_empty());

    for export_id in [6, 7, 7] {
        // An unrelated and a duplicate result are ignored.
        harness.state_mut().ui_state.apply_log_store_result(
            crate::log_store::LogStoreResult::Exported {
                window_id,
                export_id,
                path: path.clone(),
                lines: 3,
            },
        );
    }
    assert_eq!(
        history(&harness),
        vec![(
            "Logs exported".to_owned(),
            "closed.log".to_owned(),
            Some(format!("3 lines saved to {}", path.display()))
        )]
    );
}

#[test]
fn switching_container_start_waits_for_a_running_export() {
    let directory = tempfile::tempdir().unwrap();
    let dialog =
        MockSaveFileDialog::answering(SaveFileChoice::Chosen(directory.path().join("busy.log")));
    let (mut harness, window_id) = log_window_harness(&dialog, vec![target("api-0")]);
    let window = harness
        .state_mut()
        .ui_state
        .log_windows
        .get_mut(&window_id)
        .unwrap();
    window.export.status = Some(crate::ui::log_windows::LogExportStatus::Writing {
        export_id: 1,
        path: directory.path().join("busy.log"),
    });
    harness.run_steps(2);

    assert!(
        harness
            .get_by_label("Previous")
            .accesskit_node()
            .is_disabled(),
        "switching would close the store the export reads from"
    );
    assert!(
        harness
            .get_by_label("Export logs")
            .accesskit_node()
            .is_disabled()
    );
}
