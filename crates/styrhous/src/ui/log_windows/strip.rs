//! The volume strip: how busy the log was, how many errors and warnings it
//! holds, its sources and which part of it is on screen.

use super::*;
use crate::log_store::{LogSummary, VolumeBucket};
use components::{SourcePill, VolumeBar, VolumeBars, VolumeStrip};

/// Fewest seconds between two summary requests of a changing log.
const SUMMARY_INTERVAL: f64 = 0.5;
/// A summary request that has not been answered by now is asked again.
const SUMMARY_TIMEOUT: f64 = 2.0;

/// How many bars fit a strip `width` wide.
pub(super) fn bars_that_fit(width: f32) -> usize {
    let pitch = size::VOLUME_BAR + size::VOLUME_BAR_GAP;
    ((width + size::VOLUME_BAR_GAP) / pitch).floor().max(0.0) as usize
}

/// "40 s", "15 min", "6 h", "3 d": the span a log's volume covers.
pub(super) fn span_label(seconds: u64) -> String {
    match seconds {
        0..=89 => format!("{} s", seconds.max(1)),
        90..=5_399 => format!("{} min", (seconds as f64 / 60.0).round()),
        5_400..=129_599 => format!("{} h", (seconds as f64 / 3_600.0).round()),
        _ => format!("{} d", (seconds as f64 / 86_400.0).round()),
    }
}

/// The caption of the strip: "Volume · 15 min · 2 errors · 4 warnings".
pub(super) fn strip_caption(window: &PodLogWindowState) -> String {
    if log_is_unavailable(window) && window.instance == PodLogInstance::Previous {
        return "Volume · previous instance".to_owned();
    }
    if initial_spool_is_pending(window) {
        return "Volume · spooling".to_owned();
    }
    let Some(summary) = &window.summary else {
        return "Volume".to_owned();
    };
    let mut parts = vec!["Volume".to_owned()];
    if let Some(seconds) = summary.span_seconds {
        parts.push(span_label(seconds));
    }
    if window.filter_is_active() && window.search.search_complete {
        parts.push(line_count_text(window.search.match_count, MATCHES));
    } else {
        for (count, singular, plural) in [
            (summary.errors, "error", "errors"),
            (summary.warnings, "warning", "warnings"),
        ] {
            if count > 0 {
                parts.push(components::search::counted(count, singular, plural));
            }
        }
    }
    parts.join(" · ")
}

/// The muted words before the source pills: the search position or the
/// backfill, and the time zone of the times.
pub(super) fn strip_meta(
    window: &PodLogWindowState,
    display_options: &LogDisplayOptions,
) -> Option<String> {
    if window.total_lines == 0 {
        return None;
    }
    let search = &window.search;
    let mut parts = Vec::new();
    if !search.query.is_empty() && search.search_complete && search.error.is_none() {
        // A filtered view says how many matches it holds in the caption.
        if !window.filter_is_active() {
            parts.push(match search.active_match {
                Some(active) => format!(
                    "{} of {}",
                    components::search::format_count(active + 1),
                    line_count_text(search.match_count, MATCHES)
                ),
                None => line_count_text(search.match_count, MATCHES),
            });
        }
    }
    if let Some(backfill) = window.backfill_lines {
        parts.push(format!(
            "backfill {}",
            components::search::format_compact_count(backfill)
        ));
    }
    if display_options.show_timestamps {
        parts.push("UTC".to_owned());
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The bars of a summary, the busiest filling most of the chart.
fn volume_bars(summary: &LogSummary) -> Vec<VolumeBar> {
    let busiest = summary
        .bars
        .iter()
        .map(|bar| bar.lines)
        .max()
        .unwrap_or_default()
        .max(1) as f32;
    summary
        .bars
        .iter()
        .map(|bar| {
            let busy = bar.lines as f32 / busiest;
            let tone = if bar.errors > 0 {
                Some(Tone::Danger)
            } else if bar.warnings > 0 {
                Some(Tone::Warning)
            } else {
                None
            };
            let height = match (bar.lines, tone) {
                (0, _) => size::VOLUME_BAR_EMPTY,
                (_, Some(_)) => size::VOLUME_BAR_PROBLEM + size::VOLUME_BAR_PROBLEM_SPAN * busy,
                _ => {
                    size::VOLUME_BAR_QUIET + (size::VOLUME_BAR_BUSY - size::VOLUME_BAR_QUIET) * busy
                }
            };
            VolumeBar { height, tone }
        })
        .collect()
}

/// Where line `line` is among the bars, in bars from the left (a bar holds
/// its lines in order, so the position inside it is proportional).
pub(super) fn bar_position(bars: &[VolumeBucket], line: usize) -> f32 {
    let mut before = 0;
    for (index, bar) in bars.iter().enumerate() {
        if line < before + bar.lines {
            return index as f32 + (line - before) as f32 / bar.lines as f32;
        }
        before += bar.lines;
    }
    bars.len() as f32
}

/// The line at `position` bars from the left: the inverse of
/// [`bar_position`].
pub(super) fn line_at_position(bars: &[VolumeBucket], position: f32) -> usize {
    let position = position.max(0.0);
    let index = position.floor() as usize;
    let before: usize = bars.iter().take(index).map(|bar| bar.lines).sum();
    match bars.get(index) {
        Some(bar) => before + (position.fract() * bar.lines as f32) as usize,
        None => before.saturating_sub(1),
    }
}

/// The log line shown at display row `row`.
fn line_of_row(window: &PodLogWindowState, row: usize) -> Option<usize> {
    if !window.filter_is_active() {
        return Some(row);
    }
    log_row_for_display_row(window, row).map(|row| row.line_index)
}

/// The part of the log on screen, as `(start bar, bars long)`.
fn visible_window(window: &PodLogWindowState, summary: &LogSummary) -> Option<(f32, f32)> {
    let counted: usize = summary.bars.iter().map(|bar| bar.lines).sum();
    if counted == 0
        || window.total_lines == 0
        || initial_spool_is_pending(window)
        || log_is_unavailable(window)
    {
        return None;
    }
    let scale = |line: usize| line * counted / window.total_lines.max(1);
    let top = line_of_row(window, window.visible_top_display_row)?;
    let bottom_row =
        (window.visible_top_display_row + window.visible_rows).min(displayed_line_count(window));
    let bottom = line_of_row(window, bottom_row.saturating_sub(1)).unwrap_or(top);
    let start = bar_position(&summary.bars, scale(top));
    let end = bar_position(&summary.bars, scale(bottom + 1));
    Some((start, (end - start).max(1.0)))
}

/// Scroll so the visible window moves `dragged` bars.
pub(super) fn drag_window(window: &mut PodLogWindowState, summary: &LogSummary, dragged: f32) {
    let counted: usize = summary.bars.iter().map(|bar| bar.lines).sum();
    let Some((start, _)) = visible_window(window, summary) else {
        return;
    };
    if counted == 0 {
        return;
    }
    let line =
        line_at_position(&summary.bars, start + dragged) * window.total_lines.max(1) / counted;
    let row = if window.filter_is_active() {
        line * displayed_line_count(window) / window.total_lines.max(1)
    } else {
        line
    };
    window.search.scroll_to_display_row =
        Some(row.min(displayed_line_count(window).saturating_sub(1)));
    window.search.active_display_row = None;
    window.tail.release();
}

/// What a source pill says about its source.
fn source_pills<'a>(
    window: &'a PodLogWindowState,
    summary: Option<&LogSummary>,
) -> Vec<SourcePill<'a>> {
    let spooling = initial_spool_is_pending(window);
    let unavailable = log_is_unavailable(window);
    window
        .targets
        .iter()
        .map(|target| {
            let key = target.display_name();
            let name = window
                .source_labels
                .get(&key)
                .map_or(target.pod_name.as_str(), String::as_str);
            let lines = if window.has_multiple_sources() {
                summary
                    .and_then(|summary| {
                        summary
                            .sources
                            .iter()
                            .find(|(source, _)| *source == key)
                            .map(|(_, count)| *count)
                    })
                    .unwrap_or_default()
            } else {
                window.total_lines
            };
            let pill = if unavailable {
                SourcePill::offline(name).detail("no data")
            } else if window.source_failures.contains_key(target) {
                SourcePill::offline(name)
            } else if window.source_reconnects.contains_key(target) {
                SourcePill::reconnecting(name).detail(reconnect_detail(lines))
            } else if spooling {
                SourcePill::spooling(name, lines)
            } else {
                SourcePill::live(name, lines)
            };
            pill.not_implemented()
        })
        .collect()
}

/// "4,502 · reconnecting".
fn reconnect_detail(lines: usize) -> String {
    format!(
        "{} · reconnecting",
        components::search::format_exact_count(lines)
    )
}

/// The strip across the window, and keeping its summary fresh.
pub(super) fn show_volume_strip(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &LogDisplayOptions,
    log_store: &LogStoreService,
) {
    let bars_wanted = bars_that_fit(ui.available_width() - size::VOLUME_STRIP_PADDING.sum().x);
    request_summary_if_stale(ui.ctx(), window, log_store, bars_wanted);

    let caption = strip_caption(window);
    let meta = strip_meta(window, display_options);
    let summary = window.summary.clone();
    let bars = summary.as_deref().map(volume_bars).unwrap_or_default();
    let volume = match summary
        .as_ref()
        .and_then(|summary| visible_window(window, summary))
    {
        Some((start, length)) => VolumeBars::new(&bars).window(start, length),
        None => VolumeBars::new(&bars),
    };
    let mut strip = VolumeStrip::new(&caption, volume).height(size::VOLUME_STRIP);
    if let Some(meta) = &meta {
        strip = strip.meta(meta);
    }
    for pill in source_pills(window, summary.as_deref()) {
        strip = strip.source(pill);
    }
    let response = strip.show(ui);
    if response.window_dragged != 0.0
        && let Some(summary) = &summary
    {
        drag_window(window, summary, response.window_dragged);
    }
}

/// Ask the store for a fresh summary when the log, the search or the width
/// of the strip changed, no more often than [`SUMMARY_INTERVAL`].
pub(super) fn request_summary_if_stale(
    ctx: &egui::Context,
    window: &mut PodLogWindowState,
    log_store: &LogStoreService,
    bars: usize,
) {
    if !window.store_opened {
        return;
    }
    let now = ctx.input(|input| input.time);
    let wanted = SummaryRequest {
        at: now,
        bars,
        filter: window
            .filter_is_active()
            .then_some(window.search.generation),
        total_lines: window.total_lines,
        match_count: if window.filter_is_active() {
            window.search.match_count
        } else {
            0
        },
        answered: false,
    };
    let stale = window
        .summary_request
        .is_none_or(|asked| !asked.same_question(&wanted));
    if !stale {
        return;
    }
    let pending = window
        .summary_request
        .is_some_and(|asked| !asked.answered && now - asked.at < SUMMARY_TIMEOUT);
    let waited = window
        .summary_request
        .map_or(f64::INFINITY, |asked| now - asked.at);
    if pending || waited < SUMMARY_INTERVAL {
        let remaining = if pending {
            SUMMARY_TIMEOUT
        } else {
            SUMMARY_INTERVAL - waited
        };
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining.max(0.0)));
        return;
    }
    if log_store.summarize(window.id, bars, wanted.filter) {
        window.summary_request = Some(wanted);
    } else {
        // The store was too busy to take the request; offer it again soon.
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SUMMARY_INTERVAL));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(lines: &[usize]) -> Vec<VolumeBucket> {
        lines
            .iter()
            .map(|&lines| VolumeBucket {
                lines,
                ..VolumeBucket::default()
            })
            .collect()
    }

    #[test]
    fn spans_read_in_the_nearest_unit() {
        assert_eq!(span_label(0), "1 s");
        assert_eq!(span_label(40), "40 s");
        assert_eq!(span_label(900), "15 min");
        assert_eq!(span_label(3 * 3_600), "3 h");
        assert_eq!(span_label(3 * 86_400), "3 d");
    }

    #[test]
    fn bars_that_fit_counts_the_gaps_between_them() {
        assert_eq!(bars_that_fit(0.0), 0);
        assert_eq!(bars_that_fit(5.0), 1);
        assert_eq!(bars_that_fit(13.0), 2);
        assert_eq!(bars_that_fit(1_444.0), 180);
    }

    #[test]
    fn lines_and_bar_positions_map_to_each_other() {
        let bars = bars(&[10, 0, 30, 20]);
        assert_eq!(bar_position(&bars, 0), 0.0);
        assert_eq!(bar_position(&bars, 5), 0.5);
        assert_eq!(bar_position(&bars, 10), 2.0, "an empty bar is skipped");
        assert_eq!(bar_position(&bars, 25), 2.5);
        assert_eq!(bar_position(&bars, 60), 4.0);
        assert_eq!(line_at_position(&bars, 0.5), 5);
        assert_eq!(line_at_position(&bars, 2.5), 25);
        assert_eq!(line_at_position(&bars, 99.0), 59);
    }

    #[test]
    fn problem_bars_stand_out_from_quiet_ones_and_tower_over_busy_ones() {
        let bucket = |lines, errors, warnings| VolumeBucket {
            lines,
            errors,
            warnings,
        };
        let summary = LogSummary {
            bars: vec![
                bucket(100, 0, 0),
                bucket(1, 0, 0),
                bucket(1, 1, 0),
                bucket(100, 0, 3),
                bucket(0, 0, 0),
            ],
            ..LogSummary::default()
        };
        let bars = volume_bars(&summary);
        assert_eq!(bars[0].tone, None);
        assert_eq!(bars[2].tone, Some(Tone::Danger));
        assert_eq!(bars[3].tone, Some(Tone::Warning));
        assert!(
            bars[2].height > bars[1].height,
            "an error bar beats a quiet one"
        );
        assert!(
            bars[3].height > bars[0].height,
            "a busy problem bar beats a busy one"
        );
        assert!(
            bars[4].height < bars[1].height,
            "an empty stretch is a stub"
        );
    }
}
