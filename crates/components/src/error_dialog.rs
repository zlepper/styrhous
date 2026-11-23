//! Reusable modal treatment for actionable application errors.

use egui::{Align, Context, Id, Key, Modifiers};

use crate::confirmation_dialog::{danger_callout_frame, dialog_modal, show_dialog_header};
use crate::design::theme::{self, ThemeExt, ThemeScope};
use crate::design::{space, type_scale};
use crate::{ButtonSize, TailwindButton};

const DIALOG_WIDTH: f32 = 480.0;

/// Content and actions for a standard application error dialog.
pub struct ErrorDialog<'a> {
    /// Stable identity for the dialog and its input-blocking scrim.
    pub id: Id,
    /// Small contextual label shown above the title.
    pub eyebrow: &'a str,
    /// Clear description of the failed user-visible operation.
    pub title: &'a str,
    /// Brief explanation of the failure.
    pub message: &'a str,
    /// Optional technical details for diagnosing the failure.
    pub details: Option<&'a str>,
    /// Optional next-step guidance.
    pub recovery: Option<&'a str>,
    /// Optional primary action label. The caller handles the action result.
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
    /// Show the modal error dialog and report the action selected this frame.
    pub fn show(&self, ctx: &Context) -> ErrorDialogAction {
        let mut action = ErrorDialogAction::None;
        let response = dialog_modal(ctx, self.id, DIALOG_WIDTH).show(ctx, |ui| {
            theme::scope(ui, ThemeScope::Dialog, |ui| {
                let tokens = ui.tokens();
                ui.set_width(DIALOG_WIDTH);
                show_dialog_header(ui, self.eyebrow, self.title, self.message);

                if let Some(details) = self.details {
                    ui.add_space(space::S16);
                    danger_callout_frame(&tokens).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(details)
                                .font(type_scale::MONO.font_id(ui.ctx()))
                                .color(tokens.danger.text_on_tint),
                        );
                    });
                }

                if let Some(recovery) = self.recovery {
                    ui.add_space(space::S16);
                    ui.label(
                        egui::RichText::new(recovery)
                            .font(type_scale::BODY.font_id(ui.ctx()))
                            .color(tokens.text.secondary),
                    );
                }

                ui.add_space(space::S24);
                ui.separator();
                ui.add_space(space::S12);
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if let Some(label) = self.primary_action_label
                        && TailwindButton::new(label)
                            .size(ButtonSize::Md)
                            .show(ui)
                            .clicked()
                    {
                        action = ErrorDialogAction::PrimaryAction;
                    }
                    if TailwindButton::secondary("Dismiss")
                        .size(ButtonSize::Md)
                        .show(ui)
                        .clicked()
                    {
                        action = ErrorDialogAction::Dismiss;
                    }
                });
            })
        });

        let escape_pressed = response.is_top_modal
            && !response.any_popup_open
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
        if escape_pressed && action == ErrorDialogAction::None {
            ErrorDialogAction::Dismiss
        } else {
            action
        }
    }
}
