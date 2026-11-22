//! What a log window's filter matches, shared by search and export so an
//! export of "matching lines" always agrees with the filtered view.

use crate::ansi::parse_kubernetes_log_line;
use regex::Regex;

/// The case-insensitive matcher for a filter query.
pub(crate) fn log_matcher(query: &str, regex_mode: bool) -> Result<Regex, regex::Error> {
    let pattern = if regex_mode {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
}

/// A record's message as shown and searched: without timestamp or ANSI styling.
pub(crate) fn searchable_message(raw: &str) -> String {
    parse_kubernetes_log_line(raw).line.text
}
