use super::*;
use std::path::Path;

const SOURCE: &str = "payments/api-0 · server";

fn records(lines: &[&str]) -> Vec<LogRecord> {
    lines
        .iter()
        .map(|line| LogRecord::from_source((*line).to_owned(), SOURCE.to_owned()))
        .collect()
}

fn spool(service: &LogStoreService, window_id: u64, lines: &[&str]) {
    service.open(window_id);
    assert!(service.append_records(window_id, records(lines)));
    let total = lines.len();
    let _ = wait_for(service, |result| {
        matches!(
            result,
            LogStoreResult::Updated { window_id: id, total_lines, .. }
                if *id == window_id && *total_lines == total
        )
    });
}

fn plain() -> LogExportOptions {
    LogExportOptions {
        filter: None,
        format: LogExportFormat::default(),
    }
}

fn with_format(format: LogExportFormat) -> LogExportOptions {
    LogExportOptions {
        filter: None,
        format,
    }
}

fn filtered(query: &str, regex_mode: bool) -> LogExportOptions {
    LogExportOptions {
        filter: Some(LogExportFilter {
            query: query.to_owned(),
            regex_mode,
        }),
        format: LogExportFormat::default(),
    }
}

/// Waits for export `export_id` and returns the file's contents and line count.
fn finish_export(
    service: &LogStoreService,
    window_id: u64,
    export_id: u64,
    path: &Path,
) -> (String, usize) {
    match wait_for(service, |result| {
        matches!(
            result,
            LogStoreResult::Exported { export_id: id, .. }
                | LogStoreResult::ExportFailed { export_id: id, .. }
                if *id == export_id
        )
    }) {
        LogStoreResult::Exported {
            window_id: id,
            path: exported,
            lines,
            ..
        } => {
            assert_eq!(id, window_id);
            assert_eq!(exported, path);
            (std::fs::read_to_string(path).expect("export exists"), lines)
        }
        other => panic!("export failed: {other:?}"),
    }
}

fn export(
    service: &LogStoreService,
    window_id: u64,
    export_id: u64,
    path: &Path,
    options: LogExportOptions,
) -> (String, usize) {
    assert!(service.export(window_id, export_id, path.to_owned(), options));
    finish_export(service, window_id, export_id, path)
}

const LINES: [&str; 4] = [
    "2026-08-10T12:00:00.000000001Z \u{1b}[32mINFO\u{1b}[0m server ready",
    "2026-08-10T12:00:01Z WARN slow request",
    "not a timestamp: raw line",
    "2026-08-10T12:00:02Z info shutting down",
];

#[test]
fn exports_every_line_with_the_chosen_decorations() {
    let directory = tempfile::tempdir().unwrap();
    let service = LogStoreService::default();
    spool(&service, 90, &LINES);

    let path = directory.path().join("plain.log");
    assert_eq!(
        export(&service, 90, 1, &path, plain()),
        (
            "INFO server ready\nWARN slow request\nnot a timestamp: raw line\ninfo shutting down\n"
                .to_owned(),
            4
        )
    );

    let path = directory.path().join("decorated.log");
    let (contents, _) = export(
        &service,
        90,
        2,
        &path,
        with_format(LogExportFormat {
            include_timestamps: true,
            include_sources: true,
            keep_ansi: true,
        }),
    );
    assert_eq!(
        contents.lines().collect::<Vec<_>>(),
        vec![
            "2026-08-10T12:00:00.000000001Z [payments/api-0 · server] \u{1b}[32mINFO\u{1b}[0m server ready",
            "2026-08-10T12:00:01Z [payments/api-0 · server] WARN slow request",
            "[payments/api-0 · server] not a timestamp: raw line",
            "2026-08-10T12:00:02Z [payments/api-0 · server] info shutting down",
        ]
    );
}

#[test]
fn exports_only_lines_matching_the_window_filter() {
    let directory = tempfile::tempdir().unwrap();
    let service = LogStoreService::default();
    spool(&service, 91, &LINES);

    // Case-insensitive, and matched against the message without ANSI styling
    // or timestamp, like the log window's own filter.
    let path = directory.path().join("info.log");
    assert_eq!(
        export(&service, 91, 1, &path, filtered("INFO SERVER", false)),
        ("INFO server ready\n".to_owned(), 1)
    );
    let path = directory.path().join("regex.log");
    assert_eq!(
        export(&service, 91, 2, &path, filtered("^(warn|info) s[lh]", true)),
        ("WARN slow request\ninfo shutting down\n".to_owned(), 2)
    );
    let path = directory.path().join("none.log");
    assert_eq!(
        export(&service, 91, 3, &path, filtered("12:00", false)),
        (String::new(), 0),
        "timestamps are not part of the searchable message"
    );
    let path = directory.path().join("styled.log");
    assert_eq!(
        export(
            &service,
            91,
            4,
            &path,
            LogExportOptions {
                format: LogExportFormat {
                    keep_ansi: true,
                    ..LogExportFormat::default()
                },
                ..filtered("info server", false)
            }
        ),
        ("\u{1b}[32mINFO\u{1b}[0m server ready\n".to_owned(), 1),
        "the filter ignores styling even when the styling is kept"
    );
}

#[test]
fn an_export_is_a_snapshot_of_the_records_when_it_was_requested() {
    let directory = tempfile::tempdir().unwrap();
    let service = LogStoreService::default();
    spool(&service, 92, &LINES);

    // Commands are handled in order, so the export snapshots before the
    // appended records arrive and keeps writing after the window closes.
    let path = directory.path().join("snapshot.log");
    assert!(service.export(92, 1, path.clone(), plain()));
    assert!(service.append_records(92, records(&["2026-08-10T12:00:03Z appended later"])));
    assert!(service.close(92));

    let (contents, lines) = finish_export(&service, 92, 1, &path);
    assert_eq!(lines, 4);
    assert!(!contents.contains("appended later"), "{contents}");
}

#[tokio::test]
async fn exports_rebased_history_in_logical_order() {
    let directory = tempfile::tempdir().unwrap();
    let service = LogStoreService::default();
    let appender = service.appender();
    service.open(93);
    appender
        .append(93, vec!["2026-08-10T10:00:02Z live two".into()])
        .await
        .unwrap();
    appender
        .append_backfill(
            93,
            vec![
                "2026-08-10T10:00:00Z history zero".into(),
                "2026-08-10T10:00:01Z history one".into(),
                "2026-08-10T10:00:02Z live two".into(),
            ],
        )
        .await
        .unwrap();
    appender.complete_backfill(93).await.unwrap();
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Rebased { window_id: 93, .. })
    });

    let path = directory.path().join("history.log");
    assert_eq!(
        export(&service, 93, 1, &path, plain()),
        ("history zero\nhistory one\nlive two\n".to_owned(), 3)
    );
}

#[test]
fn an_export_replaces_an_existing_file_with_a_normal_file_mode() {
    let directory = tempfile::tempdir().unwrap();
    let service = LogStoreService::default();
    spool(&service, 94, &LINES[..1]);
    let path = directory.path().join("existing.log");
    std::fs::write(&path, "a much longer previous export\n".repeat(10)).unwrap();

    assert_eq!(
        export(&service, 94, 1, &path, plain()),
        ("INFO server ready\n".to_owned(), 1)
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_ne!(
            mode & 0o044,
            0,
            "readable like any file the user saves (umask permitting), not private"
        );
    }
}

#[test]
fn a_failed_export_reports_the_error_and_leaves_no_file() {
    let directory = tempfile::tempdir().unwrap();
    let service = LogStoreService::default();
    spool(&service, 95, &LINES);
    let path = directory.path().join("missing").join("logs.log");

    assert!(service.export(95, 7, path.clone(), plain()));
    let LogStoreResult::ExportFailed {
        window_id, error, ..
    } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::ExportFailed { export_id: 7, .. })
    })
    else {
        unreachable!()
    };
    assert_eq!(window_id, 95);
    assert!(error.starts_with("Could not write to"), "{error}");
    assert!(!path.exists());

    assert!(service.export(96, 8, directory.path().join("closed.log"), plain()));
    assert!(matches!(
        wait_for(&service, |result| matches!(
            result,
            LogStoreResult::ExportFailed { export_id: 8, .. }
        )),
        LogStoreResult::ExportFailed { window_id: 96, .. }
    ));
}

#[test]
fn an_invalid_filter_fails_the_export() {
    let directory = tempfile::tempdir().unwrap();
    let service = LogStoreService::default();
    spool(&service, 97, &LINES);
    assert!(service.export(
        97,
        1,
        directory.path().join("invalid.log"),
        filtered("(unclosed", true),
    ));
    assert!(matches!(
        wait_for(&service, |result| matches!(
            result,
            LogStoreResult::ExportFailed { window_id: 97, .. }
        )),
        LogStoreResult::ExportFailed { .. }
    ));
}
