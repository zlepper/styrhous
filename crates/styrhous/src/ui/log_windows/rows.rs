//! What the columns of a log row are and what its message is made of.

use super::*;
use crate::ansi::AnsiStyleSpan;
use crate::log_level::leading_level_len;
use components::{CodeColor, CodeDecoration, CodeMark, CodeSpan, LogLine};
use std::ops::Range;
use time::format_description::well_known::Rfc3339;

/// The width of a time like "15:22:10.000".
const TIME_CHARACTERS: f32 = 12.0;

/// The columns every row of one window shares, so their messages start in
/// one column.
#[derive(Clone, Copy, Debug)]
pub(super) struct RowLayout {
    pub(super) character_width: f32,
    pub(super) show_numbers: bool,
    pub(super) number_width: f32,
    show_times: bool,
    pub(super) source_width: Option<f32>,
    level_column: bool,
}

impl RowLayout {
    pub(super) fn new(
        ui: &egui::Ui,
        window: &PodLogWindowState,
        display_options: LogDisplayOptions,
    ) -> Self {
        let character_width = log_character_width(ui.ctx());
        let digits = window.total_lines.max(1).ilog10() as usize + 1;
        let source_width =
            (window.has_multiple_sources() && window.show_source_labels).then(|| {
                window
                    .source_labels
                    .values()
                    .map(|label| LogLine::source_name_width(ui, label))
                    .fold(0.0, f32::max)
            });
        Self {
            character_width,
            show_numbers: display_options.show_line_numbers,
            number_width: digits as f32 * character_width,
            show_times: display_options.show_timestamps,
            source_width,
            level_column: window.level_column,
        }
    }

    pub(super) fn time_width(&self) -> f32 {
        TIME_CHARACTERS * self.character_width
    }

    /// The time column of `row`: its time, or nothing where it has none;
    /// `None` while times are hidden.
    pub(super) fn time_text(&self, row: &LogPageRow) -> Option<String> {
        self.show_times.then(|| {
            row.timestamp
                .as_deref()
                .map(format_time)
                .unwrap_or_default()
        })
    }

    /// The row `row` as a [`LogLine`] with these columns, its `message`
    /// being the fragment of its text that is on screen (starting `offset`
    /// points in).
    pub(super) fn line<'a>(
        &self,
        window: &'a PodLogWindowState,
        row: &'a LogPageRow,
        time: Option<&'a str>,
        level: Option<LogLevel>,
        message: Vec<CodeSpan<'a>>,
        offset: f32,
    ) -> LogLine<'a> {
        let mut line = LogLine::new(message)
            .message_offset(offset)
            .reserve_level(self.level_column);
        if self.show_numbers {
            line = line
                .number(row.line_index + 1)
                .number_width(self.number_width);
        }
        if let Some(time) = time {
            line = line.time(time).time_width(self.time_width());
        }
        if let (Some(width), Some(source)) = (self.source_width, row.source.as_deref()) {
            let color = window.source_order.get(source).copied().unwrap_or_default();
            line = line
                .source(window.source_label(Some(source)).unwrap_or(source), color)
                .source_width(width);
        }
        if let Some(level) = level {
            line = line.level(level);
        }
        line
    }

    /// The width of everything before a row's message, which the caret and
    /// horizontal scrolling measure from.
    pub(super) fn prefix_width(&self, has_source: bool) -> f32 {
        let mut width = size::LOG_ROW_PADDING_X;
        if self.show_numbers {
            width += self.number_width.max(size::LOG_LINE_NUMBER) + space::S8;
        }
        if self.show_times {
            width += self.time_width() + space::S8;
        }
        if let (Some(source), true) = (self.source_width, has_source) {
            width += source.max(size::LOG_SOURCE_MIN_WIDTH) + space::S8;
        }
        if self.level_column {
            width += size::LEVEL_BADGE + space::S8;
        }
        width
    }
}

/// The bytes at the start of `row`'s text a level badge stands in for, which
/// are not shown (none when a search match lies in them).
pub(super) fn hidden_level_bytes(row: &LogPageRow, level: Option<LogLevel>) -> usize {
    if level.is_none() {
        return 0;
    }
    let hidden = leading_level_len(&row.text);
    if row.match_ranges.iter().any(|(start, _)| *start < hidden) {
        0
    } else {
        hidden
    }
}

/// The advance of one character of the log font.
pub(super) fn log_character_width(ctx: &egui::Context) -> f32 {
    type_scale::LOG
        .galley(ctx, "0", egui::Color32::PLACEHOLDER, f32::INFINITY)
        .rect
        .width()
}

/// A Kubernetes timestamp as the time of day in UTC, "15:22:10.000".
pub(super) fn format_time(timestamp: &str) -> String {
    let Ok(time) = time::OffsetDateTime::parse(timestamp, &Rfc3339) else {
        return String::new();
    };
    let time = time.to_offset(time::UtcOffset::UTC);
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        time.hour(),
        time.minute(),
        time.second(),
        time.millisecond()
    )
}

/// A Kubernetes timestamp with its date, "2026-08-08 15:22:10.123 UTC", for
/// the tooltip of a row (the time column leaves the date out).
pub(super) fn format_full_time(timestamp: &str) -> Option<String> {
    let time = time::OffsetDateTime::parse(timestamp, &Rfc3339)
        .ok()?
        .to_offset(time::UtcOffset::UTC);
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} UTC",
        time.year(),
        u8::from(time.month()),
        time.day(),
        time.hour(),
        time.minute(),
        time.second(),
        time.millisecond()
    ))
}

/// The look of a run of text with its ANSI styling.
fn decoration(style: anstyle::Style, theme: &Theme) -> CodeDecoration {
    let default = egui::TextFormat {
        color: theme.code.text,
        ..Default::default()
    };
    let format = ansi_text_format(style, &default, theme);
    CodeDecoration {
        color: format.color,
        background: (format.background != egui::Color32::TRANSPARENT).then_some(format.background),
        italics: format.italics,
        underline: format.underline != egui::Stroke::NONE,
        strikethrough: format.strikethrough != egui::Stroke::NONE,
    }
}

/// Everything that marks up the message of a row.
pub(super) struct MessageMarks<'a> {
    pub(super) style_spans: &'a [AnsiStyleSpan],
    pub(super) matches: &'a [(usize, usize)],
    /// The match the search is on, in the current row.
    pub(super) active: Option<(usize, usize)>,
    pub(super) selection: Option<(usize, usize)>,
    pub(super) render_ansi: bool,
}

/// The `fragment` of `text` as code spans: its ANSI styling, with search
/// matches underlined, the active match and the selection marked.
pub(super) fn message_spans<'a>(
    text: &'a str,
    fragment: Range<usize>,
    marks: &MessageMarks<'_>,
    theme: &Theme,
) -> Vec<CodeSpan<'a>> {
    let mut boundaries = vec![fragment.start, fragment.end];
    let edges = |range: &(usize, usize)| [range.0, range.1];
    boundaries.extend(marks.style_spans.iter().flat_map(|span| edges(&span.range)));
    boundaries.extend(marks.matches.iter().flat_map(edges));
    boundaries.extend(marks.active.iter().flat_map(edges));
    boundaries.extend(marks.selection.iter().flat_map(edges));
    boundaries.retain(|boundary| fragment.contains(boundary) || *boundary == fragment.end);
    boundaries.sort_unstable();
    boundaries.dedup();
    let contains = |range: &(usize, usize), start: usize| range.0 <= start && start < range.1;
    boundaries
        .windows(2)
        .map(|pair| {
            let (start, end) = (pair[0], pair[1]);
            let color = marks
                .render_ansi
                .then(|| {
                    marks
                        .style_spans
                        .iter()
                        .find(|span| contains(&span.range, start))
                        .map(|span| CodeColor::Decorated(decoration(span.style, theme)))
                })
                .flatten()
                .unwrap_or(CodeColor::Text);
            let span = CodeSpan::new(&text[start..end], color);
            if marks.selection.is_some_and(|range| contains(&range, start)) {
                span.mark(CodeMark::Selection)
            } else if marks.active.is_some_and(|range| contains(&range, start)) {
                span.mark(CodeMark::ActiveMatch)
            } else if marks.matches.iter().any(|range| contains(range, start)) {
                span.mark(CodeMark::Underline)
            } else {
                span
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_utc_time_of_day() {
        assert_eq!(format_time("2026-08-08T15:22:10.000Z"), "15:22:10.000");
        assert_eq!(format_time("2026-08-08T15:22:10.1234567Z"), "15:22:10.123");
        assert_eq!(format_time("2026-08-08T17:22:10+02:00"), "15:22:10.000");
        assert_eq!(format_time("not a time"), "");
        assert_eq!(
            format_full_time("2026-08-08T17:22:10.5+02:00").as_deref(),
            Some("2026-08-08 15:22:10.500 UTC")
        );
        assert_eq!(format_full_time("not a time"), None);
    }

    #[test]
    fn matches_are_underlined_and_the_active_one_filled() {
        let theme = Theme::default();
        let marks = MessageMarks {
            style_spans: &[],
            matches: &[(0, 4), (10, 14)],
            active: Some((0, 4)),
            selection: None,
            render_ansi: true,
        };
        let spans = message_spans("http: GET http", 0..14, &marks, &theme);
        let words: Vec<_> = spans
            .iter()
            .map(|span| (span.text.as_ref(), span.mark))
            .collect();
        assert_eq!(
            words,
            [
                ("http", Some(CodeMark::ActiveMatch)),
                (": GET ", None),
                ("http", Some(CodeMark::Underline)),
            ]
        );
    }

    #[test]
    fn a_fragment_only_carries_its_own_marks() {
        let theme = Theme::default();
        let marks = MessageMarks {
            style_spans: &[],
            matches: &[(0, 4)],
            active: None,
            selection: Some((6, 9)),
            render_ansi: true,
        };
        let spans = message_spans("http: GET /x", 5..12, &marks, &theme);
        let words: Vec<_> = spans
            .iter()
            .map(|span| (span.text.as_ref(), span.mark))
            .collect();
        assert_eq!(
            words,
            [
                (" ", None),
                ("GET", Some(CodeMark::Selection)),
                (" /x", None)
            ]
        );
    }
}
