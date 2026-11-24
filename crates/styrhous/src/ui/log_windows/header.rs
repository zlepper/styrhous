//! The head of a log window: what it shows and how the stream is doing.

use super::*;
use components::tooltip::TooltipExt;
use components::{Button, ButtonSize, LogHeader, LogStream};

/// The window is too narrow for the full header and toolbar.
pub(super) const NARROW_WINDOW_WIDTH: f32 = 1100.0;

/// The pill of the header: how the stream is doing right now.
pub(super) fn log_stream(window: &PodLogWindowState) -> LogStream {
    let previous = window.instance == PodLogInstance::Previous;
    if !window.search.query.is_empty() && !window.search.search_complete {
        return LogStream::Working("Searching…".to_owned());
    }
    if initial_spool_is_pending(window) {
        return LogStream::Spooling;
    }
    if !window.source_reconnects.is_empty() {
        return LogStream::Reconnecting {
            down: window.source_reconnects.len(),
            total: window.targets.len(),
        };
    }
    match &window.status {
        PodLogStatus::Connecting => LogStream::Working("Connecting…".to_owned()),
        PodLogStatus::Loading => LogStream::Working("Loading previous logs…".to_owned()),
        PodLogStatus::Following => LogStream::Following,
        PodLogStatus::Finished if previous => {
            LogStream::Custom("Previous logs loaded".to_owned(), Tone::Neutral)
        }
        PodLogStatus::Finished => LogStream::Custom("Stream finished".to_owned(), Tone::Neutral),
        PodLogStatus::Failed(_) if previous => {
            LogStream::Custom("Previous logs unavailable".to_owned(), Tone::Danger)
        }
        PodLogStatus::Failed(_) => LogStream::Custom("Stream failed".to_owned(), Tone::Danger),
    }
}

/// The window has nothing to show because its stream failed before any line
/// arrived (no previous instance, a container that cannot be read).
pub(super) fn log_is_unavailable(window: &PodLogWindowState) -> bool {
    matches!(window.status, PodLogStatus::Failed(_)) && window.total_lines == 0
}

/// "container api · kube-system · 1,284 lines".
fn log_caption(window: &PodLogWindowState) -> String {
    let mut parts = Vec::new();
    match window.targets.as_slice() {
        [target] => {
            parts.push(format!("container {}", target.container));
            parts.push(target.namespace.clone());
        }
        targets => {
            parts.push(format!("{} sources", targets.len()));
            parts.push("interleaved by time".to_owned());
        }
    }
    if window.instance == PodLogInstance::Previous {
        parts.push("previous instance".to_owned());
    }
    if window.total_lines > 0 && !initial_spool_is_pending(window) {
        parts.push(line_count_text(window.total_lines, LINES));
    }
    parts.join(" · ")
}

fn log_title(window: &PodLogWindowState) -> String {
    match window.targets.as_slice() {
        [target] => target.pod_name.clone(),
        targets => format!("{} sources", targets.len()),
    }
}

/// The window header: the source, its live state, the instance and export.
pub(super) fn show_log_header(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &LogDisplayOptions,
) {
    let narrow = ui.ctx().content_rect().width() < NARROW_WINDOW_WIDTH;
    let title = log_title(window);
    let caption = log_caption(window);
    let stream = log_stream(window);
    let header = LogHeader::new("Pod", &title, &caption, stream).show(ui, |ui| {
        let mut instance = window.instance;
        // Switching replaces the window's store, which an export in progress
        // still needs.
        if Segmented::new(&mut instance)
            .option(PodLogInstance::Current, "Current")
            .option(PodLogInstance::Previous, "Previous")
            .label("Log instance")
            .enabled(!window.export.is_busy())
            .show(ui)
            .changed()
        {
            window.requested_instance = Some(instance);
        }
        if !narrow
            && Button::ghost("Container start")
                .size(ButtonSize::Sm)
                .tooltip("Jump to the first line")
                .show(ui)
                .clicked()
        {
            jump_to_container_start(window);
        }
        show_export_controls(ui, window, display_options, narrow);
    });
    if let Some(details) = status_details(window) {
        header.status.tooltip(details);
    }
}

/// Scroll to the first line of the log.
pub(super) fn jump_to_container_start(window: &mut PodLogWindowState) {
    if displayed_line_count(window) == 0 {
        return;
    }
    window.search.active_display_row = None;
    window.search.scroll_to_display_row = Some(0);
    window.tail.release();
}

/// What the status pill's tooltip says: the full status, and why a source is
/// reconnecting or unavailable.
pub(super) fn status_details(window: &PodLogWindowState) -> Option<String> {
    let mut lines = vec![status_label(window)];
    for (title, issues) in [
        ("reconnecting", &window.source_reconnects),
        ("unavailable", &window.source_failures),
    ] {
        if !issues.is_empty() {
            lines.push(format!(
                "{} source{} {title}:",
                issues.len(),
                if issues.len() == 1 { "" } else { "s" }
            ));
            lines.push(source_issue_details(issues));
        }
    }
    Some(lines.join("\n"))
}

fn source_issue_details(
    issues: &std::collections::HashMap<crate::worker::PodLogStreamTarget, String>,
) -> String {
    let mut details = issues
        .iter()
        .map(|(target, error)| format!("{}: {error}", target.display_name()))
        .collect::<Vec<_>>();
    details.sort();
    details.join("\n")
}

/// "1,284 lines", "1 match", "12.4k lines" (counts formatted like the
/// search field's).
pub(super) fn line_count_text(count: usize, [singular, plural]: [&str; 2]) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{} {noun}", components::search::format_count(count))
}

/// The nouns of line and match counts.
pub(super) const LINES: [&str; 2] = ["line", "lines"];
pub(super) const MATCHES: [&str; 2] = ["match", "matches"];

#[cfg(test)]
mod line_count_tests {
    use super::{LINES, MATCHES, line_count_text};

    #[test]
    fn counts_group_thousands_and_agree_in_number() {
        assert_eq!(line_count_text(0, LINES), "0 lines");
        assert_eq!(line_count_text(1, LINES), "1 line");
        assert_eq!(line_count_text(1_284, LINES), "1,284 lines");
        assert_eq!(line_count_text(12_400, LINES), "12.4k lines");
        assert_eq!(line_count_text(1, MATCHES), "1 match");
        assert_eq!(line_count_text(12, MATCHES), "12 matches");
    }
}
