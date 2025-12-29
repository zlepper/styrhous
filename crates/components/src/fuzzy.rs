//! Fuzzy text matching with Unicode normalization
//!
//! Provides accent-insensitive, case-insensitive fuzzy subsequence matching
//! using Unicode NFKD normalization.
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

/// Fuzzy subsequence match: needle chars must appear in order, but not consecutively.
///
/// Uses NFKD normalization to decompose characters, strips combining
/// diacritical marks, and applies Unicode case folding. Allocation-free
/// for the haystack - single pass through both strings.
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
    let mut needle_iter = needle_chars.iter();
    let mut current = needle_iter.next();

    for h in normalize_for_search(haystack) {
        if let Some(&n) = current {
            if h == n {
                current = needle_iter.next();
            }
        }
    }

    // All needle chars consumed = match
    current.is_none()
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
}
