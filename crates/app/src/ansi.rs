//! ANSI SGR parsing for pod-log display and search.
//!
//! The parser consumes complete log lines. Terminal controls other than SGR
//! are deliberately ignored: the log viewer is not a terminal emulator.

use anstyle::{Ansi256Color, AnsiColor, Color, Effects, RgbColor, Style};
use anstyle_parse::{DefaultCharAccumulator, Params, Parser, Perform};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnsiStyleSpan {
    pub(crate) range: (usize, usize),
    pub(crate) style: Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedLogLine {
    pub(crate) text: String,
    pub(crate) style_spans: Vec<AnsiStyleSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedKubernetesLogLine {
    pub(crate) timestamp: Option<String>,
    pub(crate) line: ParsedLogLine,
}

/// Split the optional timestamp Kubernetes prepends when `timestamps=true`
/// before removing ANSI controls from the log message.
pub(crate) fn parse_kubernetes_log_line(line: &str) -> ParsedKubernetesLogLine {
    if let Some((timestamp, message)) = line.split_once(' ')
        && OffsetDateTime::parse(timestamp, &Rfc3339).is_ok()
    {
        return ParsedKubernetesLogLine {
            timestamp: Some(timestamp.to_owned()),
            line: parse_log_line(message),
        };
    }
    ParsedKubernetesLogLine {
        timestamp: None,
        line: parse_log_line(line),
    }
}

pub(crate) fn parse_log_line(line: &str) -> ParsedLogLine {
    let mut parser = Parser::<DefaultCharAccumulator>::new();
    let mut collector = LogLineCollector::default();
    for &byte in line.as_bytes() {
        parser.advance(&mut collector, byte);
    }
    collector.into_parsed_line()
}

#[derive(Default)]
struct LogLineCollector {
    text: String,
    style: Style,
    style_spans: Vec<AnsiStyleSpan>,
}

impl LogLineCollector {
    fn print(&mut self, character: char) {
        let start = self.text.len();
        self.text.push(character);
        if self.style.is_plain() {
            return;
        }
        let end = self.text.len();
        if let Some(previous) = self.style_spans.last_mut()
            && previous.style == self.style
            && previous.range.1 == start
        {
            previous.range.1 = end;
        } else {
            self.style_spans.push(AnsiStyleSpan {
                range: (start, end),
                style: self.style,
            });
        }
    }

    fn into_parsed_line(self) -> ParsedLogLine {
        ParsedLogLine {
            text: self.text,
            style_spans: self.style_spans,
        }
    }

    fn apply_sgr(&mut self, params: &Params) {
        let params = params
            .iter()
            .map(|param| param.to_vec())
            .collect::<Vec<_>>();
        let mut index = 0;
        while index < params.len() {
            let parameter = &params[index];
            let code = parameter.first().copied().unwrap_or(0);
            match code {
                0 => self.style = Style::new(),
                1 => self.set_effect(Effects::BOLD, true),
                2 => self.set_effect(Effects::DIMMED, true),
                3 => self.set_effect(Effects::ITALIC, true),
                4 => self.apply_underline(parameter),
                5 | 6 => self.set_effect(Effects::BLINK, true),
                7 => self.set_effect(Effects::INVERT, true),
                8 => self.set_effect(Effects::HIDDEN, true),
                9 => self.set_effect(Effects::STRIKETHROUGH, true),
                21 => {
                    self.set_effect(Effects::BOLD, false);
                    self.set_effect(Effects::DOUBLE_UNDERLINE, true);
                }
                22 => {
                    self.set_effect(Effects::BOLD, false);
                    self.set_effect(Effects::DIMMED, false);
                }
                23 => self.set_effect(Effects::ITALIC, false),
                24 => self.clear_underline(),
                25 => self.set_effect(Effects::BLINK, false),
                27 => self.set_effect(Effects::INVERT, false),
                28 => self.set_effect(Effects::HIDDEN, false),
                29 => self.set_effect(Effects::STRIKETHROUGH, false),
                30..=37 | 90..=97 => self.style = self.style.fg_color(Some(ansi_color(code))),
                38 => index += self.apply_extended_color(&params, index, true),
                39 => self.style = self.style.fg_color(None),
                40..=47 | 100..=107 => self.style = self.style.bg_color(Some(ansi_color(code))),
                48 => index += self.apply_extended_color(&params, index, false),
                49 => self.style = self.style.bg_color(None),
                58 => index += self.apply_extended_underline_color(&params, index),
                59 => self.style = self.style.underline_color(None),
                _ => {}
            }
            index += 1;
        }
    }

    fn set_effect(&mut self, effect: Effects, enabled: bool) {
        self.style = self
            .style
            .effects(self.style.get_effects().set(effect, enabled));
    }

    fn apply_underline(&mut self, parameter: &[u16]) {
        self.clear_underline();
        let effect = match parameter.get(1).copied().unwrap_or(1) {
            0 => return,
            1 => Effects::UNDERLINE,
            2 => Effects::DOUBLE_UNDERLINE,
            3 => Effects::CURLY_UNDERLINE,
            4 => Effects::DOTTED_UNDERLINE,
            5 => Effects::DASHED_UNDERLINE,
            _ => Effects::UNDERLINE,
        };
        self.set_effect(effect, true);
    }

    fn clear_underline(&mut self) {
        self.style = self.style.effects(self.style.get_effects().remove(
            Effects::UNDERLINE
                | Effects::DOUBLE_UNDERLINE
                | Effects::CURLY_UNDERLINE
                | Effects::DOTTED_UNDERLINE
                | Effects::DASHED_UNDERLINE,
        ));
    }

    fn apply_extended_color(
        &mut self,
        params: &[Vec<u16>],
        index: usize,
        foreground: bool,
    ) -> usize {
        let (color, consumed) = extended_color(params, index);
        if let Some(color) = color {
            self.style = if foreground {
                self.style.fg_color(Some(color))
            } else {
                self.style.bg_color(Some(color))
            };
        }
        consumed
    }

    fn apply_extended_underline_color(&mut self, params: &[Vec<u16>], index: usize) -> usize {
        let (color, consumed) = extended_color(params, index);
        if let Some(color) = color {
            self.style = self.style.underline_color(Some(color));
        }
        consumed
    }
}

impl Perform for LogLineCollector {
    fn print(&mut self, character: char) {
        self.print(character);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\t' {
            self.print('\t');
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8) {
        if !ignore && intermediates.is_empty() && action == b'm' {
            self.apply_sgr(params);
        }
    }
}

fn extended_color(params: &[Vec<u16>], index: usize) -> (Option<Color>, usize) {
    let parameter = &params[index];
    if parameter.len() >= 3 {
        return color_from_extended_parts(parameter).map_or((None, 0), |color| (Some(color), 0));
    }
    let Some(mode) = params
        .get(index + 1)
        .and_then(|parameter| parameter.first())
        .copied()
    else {
        return (None, 0);
    };
    match mode {
        5 => (
            params
                .get(index + 2)
                .and_then(|parameter| parameter.first())
                .and_then(|value| u8::try_from(*value).ok())
                .map(|value| Color::Ansi256(Ansi256Color(value))),
            2,
        ),
        2 => (
            match (
                params
                    .get(index + 2)
                    .and_then(|parameter| parameter.first())
                    .and_then(|value| u8::try_from(*value).ok()),
                params
                    .get(index + 3)
                    .and_then(|parameter| parameter.first())
                    .and_then(|value| u8::try_from(*value).ok()),
                params
                    .get(index + 4)
                    .and_then(|parameter| parameter.first())
                    .and_then(|value| u8::try_from(*value).ok()),
            ) {
                (Some(red), Some(green), Some(blue)) => {
                    Some(Color::Rgb(RgbColor(red, green, blue)))
                }
                _ => None,
            },
            4,
        ),
        _ => (None, 0),
    }
}

fn color_from_extended_parts(parts: &[u16]) -> Option<Color> {
    match parts {
        [_, 5, value] => u8::try_from(*value)
            .ok()
            .map(|value| Color::Ansi256(Ansi256Color(value))),
        [_, 2, red, green, blue] => Some(Color::Rgb(RgbColor(
            u8::try_from(*red).ok()?,
            u8::try_from(*green).ok()?,
            u8::try_from(*blue).ok()?,
        ))),
        [_, 2, _, red, green, blue] => Some(Color::Rgb(RgbColor(
            u8::try_from(*red).ok()?,
            u8::try_from(*green).ok()?,
            u8::try_from(*blue).ok()?,
        ))),
        _ => None,
    }
}

fn ansi_color(code: u16) -> Color {
    let color = match code {
        30 | 40 => AnsiColor::Black,
        31 | 41 => AnsiColor::Red,
        32 | 42 => AnsiColor::Green,
        33 | 43 => AnsiColor::Yellow,
        34 | 44 => AnsiColor::Blue,
        35 | 45 => AnsiColor::Magenta,
        36 | 46 => AnsiColor::Cyan,
        37 | 47 => AnsiColor::White,
        90 | 100 => AnsiColor::BrightBlack,
        91 | 101 => AnsiColor::BrightRed,
        92 | 102 => AnsiColor::BrightGreen,
        93 | 103 => AnsiColor::BrightYellow,
        94 | 104 => AnsiColor::BrightBlue,
        95 | 105 => AnsiColor::BrightMagenta,
        96 | 106 => AnsiColor::BrightCyan,
        97 | 107 => AnsiColor::BrightWhite,
        _ => unreachable!("ANSI color code must be in a supported range"),
    };
    color.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_visible_text_and_sgr_style_spans() {
        let parsed = parse_log_line("plain \u{1b}[1;38;5;196merror\u{1b}[0m done");

        assert_eq!(parsed.text, "plain error done");
        assert_eq!(parsed.style_spans.len(), 1);
        assert_eq!(parsed.style_spans[0].range, (6, 11));
        assert_eq!(
            parsed.style_spans[0].style,
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi256(Ansi256Color(196))))
        );
    }

    #[test]
    fn supports_colon_form_true_color_and_individual_resets() {
        let parsed = parse_log_line("\u{1b}[3;4;38:2::12:34:56mstyled\u{1b}[23;24m plain");

        assert_eq!(parsed.text, "styled plain");
        assert_eq!(parsed.style_spans.len(), 2);
        assert_eq!(parsed.style_spans[0].range, (0, 6));
        assert_eq!(
            parsed.style_spans[0].style,
            Style::new()
                .italic()
                .underline()
                .fg_color(Some(Color::Rgb(RgbColor(12, 34, 56))))
        );
        assert_eq!(parsed.style_spans[1].range, (6, 12));
        assert_eq!(
            parsed.style_spans[1].style,
            Style::new().fg_color(Some(Color::Rgb(RgbColor(12, 34, 56))))
        );
    }

    #[test]
    fn ignores_non_sgr_and_incomplete_escape_sequences() {
        let parsed = parse_log_line("before\u{1b}]0;title\u{7}after\u{1b}[2J still\u{1b}[");

        assert_eq!(parsed.text, "beforeafter still");
        assert!(parsed.style_spans.is_empty());
    }

    #[test]
    fn separates_kubernetes_timestamp_before_parsing_ansi() {
        let parsed = parse_kubernetes_log_line(
            "2026-08-08T15:22:17.143Z \u{1b}[33mWARN\u{1b}[0m cache is stale",
        );

        assert_eq!(
            parsed.timestamp.as_deref(),
            Some("2026-08-08T15:22:17.143Z")
        );
        assert_eq!(parsed.line.text, "WARN cache is stale");
        assert_eq!(parsed.line.style_spans.len(), 1);
    }

    #[test]
    fn leaves_non_kubernetes_timestamp_lines_unchanged() {
        let parsed = parse_kubernetes_log_line("not-a-timestamp message");

        assert_eq!(parsed.timestamp, None);
        assert_eq!(parsed.line.text, "not-a-timestamp message");
    }
}
