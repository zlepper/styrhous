use super::*;

pub(super) fn advance_log_match(
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    forward: bool,
) {
    if window.search.match_count == 0 {
        return;
    }
    let current = window.search.active_match.unwrap_or_else(|| {
        if forward {
            window.search.match_count - 1
        } else {
            0
        }
    });
    let next = if forward {
        (current + 1) % window.search.match_count
    } else {
        (current + window.search.match_count - 1) % window.search.match_count
    };
    window.search.active_match = Some(next);
    let _ = log_store.resolve_match(window.id, window.search.generation, next);
}

pub(super) fn advance_log_line(window: &mut PodLogWindowState, forward: bool) {
    let count = displayed_line_count(window);
    if count == 0 {
        return;
    }
    let current = window.search.active_display_row;
    let next = match (current, forward) {
        (Some(row), true) => (row + 1) % count,
        (Some(row), false) => (row + count - 1) % count,
        (None, true) => 0,
        (None, false) => count - 1,
    };
    window.search.active_display_row = Some(next);
    window.search.scroll_to_display_row = Some(next);
}

pub(super) fn status_label(window: &PodLogWindowState) -> String {
    let status = if !window.search.query.is_empty() && !window.search.search_complete {
        format!("Searching… {} matches", window.search.match_count)
    } else if initial_spool_is_pending(window) {
        format!("Spooling… {} lines", window.total_lines)
    } else {
        match &window.status {
            PodLogStatus::Connecting => "Connecting…".to_owned(),
            PodLogStatus::Following => "Following".to_owned(),
            PodLogStatus::Finished => "Stream finished".to_owned(),
            PodLogStatus::Failed(error) => format!("Stream failed: {error}"),
        }
    };
    if let Some(backfill_lines) = window.backfill_lines {
        format!("{status} · backfill {}", compact_line_count(backfill_lines))
    } else {
        status
    }
}

pub(super) fn compact_line_count(lines: usize) -> String {
    match lines {
        0..=999 => lines.to_string(),
        1_000..=999_999 => format!("{:.1}k", lines as f64 / 1_000.0),
        _ => format!("{:.1}M", lines as f64 / 1_000_000.0),
    }
}

pub(super) fn status_color(status: &PodLogStatus) -> egui::Color32 {
    match status {
        PodLogStatus::Connecting => gray::_400,
        PodLogStatus::Following => SUCCESS,
        PodLogStatus::Finished => gray::_400,
        PodLogStatus::Failed(_) => status::DANGER,
    }
}
