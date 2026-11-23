//! Guard: the app shows tooltips through `components::tooltip` (C07), so
//! they are the design's cream tooltips, not egui's own popup frame.
//!
//! Scans the code (not comments or strings) of `crates/components/src` and
//! `crates/styrhous/src` outside the tooltip module for egui's tooltip
//! methods, and for truncated egui labels that would show egui's tooltip
//! of their elided text.

mod source_scan;

use source_scan::{lex, workspace_sources};

/// egui's tooltip methods on `Response`.
const STOCK_TOOLTIPS: [&str; 6] = [
    ".on_hover_text(",
    ".on_disabled_hover_text(",
    ".on_hover_ui(",
    ".on_disabled_hover_ui(",
    ".on_hover_text_at_pointer(",
    ".show_tooltip_text(",
];

/// Turns off the tooltip egui shows for a truncated label's elided text.
const NO_ELIDED_TOOLTIP: &str = ".show_tooltip_when_elided(false)";

/// The lexed code of every in-scope source file but the tooltip module, as
/// (path, code).
fn guarded_sources() -> Vec<(String, String)> {
    workspace_sources()
        .into_iter()
        .filter(|file| {
            ["crates/components/src/", "crates/styrhous/src/"]
                .iter()
                .any(|root| file.relative.starts_with(root))
                && file.relative != "crates/components/src/tooltip.rs"
        })
        .map(|file| {
            let source = std::fs::read_to_string(&file.path).expect("readable source");
            (file.relative.clone(), lex(&source).code)
        })
        .collect()
}

#[test]
fn the_app_shows_tooltips_through_the_tooltip_module() {
    let mut offenders = Vec::new();
    for (path, code) in guarded_sources() {
        for (index, line) in code.lines().enumerate() {
            if let Some(call) = STOCK_TOOLTIPS.iter().find(|call| line.contains(*call)) {
                offenders.push(format!("  {path}:{}: {call}", index + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "egui tooltips outside components::tooltip (use `TooltipExt` instead):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn truncated_labels_show_their_full_text_in_the_design_tooltip() {
    let mut offenders = Vec::new();
    for (path, code) in guarded_sources() {
        let lines = code.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            if !line.contains(".truncate()") {
                continue;
            }
            // The builder chain the call belongs to: up to the statement's end.
            let chain = lines[index..]
                .iter()
                .take_while(|line| !line.trim_end().ends_with(';'))
                .chain(
                    lines[index..]
                        .iter()
                        .find(|line| line.trim_end().ends_with(';')),
                )
                .copied()
                .collect::<String>();
            let is_label = lines[index.saturating_sub(8)..=index]
                .iter()
                .any(|line| line.contains("Label::new("));
            if is_label && !chain.contains(NO_ELIDED_TOOLTIP) {
                offenders.push(format!("  {path}:{}", index + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "truncated egui labels show egui's tooltip when elided (use \
         `tooltip::truncated_label` or `tooltip::elided_label`):\n{}",
        offenders.join("\n")
    );
}
