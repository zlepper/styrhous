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
}

#[test]
fn live_tail_rows_bridge_disk_pages_only_while_following_bottom() {
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

    state.log_windows.get_mut(&1).unwrap().following_bottom = false;
    state.apply_log_store_result(LogStoreResult::Updated {
        window_id: 1,
        total_lines: 2,
        completed_search: None,
        appended_rows: vec![tail_row(1, "wait for disk")],
        backfill_lines: None,
    });
    let window = &state.log_windows[&1];
    assert_eq!(window.total_lines, 2);
    assert!(!window.live_rows.contains_key(&1));

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
    state.apply_log_store_result(LogStoreResult::Copied {
        window_id: 1,
        selection_generation: 3,
        text: "stale copy".into(),
    });
    state.apply_log_store_result(LogStoreResult::Copied {
        window_id: 1,
        selection_generation: 4,
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
