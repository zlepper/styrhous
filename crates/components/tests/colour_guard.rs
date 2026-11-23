//! Guard: colour values are written only in `design::palette`.
//!
//! Components and screens read theme roles (`ui.tokens()`); a colour literal
//! anywhere else bypasses the theme and breaks user themes and live
//! switching. This test scans every crate's `src/` tree and fails with the
//! offending `file:line` for each literal it finds.
//!
//! Exempt are [`ALLOWED_FILES`], the `#[cfg(test)]` modules and test files
//! under `design/` (which check the palette's values), and regions marked
//!
//! ```text
//! // colour-guard: allow-begin (the reason)
//! // colour-guard: allow-end
//! ```
//!
//! for colours that come from data rather than the theme. An exemption that
//! no longer contains a literal fails too, so stale ones get removed.

mod source_scan;

use std::collections::BTreeSet;
use std::path::Path;

use source_scan::{
    Lexed, SourceFile, lex, module_file, test_files, test_module_files, test_module_lines,
    workspace_sources,
};

/// Files allowed to construct colours, with the reason.
const ALLOWED_FILES: &[(&str, &str)] = &[(
    "crates/components/src/design/palette.rs",
    "the palette: the only colour literals",
)];

/// Where `#[cfg(test)]` code may use colour literals to check palette values.
const TEST_CODE_ALLOWED_UNDER: &str = "crates/components/src/design/";

const REGION_BEGIN: &str = "colour-guard: allow-begin";
const REGION_END: &str = "colour-guard: allow-end";

/// `Color32` and `Rgba` constructors and constants that create a colour from
/// a literal. `TRANSPARENT` and `PLACEHOLDER` are not colours and stay
/// allowed.
const FORBIDDEN: &[&str] = &[
    "from_rgb",
    "from_rgb_additive",
    "from_rgba_premultiplied",
    "from_rgba_unmultiplied",
    "from_rgba_unmultiplied_const",
    "from_srgba_premultiplied",
    "from_srgba_unmultiplied",
    "from_gray",
    "from_black_alpha",
    "from_white_alpha",
    "from_additive_luminance",
    "from_luminance_alpha",
    "from_hex",
    "BLACK",
    "DARK_GRAY",
    "GRAY",
    "LIGHT_GRAY",
    "WHITE",
    "BROWN",
    "DARK_RED",
    "RED",
    "LIGHT_RED",
    "CYAN",
    "MAGENTA",
    "YELLOW",
    "ORANGE",
    "LIGHT_YELLOW",
    "KHAKI",
    "DARK_GREEN",
    "GREEN",
    "LIGHT_GREEN",
    "DARK_BLUE",
    "BLUE",
    "LIGHT_BLUE",
    "PURPLE",
    "GOLD",
    "DEBUG_COLOR",
];

fn identifier_at(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    &text[..end]
}

/// Whether `text[index..]` starts a new identifier (not the tail of a longer one).
fn starts_word(text: &str, index: usize) -> bool {
    !text[..index]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The colour literal on a line of code (comments and string contents
/// already blanked out by the lexer), if any.
fn colour_literal(code: &str) -> Option<String> {
    for type_name in ["Color32::", "Rgba::"] {
        for (start, _) in code.match_indices(type_name) {
            if !starts_word(code, start) {
                continue;
            }
            let name = identifier_at(&code[start + type_name.len()..]);
            if FORBIDDEN.contains(&name) {
                return Some(format!("{type_name}{name}"));
            }
        }
    }
    for (start, _) in code.match_indices("hex_color!") {
        if starts_word(code, start) {
            return Some("hex_color!".to_owned());
        }
    }
    for type_name in ["HsvaGamma", "Hsva"] {
        for (start, _) in code.match_indices(type_name) {
            if !starts_word(code, start) || identifier_at(&code[start..]) != type_name {
                continue;
            }
            let rest = code[start + type_name.len()..].trim_start();
            if rest.starts_with("::") || rest.starts_with('{') {
                return Some(type_name.to_owned());
            }
        }
    }
    None
}

/// A literal found in a source file.
struct Finding {
    relative: String,
    line: usize,
    literal: String,
}

/// How a file is exempted from the guard.
enum Exemption {
    None,
    WholeFile,
    /// Lines (1-based, inclusive) that may contain literals.
    Lines(Vec<(usize, usize)>),
}

/// The marked allow regions of `source`, with an error for each malformed one.
fn marked_regions(source: &str) -> (Vec<(usize, usize)>, Vec<String>) {
    let mut regions = Vec::new();
    let mut errors = Vec::new();
    let mut open: Option<usize> = None;
    for (index, line) in source.lines().enumerate() {
        let number = index + 1;
        if let Some(position) = line.find(REGION_BEGIN) {
            let reason = line[position + REGION_BEGIN.len()..].trim();
            if open.is_some() {
                errors.push(format!("line {number}: nested `{REGION_BEGIN}`"));
            }
            if !(reason.starts_with('(') && reason.len() > 2) {
                errors.push(format!(
                    "line {number}: `{REGION_BEGIN}` needs a reason in parentheses"
                ));
            }
            open = Some(number);
        } else if line.contains(REGION_END) {
            match open.take() {
                Some(begin) => regions.push((begin, number)),
                None => errors.push(format!("line {number}: `{REGION_END}` without a begin")),
            }
        }
    }
    if let Some(begin) = open {
        errors.push(format!("line {begin}: `{REGION_BEGIN}` is never closed"));
    }
    (regions, errors)
}

struct Scan {
    violations: Vec<Finding>,
    /// Literals inside exemptions, per allowlisted file or marked region.
    exempted: Vec<Finding>,
    regions: Vec<(String, (usize, usize))>,
    region_errors: Vec<String>,
}

fn scan(files: &[SourceFile]) -> Scan {
    let sources: Vec<String> = files
        .iter()
        .map(|file| std::fs::read_to_string(&file.path).expect("readable source"))
        .collect();
    let lexed: Vec<(String, Lexed)> = files
        .iter()
        .zip(&sources)
        .map(|(file, source)| (file.relative.clone(), lex(source)))
        .collect();
    let test_files: BTreeSet<String> = test_files(&lexed)
        .into_iter()
        .filter(|path| path.starts_with(TEST_CODE_ALLOWED_UNDER))
        .collect();
    let lexed_files = files
        .iter()
        .zip(sources)
        .zip(lexed.into_iter().map(|(_, lexed)| lexed))
        .map(|((file, source), lexed)| (file, source, lexed));

    let mut result = Scan {
        violations: Vec::new(),
        exempted: Vec::new(),
        regions: Vec::new(),
        region_errors: Vec::new(),
    };
    for (file, source, lexed) in lexed_files {
        let (regions, errors) = marked_regions(&source);
        result.region_errors.extend(
            errors
                .into_iter()
                .map(|error| format!("  {}: {error}", file.relative)),
        );
        result.regions.extend(
            regions
                .iter()
                .map(|region| (file.relative.clone(), *region)),
        );
        let allowlisted = ALLOWED_FILES
            .iter()
            .any(|(allowed, _)| *allowed == file.relative);
        let exemption = if allowlisted || test_files.contains(&file.relative) {
            Exemption::WholeFile
        } else if file.relative.starts_with(TEST_CODE_ALLOWED_UNDER) {
            Exemption::Lines(test_module_lines(&lexed.code))
        } else {
            Exemption::None
        };
        for (index, code) in lexed.code.lines().enumerate() {
            let line = index + 1;
            let Some(literal) = colour_literal(code) else {
                continue;
            };
            let finding = Finding {
                relative: file.relative.clone(),
                line,
                literal,
            };
            let in_region = regions
                .iter()
                .any(|(begin, end)| (*begin..=*end).contains(&line));
            let exempt = match &exemption {
                Exemption::WholeFile => true,
                Exemption::Lines(ranges) => ranges
                    .iter()
                    .any(|(begin, end)| (*begin..=*end).contains(&line)),
                Exemption::None => false,
            };
            if in_region || allowlisted {
                result.exempted.push(finding);
            } else if !exempt {
                result.violations.push(finding);
            }
        }
    }
    result
}

#[test]
fn colour_literals_only_live_in_the_palette() {
    let scan = scan(&workspace_sources());
    assert!(
        scan.region_errors.is_empty(),
        "malformed colour-guard regions:\n{}",
        scan.region_errors.join("\n")
    );
    let violations: Vec<String> = scan
        .violations
        .iter()
        .map(|finding| {
            format!(
                "  {}:{}: {}",
                finding.relative, finding.line, finding.literal
            )
        })
        .collect();
    assert!(
        violations.is_empty(),
        "colour literals outside design::palette (read a theme role via `ui.tokens()` instead, \
         or add the colour to the palette and a role to the theme):\n{}\n\nAllowed files:\n{}\n\
         Colours that come from data may sit between `// {REGION_BEGIN} (reason)` and \
         `// {REGION_END}` comments.",
        violations.join("\n"),
        ALLOWED_FILES
            .iter()
            .map(|(path, reason)| format!("  {path}: {reason}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[test]
fn every_exemption_still_contains_a_colour_literal() {
    let scan = scan(&workspace_sources());
    let mut stale = Vec::new();
    for (path, _) in ALLOWED_FILES {
        if !scan
            .exempted
            .iter()
            .any(|finding| finding.relative == *path)
        {
            stale.push(format!("  allowlisted file {path}"));
        }
    }
    for (path, (begin, end)) in &scan.regions {
        let used = scan
            .exempted
            .iter()
            .any(|finding| finding.relative == *path && (*begin..=*end).contains(&finding.line));
        if !used {
            stale.push(format!("  region {path}:{begin}-{end}"));
        }
    }
    assert!(
        stale.is_empty(),
        "colour-guard exemptions without any colour literal; remove them:\n{}",
        stale.join("\n")
    );
}

#[test]
fn the_guard_recognises_literals_and_skips_non_colours() {
    let code = |line: &str| lex(line).code;
    let found = |line: &str| colour_literal(&code(line));
    assert_eq!(
        found("let c = egui::Color32::from_rgb(1, 2, 3);").as_deref(),
        Some("Color32::from_rgb")
    );
    assert_eq!(
        found("fill: Color32::WHITE,").as_deref(),
        Some("Color32::WHITE")
    );
    assert_eq!(
        found("let c = Color32::from_rgb_additive(1, 2, 3);").as_deref(),
        Some("Color32::from_rgb_additive")
    );
    assert_eq!(
        found("let c = hex_color!(\"#ff0000\");").as_deref(),
        Some("hex_color!")
    );
    assert_eq!(
        found("let c = Hsva::new(0.5, 1.0, 1.0, 1.0);").as_deref(),
        Some("Hsva")
    );
    assert_eq!(
        found("let c = HsvaGamma { h: 0.0, s: 1.0, v: 1.0, a: 1.0 };").as_deref(),
        Some("HsvaGamma")
    );
    assert_eq!(found("fn tint(color: Hsva) -> Hsva"), None);
    assert_eq!(found("Color32::TRANSPARENT"), None);
    assert_eq!(found("Color32::PLACEHOLDER"), None);
    assert_eq!(found("Color32::WHITE_ISH"), None);
    assert_eq!(found("MyColor32::WHITE"), None);
    assert_eq!(found("// Color32::BLACK in a comment"), None);
    assert_eq!(found("/* Color32::BLACK */ let a = 1;"), None);
    assert_eq!(found("let s = \"Color32::BLACK\";"), None);
    // A `//` inside a string does not hide the code after it.
    assert_eq!(
        found("let url = \"https://example.com\"; let c = Color32::RED;").as_deref(),
        Some("Color32::RED")
    );
}

#[test]
fn regions_need_a_reason_and_must_be_closed() {
    let (regions, errors) = marked_regions(
        "a\n// colour-guard: allow-begin (xterm)\nb\n// colour-guard: allow-end\nc\n",
    );
    assert_eq!(regions, [(2, 4)]);
    assert!(errors.is_empty());

    let (_, errors) = marked_regions("// colour-guard: allow-begin\n");
    assert_eq!(errors.len(), 2, "{errors:?}");
    let (_, errors) = marked_regions("// colour-guard: allow-end\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
}
#[test]
fn the_lexer_separates_code_comments_and_literals() {
    let source = concat!(
        "let a = \"// not a comment\"; Color32::RED // trailing\n",
        "/* block /* nested */ still */ let b = 'x';\n",
        "fn f<'a>(s: &'a str) { r#\"raw \"quoted\"\"#; b\"bytes\"; '\\u{2192}' }\n",
        "let c = \"line \\\n    continued \\u{25CF}\";\n",
    );
    let lexed = lex(source);
    let lines: Vec<&str> = lexed.code.lines().collect();
    assert_eq!(lines.len(), 5, "a continued string keeps its line break");
    assert!(lines[0].contains("Color32::RED"));
    assert!(!lines[0].contains("trailing"));
    assert!(!lines[0].contains("not a comment"));
    assert!(!lines[1].contains("nested"));
    assert!(!lines[1].contains("still"));
    assert!(lines[1].contains("let b"));
    assert!(lines[2].contains("fn f<'a>(s: &'a str)"));
    let values: Vec<(usize, &str)> = lexed
        .literals
        .iter()
        .map(|literal| (literal.line, literal.value.as_str()))
        .collect();
    assert_eq!(
        values,
        [
            (1, "// not a comment"),
            (2, "x"),
            (3, "raw \"quoted\""),
            (3, "bytes"),
            (3, "→"),
            (4, "line continued ●"),
        ]
    );
}

#[test]
fn test_modules_are_found_inline_and_as_files() {
    let code = "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn b() {\n    }\n}\nfn c() {}\n#[cfg(test)]\nmod parity_tests;\n";
    assert_eq!(test_module_lines(code), [(2, 6)]);
    assert_eq!(test_module_files(code), ["parity_tests"]);
    assert_eq!(
        module_file(Path::new("design/theme/mod.rs"), "parity_tests")[0],
        Path::new("design/theme/parity_tests.rs")
    );
    assert_eq!(
        module_file(Path::new("design/palette.rs"), "tests")[0],
        Path::new("design/palette/tests.rs")
    );
}
