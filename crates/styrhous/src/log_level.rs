//! Reading the severity of a log line from the common log formats.
//!
//! Kubernetes does not carry a level with a log record, so the level is read
//! from the line's own text. The reader looks only at the start of the line,
//! where every common format puts it, and prefers a false negative over
//! calling a message that merely mentions "error" an error.

use components::LogLevel;

/// How much of a line is searched for its level.
const SEARCH_BYTES: usize = 160;
/// How many words (letters only) are tried as a bare level.
const BARE_WORDS: usize = 5;

/// The keys of structured formats (`level=info`, `"severity":"WARNING"`).
const LEVEL_KEYS: [&str; 5] = ["level", "lvl", "severity", "loglevel", "log_level"];

/// The level of a log line (as it reads without ANSI styling or the
/// Kubernetes timestamp), when it states one.
///
/// Recognised, near the start of the line: klog (`E0808 15:22:17 …`),
/// `key=value` and JSON (`level=warn`, `"level":"error"`), bracketed levels
/// in any case (`[info]`) and bare upper-case words (`INFO`, `WARNING`,
/// `ERR`, `DBG`). FATAL, PANIC and CRITICAL count as errors, TRACE as debug.
pub(crate) fn parse_log_level(text: &str) -> Option<LogLevel> {
    let head = &text[..floor_char_boundary(text, SEARCH_BYTES)];
    klog_level(head)
        .or_else(|| keyed_level(head))
        .or_else(|| bare_level(head))
}

/// The bytes of a leading level token a level badge replaces: `INFO `,
/// `[warn] `, `ERROR: ` and the white space around them; 0 when the line does
/// not start with one.
pub(crate) fn leading_level_len(text: &str) -> usize {
    let body = text.trim_start();
    let indent = text.len() - body.len();
    let (word, bracketed) = match body.strip_prefix('[') {
        Some(rest) => (rest.split(']').next().unwrap_or_default(), true),
        None => (
            body.split(|c: char| !c.is_ascii_alphabetic())
                .next()
                .unwrap_or_default(),
            false,
        ),
    };
    let upper = word.bytes().all(|byte| byte.is_ascii_uppercase());
    let closed = !bracketed || body[1 + word.len()..].starts_with(']');
    if word.is_empty() || !closed || !(bracketed || upper) || level_word(word).is_none() {
        return 0;
    }
    let token = if bracketed {
        word.len() + 2
    } else {
        word.len()
    };
    let rest = &body[token..];
    let rest = rest.strip_prefix(':').unwrap_or(rest);
    let separator = rest.len() - rest.trim_start().len();
    indent + token + (body[token..].len() - rest.len()) + separator
}

/// The largest index at or below `index` that is a character boundary.
fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// klog and glog: one severity letter and the date, `E0808 15:22:17.143`.
fn klog_level(head: &str) -> Option<LogLevel> {
    let bytes = head.as_bytes();
    let is_klog = bytes.len() > 5 && bytes[1..5].iter().all(u8::is_ascii_digit) && bytes[5] == b' ';
    if !is_klog {
        return None;
    }
    match bytes[0] {
        b'I' => Some(LogLevel::Info),
        b'W' => Some(LogLevel::Warn),
        b'E' | b'F' => Some(LogLevel::Error),
        _ => None,
    }
}

/// The word of a level in any case (`warn`, `Warning`), for positions where
/// the format already says it is a level.
fn level_word(word: &str) -> Option<LogLevel> {
    let upper = word.to_ascii_uppercase();
    match upper.as_str() {
        "ERROR" | "ERR" | "ERRO" | "FATAL" | "PANIC" | "CRITICAL" | "CRIT" | "SEVERE" => {
            Some(LogLevel::Error)
        }
        "WARN" | "WARNING" | "WRN" => Some(LogLevel::Warn),
        "INFO" | "INF" | "NOTICE" | "INFORMATION" => Some(LogLevel::Info),
        "DEBUG" | "DBG" | "DEBU" | "TRACE" | "TRC" | "VERBOSE" => Some(LogLevel::Debug),
        _ => None,
    }
}

/// `level=info`, `level: warn`, `"level":"error"`.
fn keyed_level(head: &str) -> Option<LogLevel> {
    let lower = head.to_ascii_lowercase();
    LEVEL_KEYS
        .iter()
        .filter_map(|key| {
            let mut from = 0;
            while let Some(found) = lower[from..].find(key) {
                let start = from + found;
                let end = start + key.len();
                from = end;
                let before = lower[..start].chars().next_back();
                if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                    continue;
                }
                let Some(rest) = lower[end..]
                    .trim_start_matches(['"', '\''])
                    .trim_start()
                    .strip_prefix(['=', ':'])
                else {
                    continue;
                };
                let value = rest
                    .trim_start()
                    .trim_start_matches(['"', '\''])
                    .split(|c: char| !c.is_ascii_alphabetic())
                    .next()
                    .unwrap_or_default();
                if let Some(level) = level_word(value) {
                    return Some((start, level));
                }
            }
            None
        })
        .min_by_key(|(start, _)| *start)
        .map(|(_, level)| level)
}

/// A level standing alone near the start: `[info]`, `INFO`, `ERROR:`.
fn bare_level(head: &str) -> Option<LogLevel> {
    let mut words = 0;
    let mut chars = head.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if !c.is_ascii_alphabetic() {
            chars.next();
            continue;
        }
        let mut end = start;
        while let Some(&(index, c)) = chars.peek() {
            if !c.is_ascii_alphabetic() {
                break;
            }
            end = index + c.len_utf8();
            chars.next();
        }
        let word = &head[start..end];
        let bracketed = head[..start].ends_with('[') && head[end..].starts_with(']');
        let upper = word.bytes().all(|b| b.is_ascii_uppercase());
        let level = if bracketed || upper {
            level_word(word)
        } else {
            None
        };
        if level.is_some() {
            return level;
        }
        words += 1;
        if words >= BARE_WORDS {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use LogLevel::{Debug, Error, Info, Warn};

    #[test]
    fn bare_upper_case_words_are_levels() {
        assert_eq!(parse_log_level(" INFO  server: listening"), Some(Info));
        assert_eq!(parse_log_level("WARN cache: refreshing"), Some(Warn));
        assert_eq!(parse_log_level("WARNING: disk almost full"), Some(Warn));
        assert_eq!(parse_log_level("ERROR payments: upstream 503"), Some(Error));
        assert_eq!(
            parse_log_level("2026-08-08 15:22:17 DEBUG gc ran"),
            Some(Debug)
        );
        assert_eq!(parse_log_level("15:22:17.143 [ERR] failed"), Some(Error));
        assert_eq!(parse_log_level("TRACE entering handler"), Some(Debug));
        assert_eq!(parse_log_level("FATAL out of memory"), Some(Error));
        assert_eq!(parse_log_level("ERROR:root:Traceback"), Some(Error));
    }

    #[test]
    fn brackets_and_keys_accept_any_case() {
        assert_eq!(parse_log_level("[info] ready"), Some(Info));
        assert_eq!(
            parse_log_level("2026/08/08 [Warn] slow request"),
            Some(Warn)
        );
        assert_eq!(parse_log_level("ts=1 level=error msg=boom"), Some(Error));
        assert_eq!(parse_log_level("time=1 level=WARNING msg=slow"), Some(Warn));
        assert_eq!(parse_log_level("lvl=dbg x=1"), Some(Debug));
        assert_eq!(
            parse_log_level(r#"{"ts":"1","level":"info","msg":"ready"}"#),
            Some(Info)
        );
        assert_eq!(
            parse_log_level(r#"{"severity": "CRITICAL", "msg": "down"}"#),
            Some(Error)
        );
        assert_eq!(parse_log_level("level: warn something"), Some(Warn));
    }

    #[test]
    fn a_leading_level_token_is_what_the_badge_replaces() {
        assert_eq!(leading_level_len("INFO server: ready"), 5);
        assert_eq!(leading_level_len(" INFO  server"), 7);
        assert_eq!(leading_level_len("[warn] slow"), 7);
        assert_eq!(leading_level_len("ERROR: boom"), 7);
        assert_eq!(leading_level_len("WARNING"), 7);
        assert_eq!(leading_level_len("level=info msg=x"), 0);
        assert_eq!(leading_level_len("server: INFO later"), 0);
        assert_eq!(leading_level_len("Info is prose"), 0);
        assert_eq!(leading_level_len("[unclosed INFO"), 0);
        assert_eq!(leading_level_len(""), 0);
    }

    #[test]
    fn klog_prefixes_are_levels() {
        assert_eq!(
            parse_log_level("I0808 15:22:17.143 controller.go:12 sync"),
            Some(Info)
        );
        assert_eq!(
            parse_log_level("W0808 15:22:17.143 controller.go:12 slow"),
            Some(Warn)
        );
        assert_eq!(
            parse_log_level("E0808 15:22:17.143 controller.go:12 boom"),
            Some(Error)
        );
        assert_eq!(
            parse_log_level("F0808 15:22:17.143 main.go:1 exit"),
            Some(Error)
        );
    }

    #[test]
    fn the_earliest_key_wins() {
        assert_eq!(
            parse_log_level("severity=warn level=error"),
            Some(Warn),
            "the first stated level is the line's"
        );
    }

    #[test]
    fn messages_that_only_mention_a_level_have_none() {
        assert_eq!(parse_log_level("server: listening on 0.0.0.0:8080"), None);
        assert_eq!(parse_log_level("no error here, just info"), None);
        assert_eq!(parse_log_level("Error: lower case words are prose"), None);
        assert_eq!(
            parse_log_level("the sublevel=error is not a level key"),
            None
        );
        assert_eq!(parse_log_level("Eleven items processed"), None);
        assert_eq!(parse_log_level("E0808x not klog"), None);
        assert_eq!(parse_log_level(""), None);
    }

    #[test]
    fn only_the_start_of_a_long_line_is_searched() {
        let line = format!("{} ERROR late", "x".repeat(SEARCH_BYTES));
        assert_eq!(parse_log_level(&line), None);
        let multibyte = format!("{}INFO", "é".repeat(SEARCH_BYTES));
        assert_eq!(
            parse_log_level(&multibyte),
            None,
            "must not split a character"
        );
    }
}

#[cfg(test)]
mod key_scan_tests {
    use super::*;

    #[test]
    fn a_key_word_without_a_value_does_not_hide_a_later_one() {
        assert_eq!(
            parse_log_level("level set to x; level=error"),
            Some(LogLevel::Error)
        );
    }
}
