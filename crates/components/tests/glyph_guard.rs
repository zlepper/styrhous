//! Guard: every non-ASCII character the application puts on screen has a
//! glyph.
//!
//! The Harbour faces cover Latin text only; symbols (● ✓ ⌃ ⌄ →) and other
//! scripts come from the DejaVu fallbacks (see `design::fonts`). This scans
//! the string and char literals of every crate's `src/` tree (outside test
//! code, whose sample text deliberately includes scripts no bundled font
//! covers) and lays each non-ASCII character out in every registered family,
//! so a symbol that would render as a replacement box fails here with its
//! `file:line`.

mod source_scan;

use std::collections::BTreeMap;

use components::design::fonts;
use egui::{Color32, Context, FontFamily, FontId};
use source_scan::{Lexed, lex, test_files, test_module_lines, workspace_sources};

/// egui's replacement glyph, drawn for characters no face in the chain has.
const REPLACEMENT: char = '◻';

/// Every family text can be laid out in.
fn families() -> Vec<FontFamily> {
    [FontFamily::Proportional, FontFamily::Monospace]
        .into_iter()
        .chain(fonts::faces().map(|face| FontFamily::Name(face.family.into())))
        .collect()
}

/// Characters that need no glyph of their own: controls, invisible format
/// characters and combining marks (which only decorate a base glyph; the
/// sources use them as ranges for stripping diacritics).
fn needs_no_glyph(c: char) -> bool {
    c.is_control()
        || components::fuzzy::is_combining_mark(c)
        || unicode_normalization::char::is_combining_mark(c)
        || matches!(
            c,
            '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}'
        )
}

/// Each non-ASCII character in a non-test literal, with where it occurs.
fn literal_characters() -> BTreeMap<char, Vec<String>> {
    let lexed: Vec<(String, Lexed)> = workspace_sources()
        .into_iter()
        .map(|file| {
            let source = std::fs::read_to_string(&file.path).expect("readable source");
            (file.relative, lex(&source))
        })
        .collect();
    let test_files = test_files(&lexed);

    let mut characters: BTreeMap<char, Vec<String>> = BTreeMap::new();
    for (relative, file_lexed) in lexed {
        if test_files.contains(&relative) {
            continue;
        }
        let test_lines = test_module_lines(&file_lexed.code);
        for literal in file_lexed.literals {
            if test_lines
                .iter()
                .any(|(begin, end)| (*begin..=*end).contains(&literal.line))
            {
                continue;
            }
            for c in literal.value.chars() {
                if c.is_ascii() || needs_no_glyph(c) {
                    continue;
                }
                let places = characters.entry(c).or_default();
                let place = format!("{relative}:{}", literal.line);
                if places.last() != Some(&place) {
                    places.push(place);
                }
            }
        }
    }
    characters
}

#[test]
fn every_literal_character_has_a_glyph_in_every_family() {
    let ctx = Context::default();
    components::design::theme::configure_default(&ctx);
    // Fonts installed by `configure_default` become active on the next pass.
    let mut output = ctx.run_ui(Default::default(), |_| {});
    output.textures_delta.clear();

    let characters = literal_characters();
    assert!(
        characters.contains_key(&'●'),
        "the scan finds the status dot of the log viewer"
    );
    let families = families();
    let mut missing = Vec::new();
    ctx.fonts_mut(|fonts| {
        // `Fonts::has_glyph` reports every character of the face that holds
        // the replacement glyph as missing, and DejaVu holds it. A missing
        // character renders that glyph instead, so compare atlas regions.
        let mut atlas_region = |family: &FontFamily, c: char| {
            let galley = fonts.layout_no_wrap(
                c.to_string(),
                FontId::new(13.0, family.clone()),
                Color32::PLACEHOLDER,
            );
            galley.rows[0].glyphs[0].uv_rect
        };
        for (c, places) in &characters {
            let without: Vec<String> = families
                .iter()
                .filter(|family| {
                    *c != REPLACEMENT
                        && atlas_region(family, *c) == atlas_region(family, REPLACEMENT)
                })
                .map(ToString::to_string)
                .collect();
            if !without.is_empty() {
                missing.push(format!(
                    "  {c} (U+{:04X}) has no glyph in {} — used at {}",
                    u32::from(*c),
                    without.join(", "),
                    places.join(", ")
                ));
            }
        }
        // The check itself: a character no bundled font covers is caught.
        let uncovered = '\u{10FFFD}';
        assert_eq!(
            atlas_region(&FontFamily::Proportional, uncovered),
            atlas_region(&FontFamily::Proportional, REPLACEMENT)
        );
        assert_ne!(
            atlas_region(&FontFamily::Proportional, '●'),
            atlas_region(&FontFamily::Proportional, REPLACEMENT)
        );
    });
    assert!(
        missing.is_empty(),
        "characters that would render as a replacement box:\n{}\n\nUse a character the fonts \
         cover, or draw the symbol as an icon.",
        missing.join("\n")
    );
}
