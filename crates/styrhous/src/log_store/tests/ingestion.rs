use super::*;

#[test]
fn store_results_request_a_repaint_when_context_is_attached() {
    let context = egui::Context::default();
    let repaint_count = Arc::new(AtomicUsize::new(0));
    let repaint_count_for_callback = repaint_count.clone();
    context.set_request_repaint_callback(move |_| {
        repaint_count_for_callback.fetch_add(1, Ordering::Relaxed);
    });
    let service = LogStoreService::with_repaint_context(context);

    assert!(service.append(1, vec!["line 0".to_owned()]));
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { .. })
    });

    let start = std::time::Instant::now();
    while repaint_count.load(Ordering::Relaxed) == 0 {
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "timed out waiting for log-store repaint request"
        );
        thread::yield_now();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_updates_include_parsed_rows_and_backfill_progress() {
    let service = LogStoreService::default();
    service.open(77);
    assert!(service.append(
        77,
        vec!["2026-08-10T12:34:56Z \u{1b}[31merror\u{1b}[0m".to_owned()]
    ));

    let LogStoreResult::Updated {
        total_lines,
        appended_rows,
        backfill_lines,
        ..
    } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { window_id: 77, .. })
    })
    else {
        unreachable!()
    };
    assert_eq!(total_lines, 1);
    assert_eq!(backfill_lines, None);
    assert_eq!(appended_rows.len(), 1);
    assert_eq!(appended_rows[0].display_row, 0);
    assert_eq!(appended_rows[0].line_index, 0);
    assert_eq!(
        appended_rows[0].timestamp.as_deref(),
        Some("2026-08-10T12:34:56Z")
    );
    assert_eq!(appended_rows[0].text, "error");
    assert_eq!(appended_rows[0].style_spans.len(), 1);

    let appender = service.appender();
    appender
        .append_backfill(77, vec!["old one".into(), "old two".into()])
        .await
        .expect("history append is accepted");
    let LogStoreResult::Updated {
        total_lines,
        appended_rows,
        backfill_lines,
        ..
    } = wait_for(&service, |result| {
        matches!(
            result,
            LogStoreResult::Updated {
                window_id: 77,
                backfill_lines: Some(2),
                ..
            }
        )
    })
    else {
        unreachable!()
    };
    assert_eq!(total_lines, 1);
    assert!(appended_rows.is_empty());
    assert_eq!(backfill_lines, Some(2));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direct_ingestion_waits_for_spool_capacity_without_ui_draining_updates() {
    let service = LogStoreService::new(LogStoreConfig {
        command_channel_capacity: 1,
        result_channel_capacity: 1,
        ..LogStoreConfig::default()
    });
    let appender = service.appender();

    // This intentionally exceeds both bounded command queues many times
    // while leaving the UI result side untouched. Every batch must be
    // accepted eventually; a try_send data path would fail here.
    for line_index in 0..512 {
        appender
            .append(41, vec![format!("line {line_index}")])
            .await
            .expect("direct ingestion waits instead of dropping a batch");
    }

    let updated = wait_for(&service, |result| {
        matches!(
            result,
            LogStoreResult::Updated {
                window_id: 41,
                total_lines: 512,
                ..
            }
        )
    });
    assert!(matches!(
        updated,
        LogStoreResult::Updated {
            window_id: 41,
            total_lines: 512,
            ..
        }
    ));

    assert!(service.load_page(41, 0, false, 256));
    let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
        matches!(
            result,
            LogStoreResult::PageLoaded {
                page_start: 256,
                ..
            }
        )
    }) else {
        unreachable!()
    };
    assert_eq!(rows.len(), LOG_PAGE_SIZE);
    assert_eq!(rows[0].text, "line 256");
}

#[test]
fn page_reads_overtake_pending_append_batches() {
    let (control_sender, control_receiver) = mpsc::sync_channel(1);
    let (_scan_sender, scan_receiver) = mpsc::sync_channel(1);
    let (append_sender, append_receiver) = mpsc::sync_channel(1);
    let (_backfill_sender, backfill_receiver) = mpsc::sync_channel(1);
    append_sender
        .send(Command::Append {
            window_id: 1,
            lines: vec!["queued append".to_owned()],
        })
        .expect("append queue accepts the batch");
    control_sender
        .send(Command::LoadPage {
            window_id: 1,
            generation: 0,
            filter_matches: false,
            page_start: 0,
        })
        .expect("control queue accepts the page request");

    assert!(matches!(
        next_store_command(
            &control_receiver,
            &append_receiver,
            &backfill_receiver,
            &scan_receiver,
        ),
        Some(Command::LoadPage { window_id: 1, .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completed_backfill_rebases_overlapping_tail_without_changing_records() {
    let service = LogStoreService::default();
    let appender = service.appender();
    service.open(12);
    appender
        .append(
            12,
            vec![
                "2026-08-10T10:00:02Z tail two".into(),
                "2026-08-10T10:00:03Z tail three".into(),
                "2026-08-10T10:00:04Z live four".into(),
            ],
        )
        .await
        .expect("initial tail is spooled");
    let _ = wait_for(&service, |result| {
        matches!(
            result,
            LogStoreResult::Updated {
                window_id: 12,
                total_lines: 3,
                ..
            }
        )
    });

    appender
        .append_backfill(
            12,
            vec![
                "2026-08-10T10:00:00Z history zero".into(),
                "2026-08-10T10:00:01Z history one".into(),
                "2026-08-10T10:00:02Z tail two".into(),
                "2026-08-10T10:00:03Z tail three".into(),
            ],
        )
        .await
        .expect("history is spooled");
    appender
        .complete_backfill(12)
        .await
        .expect("history completion is accepted");

    assert!(matches!(
        wait_for(&service, |result| matches!(
            result,
            LogStoreResult::Rebased { .. }
        )),
        LogStoreResult::Rebased {
            window_id: 12,
            total_lines: 5,
            history_lines: 4,
            live_start: 2,
        }
    ));

    assert!(service.load_page(12, 0, false, 0));
    let LogStoreResult::PageLoaded {
        total_rows, rows, ..
    } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::PageLoaded { .. })
    })
    else {
        unreachable!()
    };
    assert_eq!(total_rows, 5);
    assert_eq!(
        rows.into_iter().map(|row| row.text).collect::<Vec<_>>(),
        vec![
            "history zero",
            "history one",
            "tail two",
            "tail three",
            "live four",
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rebase_keeps_new_live_records_after_the_overlapping_tail() {
    let service = LogStoreService::default();
    let appender = service.appender();
    service.open(13);
    appender
        .append(13, vec!["tail one".into(), "tail two".into()])
        .await
        .expect("initial tail is spooled");
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { .. })
    });
    appender
        .append_backfill(13, vec!["old".into(), "tail one".into()])
        .await
        .expect("history is spooled");
    appender
        .complete_backfill(13)
        .await
        .expect("history completion is accepted");
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Rebased { .. })
    });

    appender
        .append(13, vec!["new live".into()])
        .await
        .expect("live stream remains writable after rebase");
    assert!(matches!(
        wait_for(&service, |result| {
            matches!(
                result,
                LogStoreResult::Updated {
                    window_id: 13,
                    total_lines: 4,
                    ..
                }
            )
        }),
        LogStoreResult::Updated { .. }
    ));

    assert!(service.load_page(13, 0, false, 0));
    let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::PageLoaded { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(
        rows.into_iter().map(|row| row.text).collect::<Vec<_>>(),
        vec!["old", "tail one", "tail two", "new live"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unmatched_backfill_keeps_both_segments_instead_of_losing_live_logs() {
    let service = LogStoreService::default();
    let appender = service.appender();
    service.open(14);
    appender
        .append(14, vec!["live one".into(), "live two".into()])
        .await
        .expect("initial tail is spooled");
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { .. })
    });
    appender
        .append_backfill(14, vec!["history one".into(), "history two".into()])
        .await
        .expect("history is spooled");
    appender
        .complete_backfill(14)
        .await
        .expect("history completion is accepted");

    assert!(matches!(
        wait_for(&service, |result| matches!(
            result,
            LogStoreResult::Rebased { .. }
        )),
        LogStoreResult::Rebased {
            history_lines: 2,
            live_start: 0,
            total_lines: 4,
            ..
        }
    ));
    assert!(service.load_page(14, 0, false, 0));
    let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::PageLoaded { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(
        rows.into_iter().map(|row| row.text).collect::<Vec<_>>(),
        vec!["history one", "history two", "live one", "live two"]
    );
}
