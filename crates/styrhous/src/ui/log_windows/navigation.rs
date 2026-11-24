use super::*;

pub(super) fn advance_log_match(
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    forward: bool,
) -> bool {
    if window.search.match_count == 0 {
        return false;
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
    true
}

/// Make display row `row` the active one and scroll to it.
pub(super) fn jump_to_display_row(window: &mut PodLogWindowState, row: usize) {
    window.search.active_display_row = Some(row);
    window.search.scroll_to_display_row = Some(row);
    window.tail.release();
}

/// The error and warning rows of what the window displays, as the log
/// store last counted them (none while its answer is about something else).
pub(super) fn problem_rows(window: &PodLogWindowState) -> &[(usize, Problem)] {
    let Some(summary) = &window.summary else {
        return &[];
    };
    let current = match (window.filter_is_active(), summary.filtered_by) {
        (false, None) => true,
        (true, Some((generation, _))) => generation == window.search.generation,
        _ => false,
    };
    if current { &summary.problems } else { &[] }
}

/// Step to the next or previous error or warning row from the active row
/// (the top of the screen without one), wrapping at the ends. Returns
/// whether there was a problem to go to.
pub(super) fn step_problem(window: &mut PodLogWindowState, forward: bool) -> bool {
    let rows = problem_rows(window);
    let active = window.search.active_display_row;
    let from = active.unwrap_or(window.visible_top_display_row);
    let next = if forward {
        rows.iter()
            .map(|(row, _)| *row)
            .find(|row| {
                if active.is_some() {
                    *row > from
                } else {
                    *row >= from
                }
            })
            .or_else(|| rows.first().map(|(row, _)| *row))
    } else {
        rows.iter()
            .map(|(row, _)| *row)
            .rev()
            .find(|row| *row < from)
            .or_else(|| rows.last().map(|(row, _)| *row))
    };
    let Some(row) = next else {
        return false;
    };
    jump_to_display_row(window, row);
    true
}

/// Step to the next or previous displayed line from the active one, wrapping at
/// the ends (the last or the first without an active line). While a search that
/// filters the log is still running the ends are not known yet (more matches
/// may arrive), so stepping stops at them instead. Returns whether there was a
/// line to go to.
pub(super) fn step_line(window: &mut PodLogWindowState, forward: bool) -> bool {
    let count = displayed_line_count(window);
    if count == 0 {
        return false;
    }
    let ends_known = !window.filter_is_active() || window.search.search_complete;
    let next = match (window.search.active_display_row, forward) {
        (Some(row), true) if ends_known => (row + 1) % count,
        (Some(row), true) => (row + 1).min(count - 1),
        (Some(row), false) if ends_known => (row + count - 1) % count,
        (Some(row), false) => row.saturating_sub(1).min(count - 1),
        (None, true) => 0,
        (None, false) => count - 1,
    };
    jump_to_display_row(window, next);
    true
}

pub(super) fn status_label(window: &PodLogWindowState) -> String {
    let status = if !window.search.query.is_empty() && !window.search.search_complete {
        format!("Searching… {} matches", window.search.match_count)
    } else if initial_spool_is_pending(window) {
        format!("Spooling… {} lines", window.total_lines)
    } else if all_sources_are_reconnecting(window) {
        "Reconnecting…".to_owned()
    } else {
        match &window.status {
            PodLogStatus::Connecting => "Connecting…".to_owned(),
            PodLogStatus::Loading => "Loading previous logs…".to_owned(),
            PodLogStatus::Following => "Following".to_owned(),
            PodLogStatus::Finished => match window.instance {
                crate::worker::PodLogInstance::Current => "Stream finished".to_owned(),
                crate::worker::PodLogInstance::Previous => "Previous logs loaded".to_owned(),
            },
            PodLogStatus::Failed(error) => match window.instance {
                crate::worker::PodLogInstance::Current => format!("Stream failed: {error}"),
                crate::worker::PodLogInstance::Previous => {
                    format!("Previous logs unavailable: {error}")
                }
            },
        }
    };
    if let Some(backfill_lines) = window.backfill_lines {
        format!(
            "{status} · backfill {}",
            components::search::format_compact_count(backfill_lines)
        )
    } else {
        status
    }
}

pub(super) fn all_sources_are_reconnecting(window: &PodLogWindowState) -> bool {
    !window.targets.is_empty() && window.source_reconnects.len() == window.targets.len()
}
