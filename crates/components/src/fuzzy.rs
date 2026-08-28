//! Fuzzy text matching with Unicode normalization
//!
//! Provides accent-insensitive, case-insensitive fuzzy subsequence matching
//! and relevance ranking using Unicode NFKD normalization.
//!
//! # Example
//!
//! ```
//! use components::fuzzy::{normalize_for_search, matches_fuzzy};
//!
//! // Pre-compute normalized needle (do this once, cache the result)
//! let needle: Vec<char> = normalize_for_search("cafe").collect();
//!
//! // Match against haystacks
//! assert!(matches_fuzzy("Café au lait", &needle));
//! assert!(matches_fuzzy("CAFE", &needle));
//! ```

use unicode_normalization::UnicodeNormalization;

/// The relative quality of a fuzzy match.
///
/// Higher values are better. Matches are ranked by match kind, then by fewer
/// skipped characters, a shorter normalized haystack, and earlier position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FuzzyMatchScore {
    kind: FuzzyMatchKind,
    gaps: usize,
    start: usize,
    haystack_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FuzzyMatchKind {
    Subsequence,
    Substring,
    Prefix,
    Exact,
}

impl Ord for FuzzyMatchScore {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.kind
            .cmp(&other.kind)
            .then_with(|| other.gaps.cmp(&self.gaps))
            .then_with(|| other.haystack_len.cmp(&self.haystack_len))
            .then_with(|| other.start.cmp(&self.start))
    }
}

impl PartialOrd for FuzzyMatchScore {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Check if a character is a combining diacritical mark.
///
/// Covers the main Unicode blocks for combining marks:
/// - U+0300..U+036F: Combining Diacritical Marks
/// - U+1AB0..U+1AFF: Combining Diacritical Marks Extended
/// - U+1DC0..U+1DFF: Combining Diacritical Marks Supplement
/// - U+20D0..U+20FF: Combining Diacritical Marks for Symbols
/// - U+FE20..U+FE2F: Combining Half Marks
#[inline]
pub fn is_combining_mark(c: char) -> bool {
    matches!(
        c,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

/// Normalize a string for search: NFKD decomposition, strip accents, lowercase.
///
/// This enables accent-insensitive and case-insensitive matching:
/// - "Café" → "cafe"
/// - "naïve" → "naive"
/// - "MÜNCHEN" → "munchen"
/// - "ﬁle" → "file" (ligature decomposition)
pub fn normalize_for_search(s: &str) -> impl Iterator<Item = char> + Clone + '_ {
    s.nfkd()
        .filter(|&c| !is_combining_mark(c))
        .flat_map(char::to_lowercase)
}

/// Calculate a score for a fuzzy subsequence match.
///
/// The needle characters must appear in order, but do not need to be
/// consecutive. The score favours exact, prefix, and contiguous matches over
/// sparse subsequences. Among subsequences of the same kind, it favours fewer
/// gaps, a shorter haystack, and an earlier match.
///
/// The `needle_chars` should be pre-normalized using [`normalize_for_search`].
/// Returns [`None`] when the haystack does not match.
pub fn fuzzy_match_score(haystack: &str, needle_chars: &[char]) -> Option<FuzzyMatchScore> {
    let normalized_haystack: Vec<_> = normalize_for_search(haystack).collect();
    if needle_chars.is_empty() {
        return Some(FuzzyMatchScore {
            kind: FuzzyMatchKind::Exact,
            gaps: 0,
            start: 0,
            haystack_len: normalized_haystack.len(),
        });
    }

    if normalized_haystack == needle_chars {
        return Some(FuzzyMatchScore {
            kind: FuzzyMatchKind::Exact,
            gaps: 0,
            start: 0,
            haystack_len: normalized_haystack.len(),
        });
    }

    let contiguous_start = normalized_haystack
        .windows(needle_chars.len())
        .position(|window| window == needle_chars);
    if let Some(start) = contiguous_start {
        return Some(FuzzyMatchScore {
            kind: if start == 0 {
                FuzzyMatchKind::Prefix
            } else {
                FuzzyMatchKind::Substring
            },
            gaps: 0,
            start,
            haystack_len: normalized_haystack.len(),
        });
    }

    let mut latest_starts = vec![None; needle_chars.len()];
    let mut best_match = None;
    for (index, character) in normalized_haystack.iter().enumerate() {
        for needle_index in (0..needle_chars.len()).rev() {
            if *character != needle_chars[needle_index] {
                continue;
            }
            let start = if needle_index == 0 {
                Some(index)
            } else {
                latest_starts[needle_index - 1]
            };
            if let Some(start) = start {
                latest_starts[needle_index] = Some(
                    latest_starts[needle_index]
                        .map_or(start, |latest_start| latest_start.max(start)),
                );
                if needle_index + 1 == needle_chars.len() {
                    let candidate = (index - start + 1 - needle_chars.len(), start);
                    if best_match.is_none_or(|best| candidate < best) {
                        best_match = Some(candidate);
                    }
                }
            }
        }
    }

    best_match.map(|(gaps, start)| FuzzyMatchScore {
        kind: FuzzyMatchKind::Subsequence,
        gaps,
        start,
        haystack_len: normalized_haystack.len(),
    })
}

/// Fuzzy subsequence match: needle chars must appear in order, but not consecutively.
///
/// Uses NFKD normalization to decompose characters, strips combining
/// diacritical marks, and applies Unicode case folding.
///
/// The `needle_chars` should be pre-normalized using [`normalize_for_search`].
///
/// # Examples
///
/// ```
/// use components::fuzzy::{normalize_for_search, matches_fuzzy};
///
/// let needle: Vec<char> = normalize_for_search("mf").collect();
///
/// assert!(matches_fuzzy("Michael Foster", &needle));
/// assert!(matches_fuzzy("my friend", &needle));
/// assert!(!matches_fuzzy("friend of mine", &needle)); // wrong order
/// ```
///
/// More examples:
/// - "foobar" matches "fobr" (f→o→b→r appear in order)
/// - "Café" matches "cafe" (accent-insensitive)
/// - "MÜNCHEN" matches "munchen" (case + accent insensitive)
pub fn matches_fuzzy(haystack: &str, needle_chars: &[char]) -> bool {
    fuzzy_match_score(haystack, needle_chars).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to normalize needle like callers would
    fn needle(s: &str) -> Vec<char> {
        normalize_for_search(s).collect()
    }

    #[test]
    fn test_fuzzy_matching() {
        // Basic fuzzy matching - chars in order but not consecutive
        assert!(matches_fuzzy("foobar", &needle("fobr")));
        assert!(matches_fuzzy("Michael Foster", &needle("mf")));
        assert!(matches_fuzzy("Michael Foster", &needle("mifr")));

        // Case insensitive
        assert!(matches_fuzzy("Michael Foster", &needle("MICHAEL")));
        assert!(matches_fuzzy("FOOBAR", &needle("foobar")));

        // Accent insensitive (Unicode NFKD normalization)
        assert!(matches_fuzzy("Café", &needle("cafe")));
        assert!(matches_fuzzy("naïve", &needle("naive")));
        assert!(matches_fuzzy("MÜNCHEN", &needle("munchen")));
        assert!(matches_fuzzy("Ångström", &needle("angstrom")));

        // Empty needle matches everything
        assert!(matches_fuzzy("anything", &needle("")));
        assert!(matches_fuzzy("", &needle("")));

        // Non-matches
        assert!(!matches_fuzzy("foobar", &needle("baz")));
        assert!(!matches_fuzzy("abc", &needle("cba"))); // Wrong order
        assert!(!matches_fuzzy("short", &needle("shorterlonger")));
    }

    #[test]
    fn test_normalize_for_search() {
        let normalized: String = normalize_for_search("Café").collect();
        assert_eq!(normalized, "cafe");

        let normalized: String = normalize_for_search("NAÏVE").collect();
        assert_eq!(normalized, "naive");

        // NFKD also decomposes ligatures
        let normalized: String = normalize_for_search("ﬁle").collect();
        assert_eq!(normalized, "file");
    }

    #[test]
    fn fuzzy_match_scores_rank_match_quality() {
        let query = needle("app");
        let exact = fuzzy_match_score("app", &query).expect("exact match");
        let prefix = fuzzy_match_score("apple", &query).expect("prefix match");
        let substring = fuzzy_match_score("my-app", &query).expect("substring match");
        let compact = fuzzy_match_score("a-p-p", &query).expect("compact subsequence match");
        let sparse = fuzzy_match_score("a-long-p-p", &query).expect("sparse subsequence match");

        assert!(exact > prefix);
        assert!(prefix > substring);
        assert!(substring > compact);
        assert!(compact > sparse);
    }

    #[test]
    fn fuzzy_match_scores_prefer_earlier_and_shorter_equivalent_matches() {
        let query = needle("ap");
        let earlier = fuzzy_match_score("a-p-long", &query).expect("earlier match");
        let later = fuzzy_match_score("long-a-p", &query).expect("later match");
        let shorter = fuzzy_match_score("a-p", &query).expect("shorter match");

        assert!(earlier > later);
        assert!(shorter > earlier);
    }

    #[test]
    fn fuzzy_match_scores_preserve_unicode_normalization() {
        let query = needle("cafe");
        assert!(fuzzy_match_score("Café", &query).is_some());
        assert!(fuzzy_match_score("tea", &query).is_none());
    }
}
