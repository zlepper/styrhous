//! Reusable modal treatment for user-confirmed application actions.

use egui::{Align, Context, Frame, Id, Key, Margin, Modal, Modifiers};

use crate::design::theme::{self, Theme, ThemeExt, ThemeScope};
use crate::design::{radius, size, space, stroke, type_scale};
use crate::frames::overlay_frame;
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
    /// Optional visually prominent warning rendered within the shared dialog treatment.
    pub warning: Option<ConfirmationDialogWarning<'a>>,
    /// Optional typed acknowledgement shown before the action buttons.
    pub acknowledgement: Option<ConfirmationDialogAcknowledgement<'a>>,
}

/// A typed acknowledgement required before a confirmation may proceed.
pub struct ConfirmationDialogAcknowledgement<'a> {
    pub label: &'a str,
    pub hint_text: &'a str,
    pub value: &'a mut String,
}

/// A high-risk action warning, optionally with a bounded detail list.
pub struct ConfirmationDialogWarning<'a> {
    pub title: &'a str,
    pub message: &'a str,
    pub details: Option<&'a str>,
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
    pub fn show(self, ctx: &Context) -> ConfirmationDialogAction {
        self.show_with_body(ctx, DIALOG_WIDTH, |_| {})
    }

    /// Show the dialog `width` points wide, with `body` rendered below the
    /// message for content the standard fields cannot express.
    pub fn show_with_body(
        mut self,
        ctx: &Context,
        width: f32,
        body: impl FnOnce(&mut egui::Ui),
    ) -> ConfirmationDialogAction {
        let mut action = ConfirmationDialogAction::None;
        let response = dialog_modal(ctx, self.id, width).show(ctx, |ui| {
            theme::scope(ui, ThemeScope::Dialog, |ui| {
                let tokens = ui.tokens();
                ui.set_width(width);
                show_dialog_header(ui, self.eyebrow, self.title, self.message);
                body(ui);
                if let Some(warning) = self.warning {
                    ui.add_space(space::S16);
                    danger_callout_frame(&tokens).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(warning.title)
                                .font(type_scale::BODY_STRONG.font_id(ui.ctx()))
                                .color(tokens.danger.text),
                        );
                        ui.add_space(space::S4);
                        ui.label(
                            egui::RichText::new(warning.message)
                                .font(type_scale::BODY.font_id(ui.ctx()))
                                .color(tokens.danger.text_on_tint),
                        );
                        if let Some(details) = warning.details {
                            ui.add_space(space::S8);
                            egui::ScrollArea::vertical()
                                .max_height(96.0)
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.label(
                                        egui::RichText::new(details)
                                            .font(type_scale::MONO.font_id(ui.ctx()))
                                            .color(tokens.danger.text_on_tint),
                                    );
                                });
                        }
                    });
                }
                if let Some(message) = self.unavailable_message {
                    ui.add_space(space::S8);
                    ui.label(
                        egui::RichText::new(message)
                            .font(type_scale::META.font_id(ui.ctx()))
                            .color(tokens.text.muted),
                    );
                }
                if let Some(acknowledgement) = &mut self.acknowledgement {
                    ui.add_space(space::S16);
                    let label = ui.label(
                        egui::RichText::new(acknowledgement.label)
                            .font(type_scale::META.font_id(ui.ctx()))
                            .color(tokens.text.secondary),
                    );
                    ui.add_space(space::S4);
                    ui.add(
                        egui::TextEdit::singleline(acknowledgement.value)
                            .id_salt("acknowledgement")
                            .desired_width(f32::INFINITY)
                            .hint_text(acknowledgement.hint_text)
                            .margin(size::FIELD_MARGIN_SM),
                    )
                    .labelled_by(label.id);
                }

                ui.add_space(space::S24);
                ui.separator();
                ui.add_space(space::S12);
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
            })
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

/// The modal of a standard application dialog (`.dlg`): `surface-panel`
/// with the overlay shadow and its hairline ring, over the scrim. Show its
/// contents in [`ThemeScope::Dialog`].
pub fn dialog_modal(ctx: &Context, id: Id, width: f32) -> Modal {
    let tokens = ctx.tokens();
    Modal::new(id)
        .area(Modal::default_area(id).default_width(width).fade_in(false))
        .backdrop_color(tokens.shade.scrim)
        .frame(overlay_frame(&tokens).inner_margin(Margin::same(space::S24 as i8)))
}

/// The eyebrow, title and message that open every application dialog.
pub fn show_dialog_header(ui: &mut egui::Ui, eyebrow: &str, title: &str, message: &str) {
    let tokens = ui.tokens();
    ui.label(
        egui::RichText::new(eyebrow)
            .font(type_scale::META.font_id(ui.ctx()))
            .color(tokens.text.muted),
    );
    ui.add_space(space::S8);
    ui.label(
        egui::RichText::new(title)
            .font(type_scale::DIALOG_TITLE.font_id(ui.ctx()))
            .color(tokens.text.primary),
    );
    ui.add_space(space::S12);
    ui.label(
        egui::RichText::new(message)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(tokens.text.secondary),
    );
}

/// A danger banner (`.banner.bad`) for warnings and failure details inside a
/// dialog. Text on it uses `danger-text` for titles and
/// `danger-text-on-tint` for body copy.
pub(crate) fn danger_callout_frame(theme: &Theme) -> Frame {
    Frame::new()
        .fill(theme.danger.tint)
        .stroke(stroke::hairline(theme.danger.outline))
        .corner_radius(egui::CornerRadius::same(radius::KEY))
        .inner_margin(Margin::same(space::S12 as i8))
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
                    eyebrow: "REMOVE FINALIZERS",
                    title: "Force delete resource?",
                    message: "This removes every finalizer from the resource.",
                    unavailable_message: (!state.available)
                        .then_some("Remove finalizers will be available in 1 second."),
                    cancel_label: "Cancel",
                    confirm_label: "Remove finalizers",
                    kind: ConfirmationDialogKind::Destructive,
                    confirm_enabled: state.available && state.acknowledgement == "resource-name",
                    warning: None,
                    acknowledgement: Some(ConfirmationDialogAcknowledgement {
                        label: "Type the resource name to acknowledge:",
                        hint_text: "Resource name",
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
