//! A small YAML tokenizer that colours manifests with the theme's `syntax-*`
//! roles.
//!
//! It is a line-oriented highlighter, not a parser: it recognises the
//! constructs that appear in Kubernetes manifests (mapping keys, sequence
//! entries, scalars, flow collections, comments, document markers and block
//! scalars) and degrades to plain text for anything it does not understand.
//! Constructs that span lines (block scalars, multi-line plain and quoted
//! scalars, flow collections) carry over to the following lines until they
//! end, a non-blank line dedents below them, or a document marker starts.
//! Every byte of the input is covered by exactly one token, in order.

use std::ops::Range;

use components::design::theme::Theme;

/// The highlighting class of a run of YAML source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum YamlToken {
    /// Whitespace, block scalar content and anything unclassified.
    Plain,
    /// A mapping key.
    Key,
    /// Plain or quoted scalar text, and block scalar indicators.
    String,
    /// An integer or floating-point scalar.
    Number,
    /// A boolean or null scalar.
    Bool,
    /// Structural indicators: `:`, `-`, `,`, brackets, document markers,
    /// anchors, aliases and tags.
    Punctuation,
    /// A `#` comment.
    Comment,
}

impl YamlToken {
    fn color(self, theme: &Theme) -> egui::Color32 {
        match self {
            Self::Plain => theme.code.text,
            Self::Key => theme.syntax.key,
            Self::String => theme.syntax.string,
            Self::Number => theme.syntax.number,
            Self::Bool => theme.syntax.bool,
            Self::Punctuation => theme.syntax.punctuation,
            Self::Comment => theme.syntax.comment,
        }
    }
}

/// A highlighted layout job for `yaml`, coloured from `theme`.
pub(super) fn yaml_layout_job(
    yaml: &str,
    font_id: &egui::FontId,
    theme: &Theme,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob {
        text: yaml.to_owned(),
        ..Default::default()
    };
    for (range, token) in tokenize_yaml(yaml) {
        job.sections.push(egui::text::LayoutSection {
            leading_space: 0.0,
            byte_range: egui::text::ByteIndex(range.start)..egui::text::ByteIndex(range.end),
            format: egui::TextFormat {
                line_height: Some(components::design::type_scale::MONO.line_height()),
                ..egui::TextFormat::simple(font_id.clone(), token.color(theme))
            },
        });
    }
    job
}

/// Split `text` into contiguous highlighting runs. Adjacent runs of the same
/// class are merged.
pub(crate) fn tokenize_yaml(text: &str) -> Vec<(Range<usize>, YamlToken)> {
    // A byte order mark may precede the stream; it is not part of the first
    // line's content.
    let first_line_start = if text.starts_with('\u{FEFF}') {
        '\u{FEFF}'.len_utf8()
    } else {
        0
    };
    let mut tokenizer = Tokenizer {
        text,
        tokens: Vec::new(),
        first_line_start,
        line_start: first_line_start,
        open: None,
        flow: None,
    };
    tokenizer.push(0..first_line_start, YamlToken::Plain);
    let mut line_start = first_line_start;
    while line_start < text.len() {
        let line_end = text[line_start..]
            .find('\n')
            .map_or(text.len(), |offset| line_start + offset);
        // The carriage return of a CRLF line ending stays plain, like the
        // newline itself.
        let content_end = if text[line_start..line_end].ends_with('\r') {
            line_end - 1
        } else {
            line_end
        };
        tokenizer.line_start = line_start;
        tokenizer.line(line_start, content_end);
        tokenizer.push(content_end..line_end, YamlToken::Plain);
        if line_end < text.len() {
            tokenizer.push(line_end..line_end + 1, YamlToken::Plain);
        }
        line_start = line_end + 1;
    }
    debug_assert!(
        tokenizer
            .tokens
            .last()
            .is_none_or(|(range, _)| range.end == text.len()),
        "tokens cover the whole text"
    );
    tokenizer.tokens
}

struct Tokenizer<'a> {
    text: &'a str,
    tokens: Vec<(Range<usize>, YamlToken)>,
    /// Where the first line starts: after a leading byte order mark.
    first_line_start: usize,
    /// Where the line being tokenized starts.
    line_start: usize,
    /// A multi-line scalar left open by an earlier line.
    open: Option<OpenScalar>,
    /// A `[`/`{` collection left open by an earlier line.
    flow: Option<Flow>,
}

/// A scalar that continues on the following lines while they are blank or
/// indented by at least `min_indent`.
#[derive(Clone, Copy)]
struct OpenScalar {
    kind: OpenScalarKind,
    min_indent: usize,
}

#[derive(Clone, Copy)]
enum OpenScalarKind {
    /// `|` or `>` block scalar content.
    Block,
    /// A block-context plain scalar, which more-indented lines without a
    /// mapping key continue.
    Plain,
    /// A single- or double-quoted scalar whose closing quote is still to
    /// come, in block or flow context.
    Quoted(u8),
}

#[derive(Clone, Copy)]
struct Flow {
    /// Nesting depth of the open brackets.
    depth: usize,
    /// The indentation continuation lines must reach; a non-blank line
    /// dedented below it ends the collection.
    min_indent: usize,
}

const FLOW_INDICATORS: &[u8] = b"[]{},";

fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

impl<'a> Tokenizer<'a> {
    fn push(&mut self, range: Range<usize>, token: YamlToken) {
        if range.is_empty() {
            return;
        }
        debug_assert_eq!(
            self.tokens.last().map_or(0, |(last, _)| last.end),
            range.start,
            "tokens are contiguous"
        );
        match self.tokens.last_mut() {
            Some((last, last_token)) if *last_token == token => last.end = range.end,
            _ => self.tokens.push((range, token)),
        }
    }

    fn bytes(&self) -> &'a [u8] {
        self.text.as_bytes()
    }

    /// The end of the run of bytes from `start` (before `end`) that all
    /// satisfy `include`.
    fn run_end(&self, start: usize, end: usize, include: impl Fn(u8) -> bool) -> usize {
        start
            + self.bytes()[start..end]
                .iter()
                .take_while(|&&byte| include(byte))
                .count()
    }

    /// The end of the spaces and tabs from `start`.
    fn blank_end(&self, start: usize, end: usize) -> usize {
        self.run_end(start, end, is_blank)
    }

    /// The end of the word from `start`: up to a blank or one of `stops`.
    fn word_end(&self, start: usize, end: usize, stops: &[u8]) -> usize {
        self.run_end(start, end, |byte| !is_blank(byte) && !stops.contains(&byte))
    }

    fn skip_whitespace(&mut self, start: usize, end: usize) -> usize {
        let next = self.blank_end(start, end);
        self.push(start..next, YamlToken::Plain);
        next
    }

    fn is_blank_or_end(&self, index: usize, end: usize) -> bool {
        index >= end || is_blank(self.bytes()[index])
    }

    fn line(&mut self, start: usize, end: usize) {
        let content = self.blank_end(start, end);
        let indent = content - start;

        if indent == 0 && self.is_document_marker(content, end) {
            // A document marker ends everything the previous document left
            // open, even an unclosed quote or bracket.
            self.open = None;
            self.flow = None;
            self.push(content..content + 3, YamlToken::Punctuation);
            // The continuation lines of a top-level node (such as the
            // content of `--- |`) may start at column 0.
            self.value(content + 3, end, 0);
            return;
        }
        if content < end {
            // A non-blank line dedented below an open construct ends it, so
            // an unclosed quote or bracket being typed does not recolour the
            // rest of the buffer.
            if self.open.is_some_and(|open| indent < open.min_indent) {
                self.open = None;
            }
            // A closing bracket may still sit at the owner's column, as in
            // `key: [` … `]`.
            if self.flow.is_some_and(|flow| indent < flow.min_indent)
                && !matches!(self.bytes()[content], b']' | b'}')
            {
                self.flow = None;
            }
        }

        if let Some(open) = self.open {
            match open.kind {
                OpenScalarKind::Block => {
                    self.push(start..end, YamlToken::Plain);
                    return;
                }
                OpenScalarKind::Quoted(quote) => {
                    let position = self.skip_whitespace(start, end);
                    let Some(quoted_end) = self.closing_quote(quote, position, end) else {
                        self.push(position..end, YamlToken::String);
                        return;
                    };
                    self.push(position..quoted_end, YamlToken::String);
                    self.open = None;
                    let after = if self.flow.is_some() {
                        self.flow(quoted_end, end)
                    } else {
                        quoted_end
                    };
                    self.trailing(after, end);
                    return;
                }
                OpenScalarKind::Plain => {
                    if content == end {
                        self.push(start..end, YamlToken::Plain);
                        return;
                    }
                    self.open = None;
                    // A comment line ends the scalar, and a mapping key
                    // starts a new node instead of continuing it.
                    if !self.is_comment_start(content) && self.mapping_key(content, end).is_none() {
                        let position = self.skip_whitespace(start, end);
                        let scalar_end = self.plain_scalar_end(position, end, open.min_indent);
                        self.push(position..scalar_end, YamlToken::String);
                        self.trailing(scalar_end, end);
                        return;
                    }
                }
            }
        }

        let position = self.skip_whitespace(start, end);
        if self.flow.is_some() {
            let after = self.flow(position, end);
            self.trailing(after, end);
            return;
        }
        self.node(position, end, indent);
    }

    fn is_document_marker(&self, position: usize, end: usize) -> bool {
        let rest = &self.text[position..end];
        (rest.starts_with("---") || rest.starts_with("..."))
            && self.is_blank_or_end(position + 3, end)
    }

    /// A block node: optional sequence indicators, then a `key: value` pair
    /// or a bare value. `indent` is the line's indentation.
    fn node(&mut self, mut position: usize, end: usize, indent: usize) {
        let mut min_indent = indent + 1;
        while position < end
            && self.bytes()[position] == b'-'
            && self.is_blank_or_end(position + 1, end)
        {
            min_indent = position - self.line_start + 1;
            self.push(position..position + 1, YamlToken::Punctuation);
            position = self.skip_whitespace(position + 1, end);
        }
        if position >= end {
            return;
        }
        if let Some((key_end, colon)) = self.mapping_key(position, end) {
            self.push(position..key_end, YamlToken::Key);
            self.push(key_end..colon, YamlToken::Plain);
            self.push(colon..colon + 1, YamlToken::Punctuation);
            self.value(colon + 1, end, position - self.line_start + 1);
        } else {
            self.value(position, end, min_indent);
        }
    }

    /// The end of a block mapping key starting at `position` and the index
    /// of its `:` indicator.
    fn mapping_key(&self, position: usize, end: usize) -> Option<(usize, usize)> {
        let bytes = self.bytes();
        let key_end = match bytes[position] {
            quote @ (b'"' | b'\'') => self.closing_quote(quote, position + 1, end)?,
            b'#' | b'[' | b']' | b'{' | b'}' | b',' | b'&' | b'*' | b'!' | b'|' | b'>' | b'%'
            | b'@' | b'`' => return None,
            _ => {
                let colon = (position..end)
                    .find(|&index| bytes[index] == b':' && self.is_blank_or_end(index + 1, end))?;
                if self.comment_start(position, colon).is_some() {
                    return None;
                }
                return Some((self.trim_end(position, colon), colon));
            }
        };
        let colon = self.blank_end(key_end, end);
        (colon < end && bytes[colon] == b':' && self.is_blank_or_end(colon + 1, end))
            .then_some((key_end, colon))
    }

    /// Whether the byte at `index` is a `#` that starts a comment: it starts
    /// a line or follows whitespace.
    fn is_comment_start(&self, index: usize) -> bool {
        let bytes = self.bytes();
        bytes[index] == b'#'
            && (index == self.first_line_start || matches!(bytes[index - 1], b' ' | b'\t' | b'\n'))
    }

    /// The first `#` in `start..end` that starts a comment.
    fn comment_start(&self, start: usize, end: usize) -> Option<usize> {
        (start..end).find(|&index| self.is_comment_start(index))
    }

    fn trim_end(&self, start: usize, end: usize) -> usize {
        start + self.text[start..end].trim_end_matches([' ', '\t']).len()
    }

    /// The index just past the `quote` that closes a quoted scalar whose
    /// text continues at `from`, or `None` when it is still open at `end`.
    fn closing_quote(&self, quote: u8, from: usize, end: usize) -> Option<usize> {
        let bytes = self.bytes();
        let mut index = from;
        while index < end {
            match bytes[index] {
                // A backslash at the end of the line escapes the line break,
                // so the scalar stays open.
                b'\\' if quote == b'"' => index += 2,
                b'\'' if quote == b'\'' && index + 1 < end && bytes[index + 1] == b'\'' => {
                    index += 2
                }
                byte if byte == quote => return Some(index + 1),
                _ => index += 1,
            }
        }
        None
    }

    /// The end of a block plain scalar from `position`: a comment or the end
    /// of the line. Without a comment the scalar stays open for continuation
    /// lines indented by at least `min_indent`.
    fn plain_scalar_end(&mut self, position: usize, end: usize, min_indent: usize) -> usize {
        let comment = self.comment_start(position, end);
        if comment.is_none() {
            self.open = Some(OpenScalar {
                kind: OpenScalarKind::Plain,
                min_indent,
            });
        }
        self.trim_end(position, comment.unwrap_or(end))
    }

    /// A value in block context. Its continuation lines (block scalar
    /// content, multi-line scalars and flow collections) must be indented by
    /// at least `min_indent`.
    fn value(&mut self, position: usize, end: usize, min_indent: usize) {
        let mut position = self.skip_whitespace(position, end);
        while position < end {
            match self.bytes()[position] {
                b'#' => break,
                b'&' | b'*' | b'!' => {
                    let token_end = self.word_end(position, end, &[]);
                    self.push(position..token_end, YamlToken::Punctuation);
                    position = self.skip_whitespace(token_end, end);
                }
                b'|' | b'>' => {
                    let indicator_end = self.run_end(position + 1, end, |byte| {
                        matches!(byte, b'0'..=b'9' | b'+' | b'-')
                    });
                    self.push(position..indicator_end, YamlToken::String);
                    self.open = Some(OpenScalar {
                        kind: OpenScalarKind::Block,
                        min_indent,
                    });
                    position = indicator_end;
                    break;
                }
                quote @ (b'"' | b'\'') => {
                    let closing = self.closing_quote(quote, position + 1, end);
                    if closing.is_none() {
                        self.open = Some(OpenScalar {
                            kind: OpenScalarKind::Quoted(quote),
                            min_indent,
                        });
                    }
                    let quoted_end = closing.unwrap_or(end);
                    self.push(position..quoted_end, YamlToken::String);
                    position = quoted_end;
                    break;
                }
                b'[' | b'{' => {
                    self.flow = Some(Flow {
                        depth: 0,
                        min_indent,
                    });
                    position = self.flow(position, end);
                    break;
                }
                _ => {
                    let scalar_end = self.plain_scalar_end(position, end, min_indent);
                    self.scalar(position, scalar_end);
                    position = scalar_end;
                    break;
                }
            }
        }
        self.trailing(position, end);
    }

    /// Whitespace and an optional comment up to the end of the line. Text
    /// that is neither is left plain.
    fn trailing(&mut self, position: usize, end: usize) {
        let position = self.skip_whitespace(position, end);
        if position >= end {
            return;
        }
        match self.comment_start(position, end) {
            Some(comment) => {
                self.push(position..comment, YamlToken::Plain);
                self.push(comment..end, YamlToken::Comment);
            }
            None => self.push(position..end, YamlToken::Plain),
        }
    }

    /// Content of the open flow collection from `position`, tracking nesting
    /// across lines. Returns where the collection (or the line) ends.
    fn flow(&mut self, mut position: usize, end: usize) -> usize {
        let bytes = self.bytes();
        while position < end {
            match bytes[position] {
                byte if is_blank(byte) => position = self.skip_whitespace(position, end),
                b'#' if self.is_comment_start(position) => return position,
                b'[' | b'{' => {
                    if let Some(flow) = &mut self.flow {
                        flow.depth += 1;
                    }
                    self.push(position..position + 1, YamlToken::Punctuation);
                    position += 1;
                }
                b']' | b'}' => {
                    self.push(position..position + 1, YamlToken::Punctuation);
                    position += 1;
                    let closed = self.flow.as_mut().is_none_or(|flow| {
                        flow.depth = flow.depth.saturating_sub(1);
                        flow.depth == 0
                    });
                    if closed {
                        self.flow = None;
                        return position;
                    }
                }
                b',' => {
                    self.push(position..position + 1, YamlToken::Punctuation);
                    position += 1;
                }
                b':' if self.is_flow_separator(position + 1, end) => {
                    self.push(position..position + 1, YamlToken::Punctuation);
                    position += 1;
                }
                b'&' | b'*' | b'!' => {
                    let token_end = self.word_end(position, end, FLOW_INDICATORS);
                    self.push(position..token_end, YamlToken::Punctuation);
                    position = token_end;
                }
                quote @ (b'"' | b'\'') => {
                    let Some(quoted_end) = self.closing_quote(quote, position + 1, end) else {
                        self.push(position..end, YamlToken::String);
                        self.open = Some(OpenScalar {
                            kind: OpenScalarKind::Quoted(quote),
                            min_indent: self.flow.map_or(0, |flow| flow.min_indent),
                        });
                        return end;
                    };
                    // A quoted (JSON-like) key may be followed directly by
                    // its `:`, as in `{"a":1}`.
                    let colon = self.blank_end(quoted_end, end);
                    if colon < end && bytes[colon] == b':' {
                        self.push(position..quoted_end, YamlToken::Key);
                        self.push(quoted_end..colon, YamlToken::Plain);
                        self.push(colon..colon + 1, YamlToken::Punctuation);
                        position = colon + 1;
                    } else {
                        self.push(position..quoted_end, YamlToken::String);
                        position = quoted_end;
                    }
                }
                _ => {
                    let scalar_end = (position..end)
                        .find(|&index| {
                            FLOW_INDICATORS.contains(&bytes[index])
                                || (bytes[index] == b':' && self.is_flow_separator(index + 1, end))
                                || self.is_comment_start(index)
                        })
                        .unwrap_or(end);
                    let trimmed_end = self.trim_end(position, scalar_end);
                    if self.flow_key_follows(trimmed_end, end) {
                        self.push(position..trimmed_end, YamlToken::Key);
                    } else {
                        self.scalar(position, trimmed_end);
                    }
                    position = trimmed_end;
                }
            }
        }
        position
    }

    fn is_flow_separator(&self, index: usize, end: usize) -> bool {
        self.is_blank_or_end(index, end) || FLOW_INDICATORS.contains(&self.bytes()[index])
    }

    /// Whether a `:` mapping indicator follows `index`, after whitespace.
    fn flow_key_follows(&self, index: usize, end: usize) -> bool {
        let colon = self.blank_end(index, end);
        colon < end && self.bytes()[colon] == b':' && self.is_flow_separator(colon + 1, end)
    }

    /// A plain scalar, classified by its resolved YAML 1.2 core type.
    fn scalar(&mut self, start: usize, end: usize) {
        self.push(start..end, scalar_token(&self.text[start..end]));
    }
}

/// The class of a plain scalar under the YAML 1.2 core schema.
fn scalar_token(scalar: &str) -> YamlToken {
    match scalar {
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" | "null" | "Null" | "NULL" | "~" => {
            YamlToken::Bool
        }
        _ if is_number(scalar) => YamlToken::Number,
        _ => YamlToken::String,
    }
}

fn is_number(scalar: &str) -> bool {
    if let Some(digits) = scalar.strip_prefix("0x") {
        return !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_hexdigit());
    }
    if let Some(digits) = scalar.strip_prefix("0o") {
        return !digits.is_empty() && digits.bytes().all(|byte| matches!(byte, b'0'..=b'7'));
    }
    let unsigned = scalar.strip_prefix(['-', '+']).unwrap_or(scalar);
    if matches!(unsigned, ".inf" | ".Inf" | ".INF") || matches!(scalar, ".nan" | ".NaN" | ".NAN") {
        return true;
    }
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(index) => (&unsigned[..index], Some(&unsigned[index + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    let all_digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    let mantissa_valid = all_digits(whole)
        && fraction.is_none_or(all_digits)
        && (!whole.is_empty() || fraction.is_some_and(|fraction| !fraction.is_empty()));
    let exponent_valid = exponent.is_none_or(|exponent| {
        let digits = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
        !digits.is_empty() && all_digits(digits)
    });
    mantissa_valid && exponent_valid
}

#[cfg(test)]
mod tests {
    use super::YamlToken::{self, Bool, Comment, Key, Number, Plain, Punctuation, String};
    use super::*;

    /// The non-plain tokens of `text`, as `(source text, class)` pairs.
    fn classes(text: &str) -> Vec<(&str, YamlToken)> {
        tokenize_yaml(text)
            .into_iter()
            .filter(|(_, token)| *token != Plain)
            .map(|(range, token)| (&text[range], token))
            .collect()
    }

    /// The class of the token covering byte `index` of `text`.
    fn token_at(text: &str, index: usize) -> YamlToken {
        tokenize_yaml(text)
            .into_iter()
            .find(|(range, _)| range.contains(&index))
            .map(|(_, token)| token)
            .expect("every byte is covered")
    }

    /// Asserts that the tokens of `text` are non-empty, contiguous and cover
    /// it in order, start and end on char boundaries, and that adjacent
    /// tokens of one class are merged.
    fn assert_covers(text: &str) {
        let tokens = tokenize_yaml(text);
        let mut expected_start = 0;
        for (range, _) in &tokens {
            assert_eq!(
                range.start, expected_start,
                "tokens are contiguous in {text:?}"
            );
            assert!(range.start < range.end, "tokens are non-empty in {text:?}");
            assert!(
                text.is_char_boundary(range.start) && text.is_char_boundary(range.end),
                "token {range:?} splits a char in {text:?}"
            );
            expected_start = range.end;
        }
        assert_eq!(expected_start, text.len(), "tokens cover {text:?}");
        assert!(
            tokens.windows(2).all(|pair| pair[0].1 != pair[1].1),
            "adjacent tokens of one class are merged in {text:?}"
        );
    }

    #[test]
    fn keys_values_and_indicators_follow_the_design_classes() {
        let yaml = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings\n  labels:\n    app.kubernetes.io/name: settings\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("apiVersion", Key),
                (":", Punctuation),
                ("v1", String),
                ("kind", Key),
                (":", Punctuation),
                ("ConfigMap", String),
                ("metadata", Key),
                (":", Punctuation),
                ("name", Key),
                (":", Punctuation),
                ("settings", String),
                ("labels", Key),
                (":", Punctuation),
                ("app.kubernetes.io/name", Key),
                (":", Punctuation),
                ("settings", String),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn scalars_resolve_numbers_bools_and_nulls() {
        let yaml = "replicas: 3\nratio: -1.5e3\nport: 0x1F\nenabled: true\nmissing: null\ntilde: ~\nversion: 1.2.3\nmode: off\nhex: 0x\n";
        assert_eq!(
            classes(yaml)
                .into_iter()
                .filter(|(_, token)| !matches!(token, Key | Punctuation))
                .collect::<Vec<_>>(),
            vec![
                ("3", Number),
                ("-1.5e3", Number),
                ("0x1F", Number),
                ("true", Bool),
                ("null", Bool),
                ("~", Bool),
                ("1.2.3", String),
                ("off", String),
                ("0x", String),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn quoted_strings_keep_escapes_and_colons_inside_the_string() {
        let yaml = "a: \"say \\\"hi\\\": now\" # note\n'quoted key': 'it''s: fine'\nurl: http://example.com:8080/x\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("a", Key),
                (":", Punctuation),
                ("\"say \\\"hi\\\": now\"", String),
                ("# note", Comment),
                ("'quoted key'", Key),
                (":", Punctuation),
                ("'it''s: fine'", String),
                ("url", Key),
                (":", Punctuation),
                ("http://example.com:8080/x", String),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn comments_cover_whole_lines_and_line_ends_but_not_hashes_in_values() {
        let yaml = "# header\nimage: nginx#latest  # pinned\n  # indented\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("# header", Comment),
                ("image", Key),
                (":", Punctuation),
                ("nginx#latest", String),
                ("# pinned", Comment),
                ("# indented", Comment),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn sequences_mark_dashes_and_nested_entries() {
        let yaml = "containers:\n- name: app\n  args:\n    - --verbose\n    - - nested\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("containers", Key),
                (":", Punctuation),
                ("-", Punctuation),
                ("name", Key),
                (":", Punctuation),
                ("app", String),
                ("args", Key),
                (":", Punctuation),
                ("-", Punctuation),
                ("--verbose", String),
                ("-", Punctuation),
                ("-", Punctuation),
                ("nested", String),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn document_markers_separate_multiple_documents() {
        let yaml = "kind: A\n---\nkind: B\n--- # second\n...\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("kind", Key),
                (":", Punctuation),
                ("A", String),
                ("---", Punctuation),
                ("kind", Key),
                (":", Punctuation),
                ("B", String),
                ("---", Punctuation),
                ("# second", Comment),
                ("...", Punctuation),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn block_scalar_content_stays_plain_until_indentation_returns() {
        let yaml = "data:\n  script: |-\n    echo key: value # not a comment\n\n    - not a sequence\n  next: 1\nitems:\n- >\n  folded: text\n- 2\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("data", Key),
                (":", Punctuation),
                ("script", Key),
                (":", Punctuation),
                ("|-", String),
                ("next", Key),
                (":", Punctuation),
                ("1", Number),
                ("items", Key),
                (":", Punctuation),
                ("-", Punctuation),
                (">", String),
                ("-", Punctuation),
                ("2", Number),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn top_level_block_scalar_after_document_marker() {
        // The content of a top-level block scalar may start at column 0 and
        // runs until the next document marker.
        let yaml = "--- |\nline: one # not a comment\n- not a sequence\n\n  indented\n---\nkind: A\n--- >-\ntext\n...\nkind: B\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("---", Punctuation),
                ("|", String),
                ("---", Punctuation),
                ("kind", Key),
                (":", Punctuation),
                ("A", String),
                ("---", Punctuation),
                (">-", String),
                ("...", Punctuation),
                ("kind", Key),
                (":", Punctuation),
                ("B", String),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn flow_collections_mark_brackets_keys_and_scalars_across_lines() {
        let yaml = "ports: [80, \"https\", true]\nselector: {app: web, tier: \"front\"}\nargs: [a,\n  b] # done\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("ports", Key),
                (":", Punctuation),
                ("[", Punctuation),
                ("80", Number),
                (",", Punctuation),
                ("\"https\"", String),
                (",", Punctuation),
                ("true", Bool),
                ("]", Punctuation),
                ("selector", Key),
                (":", Punctuation),
                ("{", Punctuation),
                ("app", Key),
                (":", Punctuation),
                ("web", String),
                (",", Punctuation),
                ("tier", Key),
                (":", Punctuation),
                ("\"front\"", String),
                ("}", Punctuation),
                ("args", Key),
                (":", Punctuation),
                ("[", Punctuation),
                ("a", String),
                (",", Punctuation),
                ("b", String),
                ("]", Punctuation),
                ("# done", Comment),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn json_like_flow_keys_take_an_adjacent_colon() {
        let yaml = "{\"a\":1,\"b\" :[\"c\"]}\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("{", Punctuation),
                ("\"a\"", Key),
                (":", Punctuation),
                ("1", Number),
                (",", Punctuation),
                ("\"b\"", Key),
                (":[", Punctuation),
                ("\"c\"", String),
                ("]}", Punctuation),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn unclosed_flow_collection_does_not_swallow_the_next_document() {
        let yaml = "ports: [80,\n---\nkind: A\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("ports", Key),
                (":", Punctuation),
                ("[", Punctuation),
                ("80", Number),
                (",", Punctuation),
                ("---", Punctuation),
                ("kind", Key),
                (":", Punctuation),
                ("A", String),
            ]
        );
        assert_covers(yaml);

        // A dedent to the indentation of the key that opened it ends it too.
        let yaml = "spec:\n  ports: {a: [1,\n    2,\n  next: x\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("spec", Key),
                (":", Punctuation),
                ("ports", Key),
                (":", Punctuation),
                ("{", Punctuation),
                ("a", Key),
                (":", Punctuation),
                ("[", Punctuation),
                ("1", Number),
                (",", Punctuation),
                ("2", Number),
                (",", Punctuation),
                ("next", Key),
                (":", Punctuation),
                ("x", String),
            ]
        );
        assert_covers(yaml);

        // A closing bracket at the owner's column still closes it.
        let yaml = "args: [\n  a,\n]\nnext: 1\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("args", Key),
                (":", Punctuation),
                ("[", Punctuation),
                ("a", String),
                (",", Punctuation),
                ("]", Punctuation),
                ("next", Key),
                (":", Punctuation),
                ("1", Number),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn multi_line_double_quoted_scalar() {
        // serde_yaml wraps long quoted scalars onto more-indented lines; a
        // `#` or `key:` inside them is string text.
        let yaml = "message: \"Back-off restarting # not a comment\n    failed: container \\\"app\\\" # still text\n\n    in pod\" # done\nesc: \"tail \\\n  next\"\nargs: [\"a\n  b\", c]\nopen: \"never closed\nnext: 1\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("message", Key),
                (":", Punctuation),
                ("\"Back-off restarting # not a comment", String),
                ("failed: container \\\"app\\\" # still text", String),
                ("in pod\"", String),
                ("# done", Comment),
                ("esc", Key),
                (":", Punctuation),
                ("\"tail \\", String),
                ("next\"", String),
                ("args", Key),
                (":", Punctuation),
                ("[", Punctuation),
                ("\"a", String),
                ("b\"", String),
                (",", Punctuation),
                ("c", String),
                ("]", Punctuation),
                ("open", Key),
                (":", Punctuation),
                ("\"never closed", String),
                // A dedent to the owning key ends an unclosed quote.
                ("next", Key),
                (":", Punctuation),
                ("1", Number),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn multi_line_single_quoted_scalar() {
        let yaml = "  message: 'Back-off restarting # not a comment\n    restarting: failed ''app'' …'\n  reason: BackOff\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("message", Key),
                (":", Punctuation),
                ("'Back-off restarting # not a comment", String),
                ("restarting: failed ''app'' …'", String),
                ("reason", Key),
                (":", Punctuation),
                ("BackOff", String),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn plain_scalar_continuation_is_string() {
        let yaml = "message: Back-off restarting failed\n  container app in pod # why\n  stale: line\nitems:\n- first part\n\n  second part\n- 2\nkey: a\n  nested: b\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("message", Key),
                (":", Punctuation),
                ("Back-off restarting failed", String),
                ("container app in pod", String),
                ("# why", Comment),
                // The comment ended the scalar.
                ("stale", Key),
                (":", Punctuation),
                ("line", String),
                ("items", Key),
                (":", Punctuation),
                ("-", Punctuation),
                ("first part", String),
                ("second part", String),
                ("-", Punctuation),
                ("2", Number),
                ("key", Key),
                (":", Punctuation),
                ("a", String),
                // A mapping key starts a new node rather than continuing.
                ("nested", Key),
                (":", Punctuation),
                ("b", String),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn anchors_aliases_and_tags_are_punctuation() {
        let yaml = "base: &defaults\n  size: !!int 3\nother: *defaults\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("base", Key),
                (":", Punctuation),
                ("&defaults", Punctuation),
                ("size", Key),
                (":", Punctuation),
                ("!!int", Punctuation),
                ("3", Number),
                ("other", Key),
                (":", Punctuation),
                ("*defaults", Punctuation),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn crlf_lines_classify_like_lf() {
        let crlf = "replicas: 1\r\nname: web # c\r\n";
        let lf = crlf.replace("\r\n", "\n");
        assert_eq!(classes(crlf), classes(&lf));
        assert!(classes(crlf).contains(&("1", Number)));
        assert_eq!(token_at(crlf, crlf.find('\r').unwrap()), Plain);
        assert_covers(crlf);
    }

    #[test]
    fn non_ascii_keys_and_values_classify() {
        let yaml = "ключ: значение # комментарий\n名前: 値\n- é: ü\n- 値\nflow: {キー: 値}\n";
        assert_eq!(
            classes(yaml),
            vec![
                ("ключ", Key),
                (":", Punctuation),
                ("значение", String),
                ("# комментарий", Comment),
                ("名前", Key),
                (":", Punctuation),
                ("値", String),
                ("-", Punctuation),
                ("é", Key),
                (":", Punctuation),
                ("ü", String),
                ("-", Punctuation),
                ("値", String),
                ("flow", Key),
                (":", Punctuation),
                ("{", Punctuation),
                ("キー", Key),
                (":", Punctuation),
                ("値", String),
                ("}", Punctuation),
            ]
        );
        assert_covers(yaml);
    }

    #[test]
    fn leading_bom_is_not_part_of_the_first_key() {
        let yaml = "\u{FEFF}apiVersion: v1\n";
        assert_eq!(
            classes(yaml),
            vec![("apiVersion", Key), (":", Punctuation), ("v1", String)]
        );
        assert_eq!(token_at(yaml, 0), Plain);
        assert_covers(yaml);

        let yaml = "\u{FEFF}# header\n---\n";
        assert_eq!(
            classes(yaml),
            vec![("# header", Comment), ("---", Punctuation)]
        );
        assert_covers(yaml);
    }

    #[test]
    fn malformed_and_partial_input_is_covered_without_panicking() {
        for yaml in [
            "",
            "\n",
            "key:",
            "\"unterminated: value",
            "a: 'open",
            "- ",
            "[unclosed",
            "}",
            "key: value\r\nnext: 1",
            "ключ: значение # комментарий",
            "a: \"tail\\",
            "x: |",
            ": orphan",
            "\u{FEFF}",
            "a: \"é\\é\"",
        ] {
            assert_covers(yaml);
        }
    }

    #[test]
    fn exhaustive_short_inputs_never_panic() {
        const ALPHABET: [char; 20] = [
            ':', '-', ' ', '#', '\'', '"', '[', ']', '{', '}', ',', '|', '>', '&', '*', '!', '\r',
            '\n', '\t', 'é',
        ];
        // Every string up to this length (about 170 000). Length 5 (3.4
        // million) takes several seconds in the unoptimised test profile.
        const MAX_LEN: usize = 4;
        let mut text = std::string::String::new();
        for len in 0..=MAX_LEN {
            let mut digits = [0; MAX_LEN];
            loop {
                text.clear();
                text.extend(digits[..len].iter().map(|&digit| ALPHABET[digit]));
                assert_covers(&text);
                // Advance the odometer over `len` positions.
                let Some(position) = digits[..len]
                    .iter()
                    .position(|&digit| digit + 1 < ALPHABET.len())
                else {
                    break;
                };
                digits[position] += 1;
                digits[..position].fill(0);
            }
        }
    }

    #[test]
    fn megabyte_single_line_flow_is_covered() {
        let mut json = std::string::String::from(
            "{\"apiVersion\":\"v1\",\"kind\":\"ConfigMap\",\"metadata\":{\"name\":\"big\"},\"data\":{",
        );
        let mut index = 0;
        while json.len() < 1 << 20 {
            json.push_str(&format!(
                "\"key-{index}\":\"value # {index}, [x]: {{y}} é\",\"n{index}\":{index},"
            ));
            index += 1;
        }
        json.push_str("\"last\":true}}");
        let yaml = format!(
            "metadata:\n  annotations:\n    kubectl.kubernetes.io/last-applied-configuration: {json}\nspec: {{}}\n"
        );
        assert_covers(&yaml);
        assert_eq!(token_at(&yaml, yaml.rfind("spec").unwrap()), Key);
    }

    #[test]
    fn layout_job_colours_tokens_from_the_theme() {
        // Every role gets its own colour, so a token painted with the wrong role is caught.
        let theme = components::test_support::distinct_role_theme();
        let font = egui::FontId::monospace(13.0);
        let job = yaml_layout_job("replicas: 3 # count\n", &font, &theme);

        let colors: Vec<_> = job
            .sections
            .iter()
            .map(|section| {
                (
                    &job.text[section.byte_range.start.0..section.byte_range.end.0],
                    section.format.color,
                )
            })
            .collect();
        assert_eq!(
            colors,
            vec![
                ("replicas", theme.syntax.key),
                (":", theme.syntax.punctuation),
                (" ", theme.code.text),
                ("3", theme.syntax.number),
                (" ", theme.code.text),
                ("# count", theme.syntax.comment),
                ("\n", theme.code.text),
            ]
        );
        assert!(
            job.sections
                .iter()
                .all(|section| section.format.font_id == font)
        );
    }
}
