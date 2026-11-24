use super::*;

fn summary_of(
    service: &LogStoreService,
    window: u64,
    bars: usize,
    filter: Option<u64>,
) -> LogSummary {
    assert!(service.summarize(window, bars, filter));
    let LogStoreResult::Summary { summary, .. } = wait_for(service, |result| {
        matches!(result, LogStoreResult::Summary { .. })
    }) else {
        unreachable!()
    };
    (*summary).clone()
}

fn record(second: u32, source: &str, text: &str) -> LogRecord {
    LogRecord::from_source(
        format!("2026-08-10T10:00:{second:02}Z {text}"),
        source.to_owned(),
    )
}

#[test]
fn a_summary_counts_levels_sources_and_volume_over_time() {
    let service = LogStoreService::default();
    service.open(1);
    assert!(service.append_records(
        1,
        vec![
            record(0, "api-0", "INFO started"),
            record(0, "api-0", "INFO ready"),
            record(1, "worker-0", "WARN slow"),
            record(2, "api-0", "ERROR boom"),
            record(3, "worker-0", "no level here"),
        ],
    ));
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { total_lines: 5, .. })
    });

    let summary = summary_of(&service, 1, 2, None);
    assert_eq!(summary.total_lines, 5);
    assert_eq!((summary.errors, summary.warnings), (1, 1));
    assert_eq!(summary.span_seconds, Some(3));
    assert_eq!(
        summary.sources,
        vec![("api-0".to_owned(), 3), ("worker-0".to_owned(), 2)]
    );
    assert_eq!(
        summary.bars,
        vec![
            VolumeBucket {
                lines: 3,
                errors: 0,
                warnings: 1
            },
            VolumeBucket {
                lines: 2,
                errors: 1,
                warnings: 0
            },
        ]
    );
    assert_eq!(
        summary.problems,
        vec![(2, Problem::Warning), (3, Problem::Error)]
    );
    assert_eq!(summary.filtered_by, None);
}

#[test]
fn a_filtered_summary_numbers_the_problems_by_match_row() {
    let service = LogStoreService::default();
    service.open(2);
    assert!(service.append_records(
        2,
        vec![
            record(0, "api-0", "ERROR unrelated"),
            record(1, "api-0", "INFO worker ready"),
            record(2, "api-0", "WARN worker slow"),
            record(3, "api-0", "ERROR worker down"),
        ],
    ));
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { total_lines: 4, .. })
    });
    assert!(service.search(2, 7, "worker".to_owned(), false));
    let _ = wait_for(&service, |result| {
        matches!(
            result,
            LogStoreResult::SearchCompleted { match_count: 3, .. }
        )
    });

    let summary = summary_of(&service, 2, 4, Some(7));
    assert_eq!(summary.filtered_by, Some((7, 3)));
    assert_eq!(
        summary.problems,
        vec![(1, Problem::Warning), (2, Problem::Error)],
        "rows 1 and 2 of the three matches; the unrelated error is not one"
    );
    assert_eq!(
        (summary.errors, summary.warnings),
        (2, 1),
        "the totals still describe the whole log"
    );

    let stale = summary_of(&service, 2, 4, Some(99));
    assert_eq!(stale.filtered_by, None, "another search's generation");
    assert_eq!(stale.problems.len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rebase_counts_the_overlap_once_and_shifts_the_live_problems() {
    let service = LogStoreService::default();
    let appender = service.appender();
    service.open(3);
    appender
        .append(
            3,
            vec![
                record(2, "api-0", "ERROR tail two"),
                record(3, "api-0", "INFO tail three"),
                record(4, "api-0", "WARN live four"),
            ],
        )
        .await
        .expect("the tail is spooled");
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { total_lines: 3, .. })
    });
    appender
        .append_backfill(
            3,
            vec![
                record(0, "api-0", "WARN history zero"),
                record(1, "api-0", "INFO history one"),
                record(2, "api-0", "ERROR tail two"),
                record(3, "api-0", "INFO tail three"),
            ],
        )
        .await
        .expect("the history is spooled");
    appender.complete_backfill(3).await.expect("accepted");
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Rebased { .. })
    });

    let summary = summary_of(&service, 3, 8, None);
    assert_eq!(summary.total_lines, 5);
    assert_eq!(
        (summary.errors, summary.warnings),
        (1, 2),
        "the overlapping error is one line, not two"
    );
    assert_eq!(summary.sources, vec![("api-0".to_owned(), 5)]);
    assert_eq!(summary.bars.iter().map(|bar| bar.lines).sum::<usize>(), 5);
    assert_eq!(
        summary.problems,
        vec![
            (0, Problem::Warning),
            (2, Problem::Error),
            (4, Problem::Warning)
        ],
        "history rows keep their place and the live warning follows the overlap"
    );
}
