//! Confirmation dialogs (C11): a question, the action's effect, and a
//! footer whose confirm button repeats the verb.
//!
//! Neutral confirmations (Restart, Run now, Scale) carry an accent tile and
//! a primary confirm button; destructive ones (Delete, Discard, Remove
//! finalizers) a danger tile and a solid danger button. A typed confirmation
//! adds a danger callout listing what is bypassed and a field the resource
//! name must be typed into.

use std::cell::{Cell, RefCell};

use egui::{Context, Id, Key, Ui};

use crate::design::theme::{ThemeExt, Tone};
use crate::design::{size, space, type_scale};
use crate::dialog::{Dialog, FocusOnOpen};
use crate::icons::Icon;
use crate::prose::Prose;
use crate::{Button, Callout, Chip, TextInput};

/// Visual emphasis for a confirmation's primary action.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConfirmationDialogKind {
    /// An accent tile and a primary confirm button; the confirm button has
    /// focus when the dialog opens.
    Primary,
    /// A danger tile and a solid danger confirm button; Cancel has focus
    /// when the dialog opens.
    Destructive,
}

impl ConfirmationDialogKind {
    fn tone(self) -> Tone {
        match self {
            Self::Primary => Tone::Accent,
            Self::Destructive => Tone::Danger,
        }
    }
}

/// Content and actions for a standard application confirmation dialog.
pub struct ConfirmationDialog<'a> {
    /// Stable identity for the dialog and its input-blocking scrim.
    pub id: Id,
    /// The glyph on the header tile, naming the action (Trash, ArrowPath).
    pub icon: Icon,
    /// The pending action as a question.
    pub title: &'a str,
    /// The action's effect, naming the resource in bold.
    pub message: Prose<'a>,
    /// Optional explanation displayed while the primary action is unavailable.
    pub unavailable_message: Option<&'a str>,
    /// Label for the safe dismiss action.
    pub cancel_label: &'a str,
    /// Label for the action that confirms the operation; repeats the verb.
    pub confirm_label: &'a str,
    /// Semantic emphasis for the confirmation action.
    pub kind: ConfirmationDialogKind,
    /// Whether the confirmation action may be selected now. A typed
    /// acknowledgement must also match before it is enabled.
    pub confirm_enabled: bool,
    /// Optional danger callout listing what the action bypasses.
    pub warning: Option<ConfirmationDialogWarning<'a>>,
    /// Optional typed acknowledgement shown before the action buttons.
    pub acknowledgement: Option<ConfirmationDialogAcknowledgement<'a>>,
}

/// A name that must be typed, exactly, before a confirmation may proceed.
pub struct ConfirmationDialogAcknowledgement<'a> {
    /// The field label, "Type important-config to confirm".
    pub label: &'a str,
    /// The text that enables the confirm button (case-sensitive).
    pub expected: &'a str,
    /// The text typed so far.
    pub value: &'a mut String,
}

/// A high-risk action warning (`.callout.bad`): what is bypassed, optionally
/// as chips (finalizers) and a bounded detail list.
pub struct ConfirmationDialogWarning<'a> {
    pub title: &'a str,
    pub message: &'a str,
    /// Short identifiers shown as chips under the message.
    pub chips: &'a [String],
    /// A longer list, one entry per line, in a scrolling mono block.
    pub details: Option<&'a str>,
}

/// User action selected from a [`ConfirmationDialog`].
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConfirmationDialogAction {
    None,
    Cancel,
    Confirm,
}

/// Gap between the blocks under the header (callout, field, notes).
const BLOCK_GAP: f32 = space::S12;
/// Gap between the callout message and its chips.
const CHIPS_TOP: f32 = space::S8;
/// Tallest a warning's detail list grows before it scrolls.
const DETAILS_MAX_HEIGHT: f32 = 96.0;

/// Which control takes focus when the dialog opens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FocusTarget {
    Cancel,
    Confirm,
    Acknowledgement,
}

impl<'a> ConfirmationDialog<'a> {
    /// Show the modal dialog and report the selected action for this frame.
    /// Escape and a click on the scrim cancel.
    pub fn show(self, ctx: &Context) -> ConfirmationDialogAction {
        self.show_with_body(ctx, size::DIALOG_WIDTH, |_| {})
    }

    /// Show the dialog `width` points wide, with `body` rendered below the
    /// header for content the standard fields cannot express.
    pub fn show_with_body(
        self,
        ctx: &Context,
        width: f32,
        body: impl FnOnce(&mut Ui),
    ) -> ConfirmationDialogAction {
        let focus = FocusOnOpen::new(ctx, self.id);
        let (dialog, parts) = self.into_parts(width, focus);
        let response = dialog.show(ctx, |ui| parts.body(ui, body), |ui| parts.footer(ui));
        match parts.action(response.inner) {
            ConfirmationDialogAction::None if response.dismissed => {
                ConfirmationDialogAction::Cancel
            }
            action => action,
        }
    }

    /// Paint the dialog in place at the cursor (design boards and previews)
    /// and report the action selected this frame.
    pub fn show_inline(self, ui: &mut Ui) -> ConfirmationDialogAction {
        let (dialog, parts) = self.into_parts(size::DIALOG_WIDTH, FocusOnOpen::NEVER);
        let inner = dialog.show_inline(ui, |ui| parts.body(ui, |_| {}), |ui| parts.footer(ui));
        parts.action(inner)
    }

    fn focus_target(&self) -> FocusTarget {
        if self.acknowledgement.is_some() {
            FocusTarget::Acknowledgement
        } else {
            match self.kind {
                ConfirmationDialogKind::Primary => FocusTarget::Confirm,
                ConfirmationDialogKind::Destructive => FocusTarget::Cancel,
            }
        }
    }

    fn into_parts(self, width: f32, focus: FocusOnOpen) -> (Dialog<'a>, Parts<'a>) {
        let focus_target = self.focus_target();
        let dialog = Dialog::new(self.id, self.icon, self.kind.tone(), self.title)
            .description(self.message)
            .width(width);
        let matched = self
            .acknowledgement
            .as_ref()
            .is_none_or(|ack| *ack.value == ack.expected);
        let parts = Parts {
            unavailable_message: self.unavailable_message,
            cancel_label: self.cancel_label,
            confirm_label: self.confirm_label,
            kind: self.kind,
            confirm_enabled: self.confirm_enabled,
            warning: self.warning,
            acknowledgement: RefCell::new(self.acknowledgement),
            matched: Cell::new(matched),
            field_confirmed: Cell::new(false),
            focus,
            focus_target,
        };
        (dialog, parts)
    }
}

/// Everything of a [`ConfirmationDialog`] but its header, shared by the
/// body and footer closures of its [`Dialog`].
struct Parts<'a> {
    unavailable_message: Option<&'a str>,
    cancel_label: &'a str,
    confirm_label: &'a str,
    kind: ConfirmationDialogKind,
    confirm_enabled: bool,
    warning: Option<ConfirmationDialogWarning<'a>>,
    acknowledgement: RefCell<Option<ConfirmationDialogAcknowledgement<'a>>>,
    /// Whether the typed acknowledgement (if any) matches, after this
    /// frame's typing.
    matched: Cell<bool>,
    /// Enter was pressed in a matching acknowledgement field.
    field_confirmed: Cell<bool>,
    focus: FocusOnOpen,
    focus_target: FocusTarget,
}

impl Parts<'_> {
    fn can_confirm(&self) -> bool {
        self.confirm_enabled && self.matched.get()
    }

    /// The blocks under the header, [`BLOCK_GAP`] apart.
    fn body(&self, ui: &mut Ui, extra: impl FnOnce(&mut Ui)) {
        ui.spacing_mut().item_spacing.y = BLOCK_GAP;
        let theme = ui.tokens();
        extra(ui);
        if let Some(warning) = &self.warning {
            Callout::new(Tone::Danger, warning.title)
                .body(warning.message)
                .compact()
                .show_with(ui, |ui| {
                    if !warning.chips.is_empty() {
                        ui.add_space(CHIPS_TOP);
                        crate::chip_flow(ui, |ui| {
                            for chip in warning.chips {
                                Chip::new(chip).show(ui);
                            }
                        });
                    }
                    if let Some(details) = warning.details {
                        ui.add_space(CHIPS_TOP);
                        crate::scroll::vertical()
                            .max_height(DETAILS_MAX_HEIGHT)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                type_scale::MONO_SMALL.label(ui, details, theme.text.primary);
                            });
                    }
                });
        }
        if let Some(acknowledgement) = self.acknowledgement.borrow_mut().as_mut() {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = space::LABEL_TO_CONTROL;
                let label =
                    type_scale::FIELD_LABEL.label(ui, acknowledgement.label, theme.text.secondary);
                let response = TextInput::new(acknowledgement.value)
                    .id_salt("acknowledgement")
                    .monospace()
                    .labelled_by(label.id)
                    .show(ui);
                if self.focus_target == FocusTarget::Acknowledgement {
                    self.focus.apply(ui, &response);
                }
                let matched = *acknowledgement.value == acknowledgement.expected;
                self.matched.set(matched);
                if response.lost_focus()
                    && ui.input(|input| input.key_pressed(Key::Enter))
                    && self.can_confirm()
                {
                    self.field_confirmed.set(true);
                }
            });
        }
        if let Some(message) = self.unavailable_message {
            type_scale::META.label(ui, message, theme.text.muted);
        }
    }

    /// The footer buttons, right to left: confirm, then cancel.
    fn footer(&self, ui: &mut Ui) -> ConfirmationDialogAction {
        let confirm = match self.kind {
            ConfirmationDialogKind::Primary => Button::primary(self.confirm_label),
            ConfirmationDialogKind::Destructive => Button::danger(self.confirm_label),
        }
        .enabled(self.can_confirm())
        .show(ui);
        let cancel = Button::secondary(self.cancel_label).show(ui);
        match self.focus_target {
            FocusTarget::Confirm if confirm.enabled() => self.focus.apply(ui, &confirm),
            FocusTarget::Cancel | FocusTarget::Confirm => self.focus.apply(ui, &cancel),
            FocusTarget::Acknowledgement => {}
        }
        if confirm.clicked() {
            ConfirmationDialogAction::Confirm
        } else if cancel.clicked() {
            ConfirmationDialogAction::Cancel
        } else {
            ConfirmationDialogAction::None
        }
    }

    /// The action of this frame, given the footer's.
    fn action(&self, footer: ConfirmationDialogAction) -> ConfirmationDialogAction {
        if footer == ConfirmationDialogAction::None && self.field_confirmed.get() {
            ConfirmationDialogAction::Confirm
        } else {
            footer
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    #[derive(Default)]
    struct DialogState {
        visible: bool,
        available: bool,
        acknowledgement: String,
    }

    #[test]
    fn acknowledgement_keeps_focus_and_text_when_countdown_disappears() {
        let mut harness = Harness::new_ui_state(
            |ui, state: &mut DialogState| {
                if !state.visible {
                    return;
                }
                ConfirmationDialog {
                    id: Id::new("confirmation"),
                    icon: Icon::Trash,
                    title: "Force delete resource?",
                    message: "This removes every finalizer from the resource.".into(),
                    unavailable_message: (!state.available)
                        .then_some("Remove finalizers will be available in 1 second."),
                    cancel_label: "Cancel",
                    confirm_label: "Remove finalizers",
                    kind: ConfirmationDialogKind::Destructive,
                    confirm_enabled: state.available,
                    warning: None,
                    acknowledgement: Some(ConfirmationDialogAcknowledgement {
                        label: "Type the resource name to acknowledge:",
                        expected: "resource-name",
                        value: &mut state.acknowledgement,
                    }),
                }
                .show(ui.ctx());
            },
            DialogState::default(),
        );
        crate::test_support::setup_egui(&mut harness);
        harness.state_mut().visible = true;
        harness.run();

        harness
            .get_by_label("Type the resource name to acknowledge:")
            .click();
        harness.run();
        assert!(
            harness
                .get_by_label("Type the resource name to acknowledge:")
                .is_focused()
        );
        harness
            .get_by_label("Type the resource name to acknowledge:")
            .type_text("resource-");
        harness.run();
        assert_eq!(harness.state().acknowledgement, "resource-");

        harness.state_mut().available = true;
        harness.run();
        assert!(
            harness
                .get_by_label("Type the resource name to acknowledge:")
                .is_focused(),
            "removing the countdown must preserve the acknowledgement input's focus"
        );
        harness
            .get_by_label("Type the resource name to acknowledge:")
            .type_text("name");
        harness.run();

        assert_eq!(harness.state().acknowledgement, "resource-name");
        assert!(
            !harness
                .get_by_label("Remove finalizers")
                .accesskit_node()
                .is_disabled()
        );
    }
}
