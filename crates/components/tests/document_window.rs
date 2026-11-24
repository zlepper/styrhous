//! Focused snapshots of the C14 window chrome and C15 code surface parts,
//! so a regression shows up next to the component instead of only on the
//! whole boards (`c14_window_chrome.rs`, `c15_code_surface.rs`).

use components::design::theme::Tone;
use components::design::{size, space};
use components::icons::Icon;
use components::test_support::{UiHarnessSnapshot, canvas_harness};
use components::{
    Banner, BannerButton, Button, ButtonVariant, CodeColor, CodeLine, CodeSpan, CodeSurface,
    Completion, CompletionDoc, CompletionExpects, Problem, ProblemsPanel, Severity, StatusBar,
    Toolbar, WindowHeader, status_number, status_ok, status_text,
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

/// The YAML editor's variants of the parts: the status bar's "Valid", the wider documentation
/// popover with `code` in its description, and the roomier Problems panel.
#[test]
fn yaml_editor_parts() {
    let mut harness = harness(560.0, 330.0, |ui| {
        StatusBar::new("valid").show(
            ui,
            |ui| {
                status_text(ui, "Changes apply directly to the cluster");
            },
            |ui| {
                status_ok(ui, "Valid");
                status_number(ui, "Ln 12, Col 19");
                status_text(ui, "YAML");
                status_text(ui, "UTF-8");
            },
        );
        ui.add_space(space::S8);
        CompletionDoc::new(
            "enum",
            "IfNotPresent",
            "Pull the image only if it isn’t already present on the node. Default when the tag isn’t `latest`.",
        )
        .width(size::COMPLETION_DOC_WIDTH_EDITOR)
        .show(ui);
        ui.add_space(space::S8);
        let problems = [
            Problem {
                severity: Severity::Error,
                line: Some(7),
                message: "spec.replicas: expected integer, got string “three”",
            },
            Problem {
                severity: Severity::Error,
                line: Some(19),
                message: "imagePullPolicy: “If” is not one of Always, IfNotPresent, Never",
            },
        ];
        let mut open = true;
        let _ = ProblemsPanel::new("editor", &problems)
            .spacing(
                size::PROBLEMS_PADDING_EDITOR,
                size::PROBLEMS_LINE_COLUMN_EDITOR,
            )
            .active(Some(0))
            .show(ui, &mut open);
    });
    harness.ui_harness("document_window/yaml_editor_parts");
}

/// A banner with two buttons: the conflict banner of the YAML editor, with and without a
/// leading icon on the first, and a message too long for the room the buttons leave.
#[test]
fn banner_with_buttons() {
    let buttons = [
        BannerButton {
            label: "Copy my edits",
            icon: Some(Icon::ClipboardDocument),
            variant: ButtonVariant::Secondary,
        },
        BannerButton {
            label: "Discard and reload",
            icon: None,
            variant: ButtonVariant::DangerQuiet,
        },
    ];
    let mut harness = harness(640.0, 120.0, move |ui| {
        let message = "This ConfigMap changed on the cluster after you opened it (resource version 1834 → 1902). Your edits can’t be applied.";
        let clicked = Banner::new(Tone::Danger, message)
            .buttons(&buttons)
            .show(ui);
        assert_eq!(clicked.button, None);
        Banner::new(Tone::Danger, "Your edits can’t be applied.")
            .buttons(&buttons)
            .show(ui);
        Banner::new(Tone::Warning, "Short message")
            .buttons(&buttons[1..])
            .dismissible(true)
            .show(ui);
    });
    harness.ui_harness("document_window/banner_with_buttons");
}

/// Each button of a banner reports its own index, and the banner's own message and the
/// dismiss button do not.
#[test]
fn banner_buttons_report_their_index() {
    use egui_kittest::kittest::Queryable;
    let buttons = [
        BannerButton {
            label: "First",
            icon: Some(Icon::ClipboardDocument),
            variant: ButtonVariant::Secondary,
        },
        BannerButton {
            label: "Second",
            icon: None,
            variant: ButtonVariant::DangerQuiet,
        },
    ];
    let mut harness = egui_kittest::Harness::new_ui_state(
        move |ui, clicked: &mut Vec<Option<usize>>| {
            let response = Banner::new(Tone::Danger, "Message")
                .buttons(&buttons)
                .dismissible(true)
                .show(ui);
            if response.button.is_some() || response.dismissed {
                clicked.push(response.button);
            }
        },
        Vec::new(),
    );
    components::test_support::setup_egui(&mut harness);
    harness.run();
    for (label, expected) in [("Second", Some(1)), ("First", Some(0))] {
        let position = harness.get_by_label(label).rect().center();
        components::test_support::primary_click(&mut harness, position);
        harness.run();
        assert_eq!(harness.state().last(), Some(&expected), "{label}");
    }
    assert_eq!(harness.state().len(), 2);
}
