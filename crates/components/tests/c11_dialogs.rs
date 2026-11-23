//! The C11 "Dialog" component board
//! (`docs/design-oracles/harbour/src/C11.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C11`.

use components::icons::Icon;
use components::test_support::board::{BoardHeader, board, column, row, section};
use components::test_support::{UiHarnessSnapshot, setup_egui};
use components::{
    ConfirmationDialog, ConfirmationDialogAcknowledgement, ConfirmationDialogAction,
    ConfirmationDialogKind, ConfirmationDialogWarning, ErrorDialog, Prose,
};
use egui::{Align, Id, Ui};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C11 · components::{ConfirmationDialog, ErrorDialog}",
    title: "Dialog",
    description: "Modal, 440 px, centred over the scrim. Icon, title and sub-title share the top \
                  row; content below runs full width. The icon tile carries the tone; the title \
                  is a question; the confirm button repeats the verb. Esc and the scrim cancel; \
                  Enter confirms only when the confirm button has focus.",
};

/// Gap between the board's columns.
const COLUMN_GAP: f32 = 40.0;
/// Gap between the sections of a column.
const SECTION_GAP: f32 = 28.0;

const TYPED_LABEL: &str = "Type important-config to confirm";

fn confirmation(
    id: &str,
    icon: Icon,
    title: &'static str,
    message: Prose<'static>,
    kind: ConfirmationDialogKind,
    labels: (&'static str, &'static str),
) -> ConfirmationDialog<'static> {
    ConfirmationDialog {
        id: Id::new(id),
        icon,
        title,
        message,
        unavailable_message: None,
        cancel_label: labels.0,
        confirm_label: labels.1,
        kind,
        confirm_enabled: true,
        warning: None,
        acknowledgement: None,
    }
}

fn c11_scene(ui: &mut Ui, typed: &mut String) {
    let finalizers = ["example.com/cleanup".to_owned()];
    board(ui, HEADER, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            column(ui, SECTION_GAP, |ui| {
                section(ui, "Confirmation", |ui| {
                    confirmation(
                        "restart",
                        Icon::ArrowPath,
                        "Restart rollout?",
                        Prose::new()
                            .plain("Starts a rolling replacement of the pods for ")
                            .strong("coredns")
                            .plain(" in ")
                            .strong("kube-system")
                            .plain("."),
                        ConfirmationDialogKind::Primary,
                        ("Cancel", "Restart rollout"),
                    )
                    .show_inline(ui);
                });
                section(ui, "Destructive confirmation", |ui| {
                    confirmation(
                        "discard",
                        Icon::ExclamationTriangle,
                        "Discard changes?",
                        Prose::new()
                            .plain("Your unsaved edits to ")
                            .strong("settings")
                            .plain(" will be lost."),
                        ConfirmationDialogKind::Destructive,
                        ("Keep editing", "Discard changes"),
                    )
                    .show_inline(ui);
                });
            });
            section(
                ui,
                "Type-to-confirm (button enabled once the name matches)",
                |ui| {
                    ConfirmationDialog {
                        warning: Some(ConfirmationDialogWarning {
                            title: "This bypasses controller cleanup",
                            message: "Can orphan external infrastructure, leak data and corrupt \
                                      application state.",
                            chips: &finalizers,
                            details: None,
                        }),
                        acknowledgement: Some(ConfirmationDialogAcknowledgement {
                            label: TYPED_LABEL,
                            expected: "important-config",
                            value: typed,
                        }),
                        ..confirmation(
                            "force-delete",
                            Icon::Trash,
                            "Force delete resource?",
                            Prose::new()
                                .plain("Removes every finalizer from ")
                                .strong("important-config")
                                .plain("."),
                            ConfirmationDialogKind::Destructive,
                            ("Cancel", "Remove finalizers"),
                        )
                    }
                    .show_inline(ui);
                },
            );
            section(ui, "Error", |ui| {
                ErrorDialog {
                    id: Id::new("terminal-error"),
                    title: "Couldn’t launch the terminal",
                    message: "No terminal emulator was found. Choose one in Application settings.",
                    details: Some("spawn alacritty: No such file or directory (os error 2)"),
                    recovery: None,
                    primary_action_label: Some("Open settings"),
                }
                .show_inline(ui);
            });
        });
    });
}

#[test]
fn c11_dialogs_board() {
    let mut harness = Harness::builder().build_ui_state(
        |ui, typed: &mut String| c11_scene(ui, typed),
        "important-conf".to_owned(),
    );
    setup_egui(&mut harness);
    // The board shows the focused field with its caret lit.
    harness
        .ctx
        .all_styles_mut(|style| style.visuals.text_cursor.blink = false);
    harness.run();
    harness.get_by_label(TYPED_LABEL).focus();
    harness.run();
    harness.ui_harness("boards/c11_dialogs");
}

/// The typed confirmation enables its confirm button only on an exact,
/// case-sensitive match, and Enter in the field then confirms.
#[test]
fn typed_confirmation_enables_on_an_exact_match_and_confirms_with_enter() {
    struct State {
        typed: String,
        action: ConfirmationDialogAction,
    }
    let mut harness = Harness::builder().build_ui_state(
        |ui, state: &mut State| {
            let action = ConfirmationDialog {
                acknowledgement: Some(ConfirmationDialogAcknowledgement {
                    label: TYPED_LABEL,
                    expected: "important-config",
                    value: &mut state.typed,
                }),
                ..confirmation(
                    "typed",
                    Icon::Trash,
                    "Force delete resource?",
                    "Removes every finalizer.".into(),
                    ConfirmationDialogKind::Destructive,
                    ("Cancel", "Remove finalizers"),
                )
            }
            .show(ui.ctx());
            if action != ConfirmationDialogAction::None {
                state.action = action;
            }
        },
        State {
            typed: String::new(),
            action: ConfirmationDialogAction::None,
        },
    );
    setup_egui(&mut harness);
    harness.run();
    let field = harness.get_by_label(TYPED_LABEL);
    assert!(field.is_focused(), "the name field has focus on open");
    field.type_text("Important-config");
    harness.run();
    assert!(
        harness
            .get_by_label("Remove finalizers")
            .accesskit_node()
            .is_disabled(),
        "a case mismatch keeps the confirm button disabled"
    );
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().action, ConfirmationDialogAction::None);

    harness.state_mut().typed = "important-config".to_owned();
    harness.run();
    assert!(
        !harness
            .get_by_label("Remove finalizers")
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label(TYPED_LABEL).focus();
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().action, ConfirmationDialogAction::Confirm);
}

/// Neutral confirmations focus their confirm button and destructive ones
/// Cancel; Escape cancels.
#[test]
fn confirmations_focus_by_tone_and_cancel_on_escape() {
    for (kind, focused) in [
        (ConfirmationDialogKind::Primary, "Restart rollout"),
        (ConfirmationDialogKind::Destructive, "Cancel"),
    ] {
        let mut harness = Harness::builder().build_ui_state(
            move |ui, action: &mut ConfirmationDialogAction| {
                let shown = confirmation(
                    "focus",
                    Icon::ArrowPath,
                    "Restart rollout?",
                    "Starts a rolling replacement.".into(),
                    kind,
                    ("Cancel", "Restart rollout"),
                )
                .show(ui.ctx());
                if shown != ConfirmationDialogAction::None {
                    *action = shown;
                }
            },
            ConfirmationDialogAction::None,
        );
        setup_egui(&mut harness);
        harness.run();
        assert!(harness.get_by_label(focused).is_focused(), "{kind:?}");
        harness.key_press(egui::Key::Escape);
        harness.run();
        assert_eq!(*harness.state(), ConfirmationDialogAction::Cancel);
    }
}

/// The dialog is an alert dialog named by its title and described by its
/// message.
#[test]
fn dialogs_are_alert_dialogs_named_by_their_title() {
    let mut harness = Harness::builder().build_ui(|ui| {
        confirmation(
            "a11y",
            Icon::ArrowPath,
            "Restart rollout?",
            Prose::new().plain("Restarts ").strong("coredns").plain("."),
            ConfirmationDialogKind::Primary,
            ("Cancel", "Restart rollout"),
        )
        .show_inline(ui);
    });
    setup_egui(&mut harness);
    harness.run();
    let dialog =
        harness.get_by_role_and_label(egui::accesskit::Role::AlertDialog, "Restart rollout?");
    // Described by its description paragraph, so the words are read once.
    harness.get_by_label("Restarts coredns.");
    let node = dialog.accesskit_node();
    assert_eq!(node.data().described_by().len(), 1);
    assert!(node.description().is_none());
}

/// A destructive confirmation as a modal: centred over the scrim, Cancel
/// focused.
#[test]
fn destructive_confirmation_modal_over_the_scrim() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(640.0, 400.0))
        .build_ui(|ui| {
            let theme = components::design::theme::ThemeExt::tokens(ui);
            ui.painter()
                .rect_filled(ui.ctx().content_rect(), 0, theme.surface.canvas);
            components::design::type_scale::BODY.label(
                ui,
                "Content under the scrim",
                theme.text.primary,
            );
            confirmation(
                "modal",
                Icon::Trash,
                "Delete pod?",
                Prose::new()
                    .plain("Deletes ")
                    .strong("coredns-66bc5c9577-ffw2s")
                    .plain(" from ")
                    .strong("kube-system")
                    .plain("."),
                ConfirmationDialogKind::Destructive,
                ("Cancel", "Delete pod"),
            )
            .show(ui.ctx());
        });
    components::design::theme::configure_default(&harness.ctx);
    egui_extras::install_image_loaders(&harness.ctx);
    harness.run();
    assert!(harness.get_by_label("Cancel").is_focused());
    harness.ui_harness("dialogs/destructive_confirmation_modal");
}

/// The error dialog focuses Close, reports its recovery action and Close,
/// and Escape dismisses it.
#[test]
fn error_dialog_focuses_close_and_reports_its_actions() {
    use components::ErrorDialogAction;
    let actions = [
        ("Open settings", ErrorDialogAction::PrimaryAction),
        ("Close", ErrorDialogAction::Dismiss),
    ];
    for (label, expected) in actions.into_iter().map(Some).chain([None]).map(|entry| {
        entry.map_or((None, ErrorDialogAction::Dismiss), |(label, action)| {
            (Some(label), action)
        })
    }) {
        let mut harness = Harness::builder().build_ui_state(
            |ui, action: &mut ErrorDialogAction| {
                let shown = ErrorDialog {
                    id: Id::new("error-dialog"),
                    title: "Couldn’t launch the terminal",
                    message: "No terminal emulator was found.",
                    details: Some("spawn alacritty: No such file or directory (os error 2)"),
                    recovery: None,
                    primary_action_label: Some("Open settings"),
                }
                .show(ui.ctx());
                if shown != ErrorDialogAction::None {
                    *action = shown;
                }
            },
            ErrorDialogAction::None,
        );
        setup_egui(&mut harness);
        harness.run();
        assert!(harness.get_by_label("Close").is_focused());
        match label {
            Some(label) => harness.get_by_label(label).click(),
            None => harness.key_press(egui::Key::Escape),
        }
        harness.run();
        assert_eq!(*harness.state(), expected, "{label:?}");
    }
}
