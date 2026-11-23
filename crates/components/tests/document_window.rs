//! Focused snapshots of the C14 window chrome and C15 code surface parts,
//! so a regression shows up next to the component instead of only on the
//! whole boards (`c14_window_chrome.rs`, `c15_code_surface.rs`).

use components::design::space;
use components::design::theme::Tone;
use components::icons::Icon;
use components::test_support::{UiHarnessSnapshot, canvas_harness};
use components::{
    Button, CodeColor, CodeLine, CodeSpan, CodeSurface, Completion, CompletionExpects, Problem,
    ProblemsPanel, Severity, StatusBar, Toolbar, WindowHeader, status_number, status_text,
};
use egui::Ui;
use egui_kittest::Harness;

/// A harness `width` × `height` points showing `scene` stacked on the
/// canvas with no gaps.
fn harness(width: f32, height: f32, scene: impl Fn(&mut Ui) + 'static) -> Harness<'static> {
    canvas_harness(width, height, None, scene)
}

/// In a narrow window the title truncates before the pill and actions.
#[test]
fn narrow_window_chrome_truncates_the_title() {
    let mut harness = harness(480.0, 160.0, |ui| {
        WindowHeader::new(
            "narrow",
            Icon::DocumentText,
            "kube-controller-manager-kind-control-plane",
        )
        .meta("Pod logs · container kube-controller-manager · kube-system")
        .status("Following", Tone::Success)
        .show(ui, |ui| {
            Button::primary("Apply changes").show(ui);
        });
        Toolbar::new("narrow").show(
            ui,
            |ui| {
                Button::secondary("View options")
                    .leading_icon(Icon::AdjustmentsHorizontal)
                    .show(ui);
            },
            |ui| {
                Button::icon(Icon::ChevronDoubleDown, "Follow").show(ui);
            },
        );
        ui.add_space(space::S12);
        StatusBar::new("narrow").show(
            ui,
            |ui| {
                status_number(ui, "1,284 lines");
            },
            |ui| {
                status_text(ui, "UTC");
            },
        );
    });
    harness.ui_harness("document_window/narrow_chrome");
}

/// An empty completion, and a collapsed and a stale Problems panel.
#[test]
fn empty_completion_and_problem_panel_states() {
    let mut harness = harness(420.0, 330.0, |ui| {
        let lines = [CodeLine::new([
            CodeSpan::new("mode", CodeColor::SyntaxKey),
            CodeSpan::new(":", CodeColor::SyntaxPunctuation),
            CodeSpan::text(" Zzz"),
        ])
        .number(9_999)
        .highlighted(true)
        .caret(9)];
        CodeSurface::new(&lines).show(ui);
        ui.add_space(space::S8);
        let _ = Completion::new(CompletionExpects::Value, &[]).show(ui);
        ui.add_space(space::S8);
        let problems = [Problem {
            severity: Severity::Error,
            line: Some(4),
            message: "“settings” is not an allowed value",
        }];
        let mut closed = false;
        let _ = ProblemsPanel::new("closed", &problems).show(ui, &mut closed);
        ui.add_space(space::S8);
        let mut open = true;
        let _ = ProblemsPanel::new("stale", &problems)
            .stale(true)
            .show(ui, &mut open);
    });
    harness.ui_harness("document_window/completion_and_problems_states");
}
