//! Error dialogs (C11): what failed in plain words, the technical detail in
//! a code block, and the recovery action as primary when one exists.

use egui::{Context, Id, Ui};

use crate::Button;
use crate::CodeBlock;
use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale;
use crate::dialog::{Dialog, FocusOnOpen};
use crate::icons::Icon;

/// Tallest the technical details grow before they scroll.
const DETAILS_MAX_HEIGHT: f32 = 240.0;

/// The label of the button that closes an error dialog.
pub const CLOSE_LABEL: &str = "Close";

/// Content and actions for a standard application error dialog.
pub struct ErrorDialog<'a> {
    /// Stable identity for the dialog and its input-blocking scrim.
    pub id: Id,
    /// Clear description of the failed user-visible operation.
    pub title: &'a str,
    /// Brief explanation of the failure, in plain words.
    pub message: &'a str,
    /// Optional technical details for diagnosing the failure.
    pub details: Option<&'a str>,
    /// Optional next-step guidance, under the details.
    pub recovery: Option<&'a str>,
    /// Optional recovery action label. The caller handles the action result.
    pub primary_action_label: Option<&'a str>,
}

/// User action selected from an [`ErrorDialog`].
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ErrorDialogAction {
    None,
    Dismiss,
    PrimaryAction,
}

impl ErrorDialog<'_> {
    /// Show the modal error dialog and report the action selected this
    /// frame. Escape and a click on the scrim dismiss it; Close has focus
    /// when it opens.
    pub fn show(&self, ctx: &Context) -> ErrorDialogAction {
        let focus = FocusOnOpen::new(ctx, self.id);
        let response = self
            .dialog()
            .show(ctx, |ui| self.body(ui), |ui| self.footer(ui, focus));
        match response.inner {
            ErrorDialogAction::None if response.dismissed => ErrorDialogAction::Dismiss,
            action => action,
        }
    }

    /// Paint the dialog in place at the cursor (design boards and previews)
    /// and report the action selected this frame.
    pub fn show_inline(&self, ui: &mut Ui) -> ErrorDialogAction {
        self.dialog().show_inline(
            ui,
            |ui| self.body(ui),
            |ui| self.footer(ui, FocusOnOpen::NEVER),
        )
    }

    fn dialog(&self) -> Dialog<'_> {
        Dialog::new(self.id, Icon::ExclamationCircle, Tone::Danger, self.title)
            .description(self.message)
    }

    fn body(&self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = crate::design::space::S16;
        if let Some(details) = self.details {
            // Long output scrolls, so the footer stays on screen.
            crate::scroll::vertical()
                .max_height(DETAILS_MAX_HEIGHT)
                .show(ui, |ui| {
                    let _ = CodeBlock::new(details).wrap().show(ui);
                });
        }
        if let Some(recovery) = self.recovery {
            type_scale::BODY.label(ui, recovery, ui.tokens().text.secondary);
        }
    }

    fn footer(&self, ui: &mut Ui, focus: FocusOnOpen) -> ErrorDialogAction {
        let primary = self
            .primary_action_label
            .map(|label| Button::primary(label).show(ui));
        let close = Button::secondary(CLOSE_LABEL).show(ui);
        focus.apply(ui, &close);
        if primary.is_some_and(|primary| primary.clicked()) {
            ErrorDialogAction::PrimaryAction
        } else if close.clicked() {
            ErrorDialogAction::Dismiss
        } else {
            ErrorDialogAction::None
        }
    }
}
