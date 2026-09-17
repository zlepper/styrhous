use super::*;

#[test]
fn ignores_stale_pages_and_evicts_pages_using_the_injected_cache_limit() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let window = state.log_windows.get_mut(&1).expect("log window exists");
    window.page_cache_limit = 64;

    state.apply_log_store_result(LogStoreResult::PageLoaded {
        window_id: 1,
        generation: 1,
        filter_matches: false,
        page_start: 0,
        total_rows: 1,
        rows: vec![test_log_row(0, "stale page must be ignored")],
    });
    assert!(state.log_windows[&1].pages.is_empty());

    for page_start in [0, 1] {
        state.apply_log_store_result(LogStoreResult::PageLoaded {
            window_id: 1,
            generation: 0,
            filter_matches: false,
            page_start,
            total_rows: 2,
            rows: vec![test_log_row(page_start, &"x".repeat(64))],
        });
        if page_start == 0 {
            state
                .log_windows
                .get_mut(&1)
                .expect("log window exists")
                .pages_needing_refresh
                .insert(LogPageKey {
                    generation: 0,
                    filter_matches: false,
                    page_start: 0,
                });
        }
    }

    let window = &state.log_windows[&1];
    assert!(!window.pages.contains_key(&LogPageKey {
        generation: 0,
        filter_matches: false,
        page_start: 0,
    }));
    assert!(window.pages.contains_key(&LogPageKey {
        generation: 0,
        filter_matches: false,
        page_start: 1,
    }));
    assert!(window.pages_needing_refresh.is_empty());
}

#[test]
fn live_tail_rows_bridge_disk_pages_while_the_viewer_catches_up() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );

    let tail_row = |display_row, text: &str| LogPageRow {
        display_row,
        line_index: display_row,
        timestamp: None,
        source: None,
        text: text.to_owned(),
        style_spans: Vec::new(),
        match_ranges: Vec::new(),
    };
    state.apply_log_store_result(LogStoreResult::Updated {
        window_id: 1,
        total_lines: 1,
        completed_search: None,
        appended_rows: vec![tail_row(0, "live now")],
        backfill_lines: Some(12_345),
    });
    let window = &state.log_windows[&1];
    assert_eq!(window.backfill_lines, Some(12_345));
    assert_eq!(window.live_rows[&0].text, "live now");

    state.apply_log_store_result(LogStoreResult::Updated {
        window_id: 1,
        total_lines: 2,
        completed_search: None,
        appended_rows: vec![tail_row(1, "wait for disk")],
        backfill_lines: None,
    });
    let window = &state.log_windows[&1];
    assert_eq!(window.total_lines, 2);
    assert_eq!(window.live_rows[&1].text, "wait for disk");

    state.apply_log_store_result(LogStoreResult::PageLoaded {
        window_id: 1,
        generation: 0,
        filter_matches: false,
        page_start: 0,
        total_rows: 2,
        rows: vec![tail_row(0, "live now"), tail_row(1, "wait for disk")],
    });
    assert!(state.log_windows[&1].live_rows.is_empty());
}

#[test]
fn live_tail_rows_survive_a_completed_search_with_an_empty_filter_query() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let window = state.log_windows.get_mut(&1).expect("log window exists");
    window.search.filter_matches = true;
    window.insert_page(
        LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: 0,
        },
        vec![test_log_row(0, "cached")],
    );

    state.apply_log_store_result(LogStoreResult::Updated {
        window_id: 1,
        total_lines: 2,
        completed_search: Some((0, 0)),
        appended_rows: vec![test_log_row(1, "live now")],
        backfill_lines: None,
    });

    let window = &state.log_windows[&1];
    assert!(
        !window.pages.is_empty(),
        "live search updates keep cached pages renderable"
    );
    assert_eq!(window.live_rows[&1].text, "live now");
}

#[test]
fn live_search_updates_preserve_cached_pages_and_refresh_only_the_filtered_tail() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let window = state.log_windows.get_mut(&1).expect("log window exists");
    window.search.query = "error".into();
    window.search.generation = 4;
    window.search.match_count = 2;
    window.search.search_complete = true;
    let unfiltered_key = LogPageKey {
        generation: 4,
        filter_matches: false,
        page_start: 0,
    };
    let filtered_key = LogPageKey {
        generation: 4,
        filter_matches: true,
        page_start: 0,
    };
    window.insert_page(unfiltered_key, vec![test_log_row(0, "cached line")]);
    window.insert_page(filtered_key, vec![test_log_row(0, "cached match")]);

    let mut appended = test_log_row(1, "new error");
    appended.match_ranges = vec![(4, 9)];
    state.apply_log_store_result(LogStoreResult::Updated {
        window_id: 1,
        total_lines: 2,
        completed_search: Some((4, 3)),
        appended_rows: vec![appended],
        backfill_lines: None,
    });

    let window = &state.log_windows[&1];
    assert!(window.pages.contains_key(&unfiltered_key));
    assert!(window.pages.contains_key(&filtered_key));
    assert_eq!(window.search.match_count, 3);
    assert!(window.pages_needing_refresh.contains(&filtered_key));
    assert!(!window.pages_needing_refresh.contains(&unfiltered_key));
    assert_eq!(window.live_rows[&1].text, "new error");
}

#[test]
fn first_live_search_match_refreshes_a_cached_empty_page() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let window = state.log_windows.get_mut(&1).expect("log window exists");
    window.search.query = "error".into();
    window.search.generation = 5;
    window.search.match_count = 0;
    let filtered_key = LogPageKey {
        generation: 5,
        filter_matches: true,
        page_start: 0,
    };
    window.insert_page(filtered_key, Vec::new());

    state.apply_log_store_result(LogStoreResult::Updated {
        window_id: 1,
        total_lines: 1,
        completed_search: Some((5, 1)),
        appended_rows: vec![test_log_row(0, "first error")],
        backfill_lines: None,
    });

    let window = &state.log_windows[&1];
    assert!(window.pages.contains_key(&filtered_key));
    assert!(window.pages_needing_refresh.contains(&filtered_key));
    assert_eq!(window.search.match_count, 1);
}

#[test]
fn stale_filtered_page_result_remains_refreshable_after_more_matches_arrive() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let window = state.log_windows.get_mut(&1).expect("log window exists");
    window.search.query = "error".into();
    window.search.filter_matches = true;
    window.search.generation = 6;
    window.search.match_count = 3;
    let filtered_key = LogPageKey {
        generation: 6,
        filter_matches: true,
        page_start: 0,
    };
    window.pages_needing_refresh.insert(filtered_key);

    state.apply_log_store_result(LogStoreResult::PageLoaded {
        window_id: 1,
        generation: 6,
        filter_matches: true,
        page_start: 0,
        total_rows: 2,
        rows: vec![
            test_log_row(0, "first error"),
            test_log_row(1, "second error"),
        ],
    });

    let window = &state.log_windows[&1];
    assert_eq!(window.search.match_count, 3);
    assert!(window.pages.contains_key(&filtered_key));
    assert!(window.pages_needing_refresh.contains(&filtered_key));
}

#[test]
fn search_progress_keeps_renderable_pages_while_match_counts_grow() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let window = state.log_windows.get_mut(&1).expect("log window exists");
    window.search.query = "error".into();
    window.search.generation = 3;
    window.search.match_count = 2;
    window.search.search_complete = false;
    let filtered_key = LogPageKey {
        generation: 3,
        filter_matches: true,
        page_start: 0,
    };
    window.insert_page(filtered_key, vec![test_log_row(0, "cached match")]);

    state.apply_log_store_result(LogStoreResult::SearchProgress {
        window_id: 1,
        generation: 3,
        scanned_lines: 10,
        total_lines: 20,
        match_count: 4,
    });

    let window = &state.log_windows[&1];
    assert!(window.pages.contains_key(&filtered_key));
    assert!(window.pages_needing_refresh.contains(&filtered_key));
    assert_eq!(window.search.match_count, 4);
}

#[test]
fn log_store_reducer_applies_only_current_async_results() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let window = state.log_windows.get_mut(&1).expect("log window exists");
    window.search.generation = 3;
    window.selection_generation = 2;

    state.apply_log_store_result(LogStoreResult::SearchProgress {
        window_id: 1,
        generation: 2,
        scanned_lines: 10,
        total_lines: 20,
        match_count: 4,
    });
    assert_eq!(state.log_windows[&1].total_lines, 0);

    state.apply_log_store_result(LogStoreResult::SearchProgress {
        window_id: 1,
        generation: 3,
        scanned_lines: 10,
        total_lines: 20,
        match_count: 4,
    });
    state.apply_log_store_result(LogStoreResult::SearchCompleted {
        window_id: 1,
        generation: 3,
        match_count: 5,
    });
    let selection_generation = state.log_windows[&1].selection_generation;
    state.apply_log_store_result(LogStoreResult::Copied {
        window_id: 1,
        selection_generation: selection_generation.wrapping_sub(1),
        text: "stale copy".into(),
    });
    state.apply_log_store_result(LogStoreResult::Copied {
        window_id: 1,
        selection_generation,
        text: "current copy".into(),
    });

    let window = &state.log_windows[&1];
    assert_eq!(window.total_lines, 20);
    assert_eq!(window.search.scanned_lines, 20);
    assert_eq!(window.search.match_count, 5);
    assert!(window.search.search_complete);
    assert_eq!(window.copied_text.as_deref(), Some("current copy"));

    state.apply_log_store_result(LogStoreResult::Failed {
        window_id: 1,
        error: "disk full".into(),
    });
    assert_eq!(
        state.log_windows[&1].status,
        PodLogStatus::Failed("Log storage failed: disk full".into())
    );
}

#[test]
fn changing_a_log_selection_rejects_an_in_flight_copy_for_its_old_range() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let window = state.log_windows.get_mut(&1).expect("log window exists");
    let selection_start = LogTextPosition {
        display_row: 0,
        byte_offset: 0,
    };
    window.set_selection(Some(LogTextSelection {
        anchor: selection_start,
        focus: selection_start,
    }));
    let old_generation = window.selection_generation;
    window.set_selection(Some(LogTextSelection {
        anchor: selection_start,
        focus: LogTextPosition {
            display_row: 0,
            byte_offset: 8,
        },
    }));
    let current_generation = window.selection_generation;

    state.apply_log_store_result(LogStoreResult::Copied {
        window_id: 1,
        selection_generation: old_generation,
        text: "old range".into(),
    });
    assert!(state.log_windows[&1].copied_text.is_none());

    state.apply_log_store_result(LogStoreResult::Copied {
        window_id: 1,
        selection_generation: current_generation,
        text: "current range".into(),
    });
    assert_eq!(
        state.log_windows[&1].copied_text.as_deref(),
        Some("current range")
    );
}

#[test]
fn rebasing_maps_an_overlapping_tail_row_to_its_history_position() {
    assert_eq!(rebase_display_row(40, 200, 100), 140);
}

#[test]
fn rebasing_maps_live_records_after_the_overlap_without_an_extra_shift() {
    assert_eq!(rebase_display_row(120, 200, 100), 220);
}

#[test]
fn rebasing_without_overlap_places_the_live_segment_after_all_history() {
    assert_eq!(rebase_display_row(40, 100, 0), 140);
}

#[test]
fn resolved_matches_scroll_in_source_or_filtered_display_row_space() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    let window = state.log_windows.get_mut(&1).expect("log window exists");
    window.search.generation = 3;
    window.search.active_match = Some(4);

    state.apply_log_store_result(LogStoreResult::MatchResolved {
        window_id: 1,
        generation: 3,
        match_row: 4,
        line_index: 400,
    });
    assert_eq!(state.log_windows[&1].search.active_display_row, Some(400));
    assert_eq!(
        state.log_windows[&1].search.scroll_to_display_row,
        Some(400)
    );

    let window = state.log_windows.get_mut(&1).expect("log window exists");
    window.search.filter_matches = true;
    window.search.active_match = Some(5);
    state.apply_log_store_result(LogStoreResult::MatchResolved {
        window_id: 1,
        generation: 3,
        match_row: 5,
        line_index: 400,
    });
    assert_eq!(state.log_windows[&1].search.active_display_row, Some(5));
    assert_eq!(state.log_windows[&1].search.scroll_to_display_row, Some(5));
}
