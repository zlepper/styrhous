use super::*;

#[test]
fn stores_complete_lines_and_returns_only_requested_page() {
    let service = LogStoreService::default();
    service.open(9);
    service.append(9, (0..600).map(|index| format!("line {index}")).collect());
    let result = wait_for(&service, |result| {
        matches!(
            result,
            LogStoreResult::Updated {
                total_lines: 600,
                ..
            }
        )
    });
    assert!(matches!(
        result,
        LogStoreResult::Updated {
            window_id: 9,
            total_lines: 600,
            ..
        }
    ));

    service.load_page(9, 0, false, 256);
    let LogStoreResult::PageLoaded {
        total_rows, rows, ..
    } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::PageLoaded { .. })
    })
    else {
        unreachable!()
    };
    assert_eq!(total_rows, 600);
    assert_eq!(rows.len(), LOG_PAGE_SIZE);
    assert_eq!(rows[0].text, "line 256");
}

#[test]
fn copy_reads_only_the_selected_utf8_log_text_from_the_spool() {
    let service = LogStoreService::default();
    let first = "2026-08-10T12:34:56Z  alphaé";
    let second = "2026-08-10T12:34:57Z  beta";
    assert!(service.append(8, vec![first.to_owned(), second.to_owned()]));
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { window_id: 8, .. })
    });
    let first_text = parse_kubernetes_log_line(first).line.text;
    let second_text = parse_kubernetes_log_line(second).line.text;
    let start = first_text.find('é').expect("utf8 character is present");
    let end = second_text.len();

    assert!(service.copy(8, 3, 0, false, 0, start, 1, end));
    let LogStoreResult::Copied {
        selection_generation,
        text,
        ..
    } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Copied { .. })
    })
    else {
        unreachable!()
    };

    assert_eq!(selection_generation, 3);
    assert_eq!(text, format!("é\n{second_text}"));
}

#[test]
fn loads_only_the_pages_requested_while_scrolling() {
    let service = LogStoreService::new(LogStoreConfig {
        page_size: 2,
        ..LogStoreConfig::default()
    });
    service.append(
        3,
        ["line 0", "line 1", "line 2", "line 3", "line 4"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { .. })
    });

    // Simulate the virtual scroll area reaching the second and then third
    // page. No full-log result is ever returned to the caller.
    service.load_page(3, 0, false, 2);
    let LogStoreResult::PageLoaded {
        page_start, rows, ..
    } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::PageLoaded { page_start: 2, .. })
    })
    else {
        unreachable!()
    };
    assert_eq!(page_start, 2);
    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
        ["line 2", "line 3"]
    );

    service.load_page(3, 0, false, 4);
    let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::PageLoaded { page_start: 4, .. })
    }) else {
        unreachable!()
    };
    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
        ["line 4"]
    );
}

#[test]
fn search_is_async_and_returns_match_ranges() {
    let service = LogStoreService::default();
    service.append(
        5,
        vec![
            "api ready".into(),
            "worker ready".into(),
            "API stopped".into(),
        ],
    );
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { .. })
    });
    service.search(5, 1, "api".into(), false);
    let LogStoreResult::SearchCompleted { match_count, .. } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::SearchCompleted { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(match_count, 2);

    service.load_page(5, 1, true, 0);
    let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::PageLoaded { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].match_ranges, vec![(0, 3)]);
}

#[test]
fn search_ignores_ansi_sequences_and_matches_across_style_boundaries() {
    let service = LogStoreService::default();
    service.append(
        5,
        vec!["\u{1b}[31map\u{1b}[0mi ready".into(), "worker ready".into()],
    );
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { .. })
    });

    service.search(5, 1, "api".into(), false);
    let LogStoreResult::SearchCompleted { match_count, .. } = wait_for(&service, |result| {
        matches!(
            result,
            LogStoreResult::SearchCompleted { generation: 1, .. }
        )
    }) else {
        unreachable!()
    };
    assert_eq!(match_count, 1);

    service.load_page(5, 1, true, 0);
    let LogStoreResult::PageLoaded { rows, .. } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::PageLoaded { generation: 1, .. })
    }) else {
        unreachable!()
    };
    assert_eq!(rows[0].text, "api ready");
    assert_eq!(rows[0].match_ranges, vec![(0, 3)]);
    assert_eq!(rows[0].style_spans[0].range, (0, 2));

    service.search(5, 2, "a.i".into(), true);
    let LogStoreResult::SearchCompleted { match_count, .. } = wait_for(&service, |result| {
        matches!(
            result,
            LogStoreResult::SearchCompleted { generation: 2, .. }
        )
    }) else {
        unreachable!()
    };
    assert_eq!(match_count, 1);
}

#[test]
fn search_includes_lines_appended_while_the_initial_scan_runs() {
    let service = LogStoreService::default();
    service.append(5, vec!["api starting".into()]);
    let _ = wait_for(&service, |result| {
        matches!(result, LogStoreResult::Updated { .. })
    });

    service.search(5, 1, "api".into(), false);
    service.append(5, vec!["api ready".into()]);

    let LogStoreResult::SearchCompleted { match_count, .. } = wait_for(&service, |result| {
        matches!(result, LogStoreResult::SearchCompleted { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(match_count, 2);
}

#[test]
fn temporary_files_are_removed_when_a_store_is_dropped() {
    let store = LogStore::new();
    let data_path = store
        .data
        .as_ref()
        .expect("data file exists")
        .path()
        .to_owned();
    let offsets_path = store
        .offsets
        .as_ref()
        .expect("offset index exists")
        .path()
        .to_owned();
    assert!(data_path.exists());
    assert!(offsets_path.exists());

    drop(store);

    assert!(!data_path.exists());
    assert!(!offsets_path.exists());
}
