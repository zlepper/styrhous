//! Reusable modal treatment for user-confirmed application actions.

use egui::{Align, Color32, Context, Frame, Id, Key, Margin, Modal, Modifiers, Shadow};

use crate::colors::{WHITE, gray};
use crate::design::{radius, spacing, surface, typography};
use crate::{ButtonSize, TailwindButton};

const DIALOG_WIDTH: f32 = 480.0;

/// Visual emphasis for a confirmation's primary action.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConfirmationDialogKind {
    Primary,
    Destructive,
}

/// Content and actions for a standard application confirmation dialog.
pub struct ConfirmationDialog<'a> {
    /// Stable identity for the dialog and its input-blocking scrim.
    pub id: Id,
    /// Small contextual label shown above the title.
    pub eyebrow: &'a str,
    /// Clear description of the pending action.
    pub title: &'a str,
    /// Brief explanation of the action's effect.
    pub message: &'a str,
    /// Optional explanation displayed while the primary action is unavailable.
    pub unavailable_message: Option<&'a str>,
    /// Label for the safe dismiss action.
    pub cancel_label: &'a str,
    /// Label for the action that confirms the operation.
    pub confirm_label: &'a str,
    /// Semantic emphasis for the confirmation action.
    pub kind: ConfirmationDialogKind,
    /// Whether the confirmation action may be selected now.
    pub confirm_enabled: bool,
}

/// User action selected from a [`ConfirmationDialog`].
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConfirmationDialogAction {
    None,
    Cancel,
    Confirm,
}

impl ConfirmationDialog<'_> {
    /// Show the modal dialog and report the selected action for this frame.
    pub fn show(&self, ctx: &Context) -> ConfirmationDialogAction {
        let mut action = ConfirmationDialogAction::None;
        let response = Modal::new(self.id)
            .area(
                Modal::default_area(self.id)
                    .default_width(DIALOG_WIDTH)
                    .fade_in(false),
            )
            .backdrop_color(Color32::from_black_alpha(122))
            .frame(
                Frame::new()
                    .fill(WHITE)
                    .stroke(surface::muted_border())
                    .corner_radius(radius::surface())
                    .shadow(Shadow {
                        offset: [0, 4],
                        blur: 18,
                        spread: 0,
                        color: Color32::BLACK.gamma_multiply(0.16),
                    })
                    .inner_margin(Margin::same(spacing::XL as i8)),
            )
            .show(ctx, |ui| {
                ui.set_width(DIALOG_WIDTH);
                ui.label(
                    egui::RichText::new(self.eyebrow)
                        .font(typography::metadata())
                        .color(gray::_500),
                );
                ui.add_space(spacing::SM);
                ui.label(
                    egui::RichText::new(self.title)
                        .font(typography::semibold(24.0))
                        .color(gray::_900),
                );
                ui.add_space(spacing::MD);
                ui.label(
                    egui::RichText::new(self.message)
                        .font(typography::body())
                        .color(gray::_600),
                );
                if let Some(message) = self.unavailable_message {
                    ui.add_space(spacing::SM);
                    ui.label(
                        egui::RichText::new(message)
                            .font(typography::metadata())
                            .color(gray::_500),
                    );
                }

                ui.add_space(spacing::XL);
                ui.separator();
                ui.add_space(spacing::MD);
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let confirm = match self.kind {
                        ConfirmationDialogKind::Primary => {
                            TailwindButton::primary(self.confirm_label)
                        }
                        ConfirmationDialogKind::Destructive => {
                            TailwindButton::danger(self.confirm_label)
                        }
                    }
                    .size(ButtonSize::Md);
                    if ui
                        .add_enabled_ui(self.confirm_enabled, |ui| confirm.show(ui))
                        .inner
                        .clicked()
                    {
                        action = ConfirmationDialogAction::Confirm;
                    }
                    if TailwindButton::secondary(self.cancel_label)
                        .size(ButtonSize::Md)
                        .show(ui)
                        .clicked()
                    {
                        action = ConfirmationDialogAction::Cancel;
                    }
                });
            });

        let escape_pressed = response.is_top_modal
            && !response.any_popup_open
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
        if escape_pressed && action == ConfirmationDialogAction::None {
            ConfirmationDialogAction::Cancel
        } else {
            action
        }
    }
}
