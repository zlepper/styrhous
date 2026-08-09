//! Reusable modal treatment for actionable application errors.

use egui::{Align, Color32, Context, Frame, Id, Key, Margin, Modal, Modifiers, Shadow, Stroke};

use crate::colors::{WHITE, gray};
use crate::design::{radius, spacing, status, surface, typography};
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

                if let Some(details) = self.details {
                    ui.add_space(spacing::LG);
                    Frame::new()
                        .fill(gray::_50)
                        .stroke(Stroke::new(1.0, status::DANGER))
                        .corner_radius(radius::control())
                        .inner_margin(Margin::same(spacing::MD as i8))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.label(
                                egui::RichText::new(details)
                                    .font(typography::monospace())
                                    .color(gray::_800),
                            );
                        });
                }

                if let Some(recovery) = self.recovery {
                    ui.add_space(spacing::LG);
                    ui.label(
                        egui::RichText::new(recovery)
                            .font(typography::body())
                            .color(gray::_600),
                    );
                }

                ui.add_space(spacing::XL);
                ui.separator();
                ui.add_space(spacing::MD);
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
