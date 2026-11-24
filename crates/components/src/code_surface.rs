//! The code surface (C15): one row model for log lines and editor lines.
//!
//! Rows are IBM Plex Mono 13/20 on `code-surface`, never wrapped. An
//! optional gutter right-aligns line numbers in `code-gutter` (`code-error`
//! for a line with a diagnostic) in [`size::CODE_GUTTER`], growing by one
//! character per digit past four; the editor draws a `code-divider` rule
//! after it. Without a gutter, rows are indented [`space::S16`].
//!
//! Highlights are layered back to front: the current or hovered row
//! (`code-line-highlight`), then the selection (`code-selection`), search
//! matches (`code-match`) and the active match (`code-match-active`), each
//! behind its own characters. A row may carry a caret (`code-caret`) and
//! diagnostic squiggles (`code-error`).

use std::borrow::Cow;
use std::ops::RangeInclusive;
use std::sync::Arc;

use egui::epaint::{PathShape, PathStroke};
use egui::text::LayoutJob;
use egui::{Color32, CornerRadius, Galley, Pos2, Rect, Sense, Ui, pos2, vec2};

use crate::design::theme::{Theme, ThemeExt};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space};

/// Digits that fit the base gutter.
const GUTTER_DIGITS: usize = 4;

/// The colour of a run of code, as a theme role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeColor {
    /// `code-text`.
    Text,
    /// `code-text-muted`: timestamps, placeholders.
    Muted,
    /// `code-error`: the ERROR level.
    Error,
    /// `code-warning`: the WARN level.
    Warning,
    /// `code-info`: the INFO level.
    Info,
    /// `code-debug`: the DEBUG level.
    Debug,
    /// The colour of the `n`th log source (`log-source-1`, `log-source-2`,
    /// repeating).
    Source(usize),
    /// `syntax-key`.
    SyntaxKey,
    /// `syntax-string`.
    SyntaxString,
    /// `syntax-number`.
    SyntaxNumber,
    /// `syntax-bool`: booleans and null.
    SyntaxBool,
    /// `syntax-punctuation`.
    SyntaxPunctuation,
    /// `syntax-comment`.
    SyntaxComment,
    /// A colour of the line's own (ANSI styling), already resolved.
    Custom(Color32),
    /// A run that carries its own look (ANSI styling): colours and text
    /// decorations, already resolved.
    Decorated(CodeDecoration),
}

/// The look of a run that styles itself (ANSI escapes in a log line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeDecoration {
    pub color: Color32,
    pub background: Option<Color32>,
    pub italics: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

impl CodeDecoration {
    /// A plain run in `color`.
    pub const fn new(color: Color32) -> Self {
        Self {
            color,
            background: None,
            italics: false,
            underline: false,
            strikethrough: false,
        }
    }
}

impl CodeColor {
    /// The colour in `theme`.
    pub fn resolve(self, theme: &Theme) -> Color32 {
        match self {
            Self::Text => theme.code.text,
            Self::Muted => theme.code.text_muted,
            Self::Error => theme.code.error,
            Self::Warning => theme.code.warning,
            Self::Info => theme.code.info,
            Self::Debug => theme.code.debug,
            Self::Source(index) => [theme.log.source_1, theme.log.source_2][index % 2],
            Self::SyntaxKey => theme.syntax.key,
            Self::SyntaxString => theme.syntax.string,
            Self::SyntaxNumber => theme.syntax.number,
            Self::SyntaxBool => theme.syntax.bool,
            Self::SyntaxPunctuation => theme.syntax.punctuation,
            Self::SyntaxComment => theme.syntax.comment,
            Self::Custom(color) => color,
            Self::Decorated(decoration) => decoration.color,
        }
    }
}

/// A highlight behind a run of code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeMark {
    /// Selected text (`code-selection`).
    Selection,
    /// A search match (`code-match` behind `code-match-text`).
    Match,
    /// The active search match (`code-match-active` behind
    /// `code-match-active-text`).
    ActiveMatch,
    // C20
    /// A search match in the log viewer: underlined in `accent-fill`, the
    /// text keeping its colour (`.ll .lm`).
    Underline,
}

/// A run of code in one colour.
#[derive(Clone, Debug)]
pub struct CodeSpan<'a> {
    pub text: Cow<'a, str>,
    pub color: CodeColor,
    pub mark: Option<CodeMark>,
    /// Underline the run with a diagnostic squiggle.
    pub squiggle: bool,
}

impl<'a> CodeSpan<'a> {
    pub fn new(text: impl Into<Cow<'a, str>>, color: CodeColor) -> Self {
        Self {
            text: text.into(),
            color,
            mark: None,
            squiggle: false,
        }
    }

    /// Plain `code-text`.
    pub fn text(text: impl Into<Cow<'a, str>>) -> Self {
        Self::new(text, CodeColor::Text)
    }

    pub fn mark(mut self, mark: CodeMark) -> Self {
        self.mark = Some(mark);
        self
    }

    pub fn squiggle(mut self) -> Self {
        self.squiggle = true;
        self
    }
}

/// One row of a [`CodeSurface`].
#[derive(Clone, Debug, Default)]
pub struct CodeLine<'a> {
    /// The line number shown in the gutter.
    pub number: Option<usize>,
    pub spans: Vec<CodeSpan<'a>>,
    /// The current line or the hovered row (`code-line-highlight`).
    pub highlighted: bool,
    /// The line has an error diagnostic: its number is `code-error`.
    pub error: bool,
    /// The caret, before this character of the line.
    pub caret: Option<usize>,
}

impl<'a> CodeLine<'a> {
    pub fn new(spans: impl IntoIterator<Item = CodeSpan<'a>>) -> Self {
        Self {
            spans: spans.into_iter().collect(),
            ..Self::default()
        }
    }

    pub fn number(mut self, number: usize) -> Self {
        self.number = Some(number);
        self
    }

    pub fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }

    pub fn error(mut self, error: bool) -> Self {
        self.error = error;
        self
    }

    pub fn caret(mut self, column: usize) -> Self {
        self.caret = Some(column);
        self
    }

    /// The line's text.
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_ref()).collect()
    }
}

/// Where a [`CodeSurface`] painted its rows.
#[derive(Debug)]
pub struct CodeSurfaceResponse {
    /// Each line's row, in order.
    pub rows: Vec<Rect>,
    /// The caret, if a line has one.
    pub caret: Option<Rect>,
    /// The surface.
    pub rect: Rect,
}

/// The code surface: [`CodeLine`]s on `code-surface`, filling the
/// available width.
#[must_use = "call `show` to add the surface"]
pub struct CodeSurface<'l, 'a> {
    lines: &'l [CodeLine<'a>],
    gutter: bool,
    gutter_rule: bool,
    corner_radius: CornerRadius,
    padding_top: f32,
    padding_bottom: f32,
    min_height: f32,
}

impl<'l, 'a> CodeSurface<'l, 'a> {
    pub fn new(lines: &'l [CodeLine<'a>]) -> Self {
        Self {
            lines,
            gutter: true,
            gutter_rule: false,
            corner_radius: CornerRadius::ZERO,
            padding_top: size::CODE_PADDING_TOP,
            padding_bottom: 0.0,
            min_height: 0.0,
        }
    }

    /// Leave out the line-number gutter (interleaved log sources).
    pub fn without_gutter(mut self) -> Self {
        self.gutter = false;
        self
    }

    /// Draw the editor's `code-divider` rule after the gutter.
    pub fn gutter_rule(mut self) -> Self {
        self.gutter_rule = true;
        self
    }

    pub fn corner_radius(mut self, corner_radius: impl Into<CornerRadius>) -> Self {
        self.corner_radius = corner_radius.into();
        self
    }

    /// Space below the last row (the top always has
    /// [`size::CODE_PADDING_TOP`]).
    pub fn padding_bottom(mut self, padding: f32) -> Self {
        self.padding_bottom = padding;
        self
    }

    /// Be at least `height` tall, the rows at its top.
    pub fn min_height(mut self, height: f32) -> Self {
        self.min_height = height;
        self
    }

    /// The width of the gutter for line numbers up to `largest`.
    pub fn gutter_width(ui: &Ui, largest: usize) -> f32 {
        let digits = largest.max(1).ilog10() as usize + 1;
        let extra = digits.saturating_sub(GUTTER_DIGITS) as f32;
        size::CODE_GUTTER + extra * char_width(ui)
    }

    pub fn show(self, ui: &mut Ui) -> CodeSurfaceResponse {
        let theme = ui.tokens();
        let row_height = type_scale::MONO.line_height();
        let height =
            (self.padding_top + self.lines.len() as f32 * row_height + self.padding_bottom)
                .max(self.min_height);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let mut result = CodeSurfaceResponse {
            rows: Vec::with_capacity(self.lines.len()),
            caret: None,
            rect,
        };
        let visible = ui.is_rect_visible(rect);
        if visible {
            ui.painter()
                .rect_filled(rect, self.corner_radius, theme.code.surface);
        }
        let largest = self.lines.iter().filter_map(|line| line.number).max();
        let gutter = if self.gutter {
            Self::gutter_width(ui, largest.unwrap_or(1))
        } else {
            0.0
        };
        if self.gutter_rule && visible {
            let x = rect.left() + gutter - (size::CODE_GUTTER - size::CODE_GUTTER_RULE);
            ui.painter().rect_filled(
                Rect::from_min_size(pos2(x, rect.top()), vec2(size::HAIRLINE, rect.height())),
                0,
                theme.code.divider,
            );
        }
        let text_left = rect.left() + if self.gutter { gutter } else { space::S16 };
        let painter = ui.painter_at(rect);
        let mut top = rect.top() + self.padding_top;
        for (index, line) in self.lines.iter().enumerate() {
            let row = Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), row_height));
            result.rows.push(row);
            top += row_height;
            if !ui.is_rect_visible(row) {
                continue;
            }
            if line.highlighted {
                painter.rect_filled(row, 0, theme.code.line_highlight);
            }
            if let Some(number) = line.number.filter(|_| self.gutter) {
                let color = if line.error {
                    theme.code.error
                } else {
                    theme.code.gutter
                };
                let galley =
                    type_scale::MONO.galley(ui.ctx(), &number.to_string(), color, f32::INFINITY);
                let right = row.left() + gutter - size::CODE_GUTTER_PADDING;
                let pos = pos2(right - galley.rect.width(), row.top());
                type_scale::paint_galley(&painter, pos, galley, color);
            }
            let caret = paint_spans(
                ui,
                &painter,
                &SpanStyle::CODE,
                &line.spans,
                line.caret,
                pos2(text_left, row.top()),
                response.id.with(("line", index)),
            );
            if caret.is_some() {
                result.caret = caret;
            }
        }
        result
    }
}

/// The advance of one Plex Mono 13 character.
pub fn char_width(ui: &Ui) -> f32 {
    type_scale::MONO
        .galley(ui.ctx(), "0", Color32::PLACEHOLDER, f32::INFINITY)
        .rect
        .width()
}

/// The inline box of a highlight or caret on a row whose top is `top`.
fn inline_box(top: f32, x: RangeInclusive<f32>) -> Rect {
    Rect::from_x_y_ranges(
        x,
        top + size::CODE_CARET_INSET..=top + size::CODE_CARET_INSET + size::CODE_CARET.y,
    )
}

/// The content area of an inline run on a row whose top is `top` (where CSS
/// paints an inline background): the font's ascent and descent around the
/// baseline, each rounded to device pixels.
fn content_box(ui: &Ui, role: TextRole, top: f32, x: RangeInclusive<f32>) -> Rect {
    let pixels_per_point = ui.ctx().pixels_per_point();
    let metrics = role.face().metrics;
    let round = |extent: f32| (extent * role.size() * pixels_per_point).round() / pixels_per_point;
    let baseline = top + role.css_baseline(pixels_per_point);
    Rect::from_x_y_ranges(
        x,
        baseline - round(metrics.ascent)..=baseline + round(metrics.descent),
    )
}

/// How [`paint_spans`] sets a row of code.
pub(crate) struct SpanStyle {
    /// The role of the text.
    pub role: TextRole,
    /// The corner radius of search match highlights.
    pub match_radius: u8,
}

impl SpanStyle {
    /// Code surface rows: Plex Mono 13/20, matches on 3 point corners.
    pub const CODE: Self = Self {
        role: type_scale::MONO,
        match_radius: radius::CODE_MATCH,
    };
}

/// Paint `spans` as one row at `pos` with their marks, squiggles and a
/// caret before character `caret`, read as one label under `id`; returns
/// the caret's rect.
pub(crate) fn paint_spans(
    ui: &Ui,
    painter: &egui::Painter,
    style: &SpanStyle,
    spans: &[CodeSpan<'_>],
    caret: Option<usize>,
    pos: Pos2,
    id: egui::Id,
) -> Option<Rect> {
    let theme = ui.tokens();
    let theme = &theme;
    let role = style.role;
    let mut job = LayoutJob::default();
    let mut ranges = Vec::with_capacity(spans.len());
    let mut chars = 0;
    for span in spans {
        let color = match span.mark {
            Some(CodeMark::Match) => theme.code.match_text,
            Some(CodeMark::ActiveMatch) => theme.code.match_active_text,
            _ => span.color.resolve(theme),
        };
        let mut format = role.text_format(ui.ctx(), color);
        if let CodeColor::Decorated(decoration) = span.color {
            format.background = decoration.background.unwrap_or(Color32::TRANSPARENT);
            format.italics = decoration.italics;
            if decoration.underline {
                format.underline = egui::Stroke::new(size::HAIRLINE, color);
            }
            if decoration.strikethrough {
                format.strikethrough = egui::Stroke::new(size::HAIRLINE, color);
            }
        }
        job.append(&span.text, 0.0, format);
        let count = span.text.chars().count();
        ranges.push(chars..chars + count);
        chars += count;
    }
    let galley = role.galley_from_job(ui.ctx(), job);
    let x_of = |galley: &Galley, char_index: usize| -> f32 {
        let row = galley.rows.first();
        row.map_or(0.0, |row| {
            row.glyphs
                .get(char_index)
                .map_or(row.rect().right(), |glyph| glyph.pos.x)
        })
    };
    // Marks behind the text, the selection first.
    for wanted in [CodeMark::Selection, CodeMark::Match, CodeMark::ActiveMatch] {
        for (span, range) in spans.iter().zip(&ranges) {
            if span.mark != Some(wanted) || range.is_empty() {
                continue;
            }
            let (fill, corner) = match wanted {
                CodeMark::Selection => (theme.code.selection, radius::CODE_SELECTION),
                CodeMark::Match => (theme.code.match_bg, style.match_radius),
                CodeMark::ActiveMatch => (theme.code.match_active_bg, style.match_radius),
                CodeMark::Underline => continue,
            };
            let x = pos.x + x_of(&galley, range.start)..=pos.x + x_of(&galley, range.end);
            painter.rect_filled(content_box(ui, role, pos.y, x), corner, fill);
        }
    }
    let baseline = pos.y + role.css_baseline(ui.ctx().pixels_per_point());
    for (span, range) in spans.iter().zip(&ranges) {
        if range.is_empty() {
            continue;
        }
        let x = pos.x + x_of(&galley, range.start)..=pos.x + x_of(&galley, range.end);
        if span.squiggle {
            paint_squiggle(painter, x.clone(), baseline, theme.code.error);
        }
        if span.mark == Some(CodeMark::Underline) {
            let underline = Rect::from_x_y_ranges(
                x,
                baseline + size::LINK_UNDERLINE_OFFSET
                    ..=baseline + size::LINK_UNDERLINE_OFFSET + size::LINK_UNDERLINE,
            );
            painter.rect_filled(underline, 0, theme.accent.fill);
        }
    }
    let caret = caret.map(|column| {
        let x = pos.x + x_of(&galley, column);
        inline_box(pos.y, x..=x + size::CODE_CARET.x)
    });
    let text: String = spans.iter().map(|span| span.text.as_ref()).collect();
    let label =
        type_scale::interact_text(ui, Rect::from_min_size(pos, galley.rect.size()), id, &text);
    type_scale::expose_text_runs(ui, &label, pos, &galley);
    type_scale::paint_galley(painter, pos, Arc::clone(&galley), Color32::PLACEHOLDER);
    if let Some(caret) = caret {
        painter.rect_filled(caret, 0, theme.code.caret);
    }
    caret
}

/// A diagnostic's wavy underline across `x`, centred
/// [`size::SQUIGGLE_OFFSET`] below `baseline`, starting at a trough.
pub fn paint_squiggle(
    painter: &egui::Painter,
    x: RangeInclusive<f32>,
    baseline: f32,
    color: Color32,
) {
    let center = baseline + size::SQUIGGLE_OFFSET;
    let (start, end) = (*x.start(), *x.end());
    let step = 0.5;
    let mut points = Vec::new();
    let mut at = start;
    while at <= end {
        let phase = (at - start) / size::SQUIGGLE_WAVELENGTH * std::f32::consts::TAU;
        points.push(pos2(at, center + size::SQUIGGLE_AMPLITUDE * phase.cos()));
        at += step;
    }
    if points.len() >= 2 {
        painter.add(PathShape::line(
            points,
            PathStroke::new(size::HAIRLINE, color),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::setup_egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn the_gutter_grows_past_four_digits() {
        let mut harness = Harness::new_ui(|ui| {
            assert_eq!(CodeSurface::gutter_width(ui, 1), size::CODE_GUTTER);
            assert_eq!(CodeSurface::gutter_width(ui, 9_999), size::CODE_GUTTER);
            let wider = CodeSurface::gutter_width(ui, 10_000);
            assert!((wider - size::CODE_GUTTER - char_width(ui)).abs() < 1e-3);
        });
        setup_egui(&mut harness);
        harness.run();
    }

    #[test]
    fn rows_stack_from_the_top_padding_and_report_the_caret() {
        let mut harness = Harness::new_ui_state(
            |ui, caret: &mut Option<Rect>| {
                let lines = [
                    CodeLine::new([CodeSpan::text("first")]).number(1),
                    CodeLine::new([CodeSpan::text("second")]).number(2).caret(3),
                ];
                let response = CodeSurface::new(&lines).show(ui);
                assert_eq!(response.rows.len(), 2);
                assert_eq!(
                    response.rows[1].top() - response.rect.top(),
                    size::CODE_PADDING_TOP + type_scale::MONO.line_height()
                );
                *caret = response.caret;
            },
            None,
        );
        setup_egui(&mut harness);
        harness.run();
        let line = harness.get_by_label("second").rect();
        let caret = harness.state().expect("the second line has a caret");
        assert_eq!(caret.top(), line.top() + size::CODE_CARET_INSET);
        assert!(caret.left() > line.left());
        assert_eq!(caret.width(), size::CODE_CARET.x);
    }
}
