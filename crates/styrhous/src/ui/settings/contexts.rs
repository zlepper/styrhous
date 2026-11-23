use super::*;
use crate::context_presentation::{ContextPresentation, automatic_badge};
use crate::ui::state::ContextPresentationSave;
use crate::worker::SaveContextPresentation;
use components::Avatar;
use components::tooltip::elided_label;
use uuid::Uuid;

const CONTEXT_ROW_HEIGHT: f32 = 112.0;
const CONTEXT_AVATAR_SIZE: f32 = 60.0;
const CONTEXT_ACTION_WIDTH: f32 = 96.0;
/// The display name over the context name, [`context_identity`]'s block.
const CONTEXT_ROW_TEXT_HEIGHT: f32 =
    type_scale::TITLE.line_height() + space::S4 + type_scale::MONO.line_height();
const CONTEXT_BADGE_INPUT_WIDTH: f32 = 184.0;
const CONTEXT_PREVIEW_RAIL_HEIGHT: f32 = 112.0;

#[derive(Debug)]
pub(super) struct ContextsSettingsBlade;

fn introduction(ui: &mut egui::Ui, text: &str) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(text)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.secondary),
    );
    ui.add_space(space::S24);
    ui.separator();
    ui.add_space(space::S24 + space::S4);
}

fn supporting_text(ui: &mut egui::Ui, text: &str) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(text)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.muted),
    );
}

fn context_identity(ui: &mut egui::Ui, name: &str, presentation: &ContextPresentation) {
    let theme = ui.tokens();
    ui.spacing_mut().item_spacing.y = space::S4;
    elided_label(
        ui,
        type_scale::TITLE,
        presentation.display_name(name),
        theme.text.primary,
    );
    elided_label(ui, type_scale::MONO, name, theme.text.muted);
}

impl GlobalBladeContent for ContextsSettingsBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        components::blade_title(ui, None, "Contexts");
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let theme = ui.tokens();
        introduction(ui, "Give each context a familiar name and badge.");
        let contexts = context.contexts();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Your contexts")
                    .font(type_scale::TITLE.font_id(ui.ctx()))
                    .color(theme.text.primary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                supporting_text(
                    ui,
                    &format!(
                        "{} context{}",
                        contexts.len(),
                        if contexts.len() == 1 { "" } else { "s" }
                    ),
                );
            });
        });
        ui.add_space(space::S24);
        let mut next_content = None;
        if contexts.is_empty() {
            supporting_text(
                ui,
                "No contexts found. Add a context through cluster discovery.",
            );
            return GlobalBladeRenderResult::default();
        }
        // Keep the title/count above and the guidance below visible as the rows scroll.
        let height = (ui.available_height() - 76.0).max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                components::scroll::vertical()
                    .id_salt("context-settings-list")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        Card::new().padding(0).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = 0.0;
                            for (index, (name, presentation)) in contexts.iter().enumerate() {
                                if index > 0 {
                                    super::destinations::settings_destination_divider(ui);
                                }
                                ui.push_id(name, |ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(ui.available_width(), CONTEXT_ROW_HEIGHT),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_size(egui::vec2(
                                                ui.available_width(),
                                                CONTEXT_ROW_HEIGHT,
                                            ));
                                            ui.spacing_mut().item_spacing.x = 0.0;
                                            ui.add_space(space::S24);
                                            Avatar::new(&presentation.badge(name))
                                                .accent()
                                                .size(CONTEXT_AVATAR_SIZE)
                                                .font(type_scale::HEADING)
                                                .show(ui);
                                            ui.add_space(space::S24);
                                            let text_width = (ui.available_width()
                                                - CONTEXT_ACTION_WIDTH
                                                - space::S24 * 2.0)
                                                .max(0.0);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(text_width, CONTEXT_ROW_TEXT_HEIGHT),
                                                egui::Layout::top_down(egui::Align::Min),
                                                |ui| {
                                                    ui.set_width(text_width);
                                                    context_identity(ui, name, presentation);
                                                },
                                            );
                                            ui.add_space(space::S24);
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(CONTEXT_ACTION_WIDTH, 40.0),
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.set_min_width(CONTEXT_ACTION_WIDTH);
                                                    if Button::secondary("Edit")
                                                        .leading_icon(icons::Icon::Pencil)
                                                        .accessibility_label(format!(
                                                            "Edit context {name}"
                                                        ))
                                                        .show(ui)
                                                        .clicked()
                                                    {
                                                        next_content =
                                                            Some(Box::new(ContextEditorBlade::new(
                                                                (*name).to_owned(),
                                                                (*presentation).clone(),
                                                            ))
                                                                as Box<dyn GlobalBladeContent>);
                                                    }
                                                },
                                            );
                                            ui.add_space(space::S24);
                                        },
                                    );
                                });
                            }
                        });
                    });
            },
        );
        ui.add_space(space::S24);
        supporting_text(ui, "Badges use the first two words of the display name.");
        supporting_text(
            ui,
            "Set a custom badge to make a context easier to recognize.",
        );
        GlobalBladeRenderResult {
            next_content,
            ..Default::default()
        }
    }
}

#[derive(Debug)]
struct ContextEditorBlade {
    context_name: String,
    display_name: String,
    badge: String,
    error: Option<String>,
    request_id: Option<Uuid>,
    effect: Option<ContextEditorEffect>,
}

impl ContextEditorBlade {
    fn new(context_name: String, presentation: ContextPresentation) -> Self {
        Self {
            context_name,
            display_name: presentation.display_name.unwrap_or_default(),
            badge: presentation.badge.unwrap_or_default(),
            error: None,
            request_id: None,
            effect: None,
        }
    }

    fn presentation(&self) -> ContextPresentation {
        ContextPresentation {
            display_name: Some(self.display_name.clone()),
            badge: Some(self.badge.clone()),
        }
    }

    fn form(&mut self, ui: &mut egui::Ui) {
        let theme = ui.tokens();
        Card::new().show_inner(ui, |ui| {
            ui.set_width((ui.available_width()).max(0.0));
            ui.label(
                egui::RichText::new("KUBECONFIG CONTEXT")
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(theme.text.muted),
            );
            ui.add_space(space::S4);
            elided_label(
                ui,
                type_scale::MONO,
                &self.context_name,
                theme.text.secondary,
            );
        });
        ui.add_space(space::S24);
        ui.separator();
        ui.add_space(space::S24);
        ui.label(
            egui::RichText::new("Appearance")
                .font(type_scale::TITLE.font_id(ui.ctx()))
                .color(theme.text.primary),
        );
        ui.add_space(space::S24);
        ui.label("Display name");
        TextInput::new(&mut self.display_name)
            .id_salt(("context-display-name", &self.context_name))
            .accessibility_label("Display name")
            .placeholder(&self.context_name)
            .show(ui);
        ui.add_space(space::S4);
        supporting_text(ui, "Leave blank to use the kubeconfig context name.");
        ui.add_space(space::S24);
        ui.label("Badge");
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(
                    CONTEXT_BADGE_INPUT_WIDTH,
                    components::FieldSize::Md.height(),
                ),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    TextInput::new(&mut self.badge)
                        .id_salt(("context-badge", &self.context_name))
                        .accessibility_label("Badge")
                        .placeholder("Automatic")
                        .show(ui);
                },
            );
            ui.add_space(space::S8);
            supporting_text(ui, &format!("{} / 2", self.badge.trim().chars().count()));
        });
        ui.add_space(space::S4);
        supporting_text(
            ui,
            "One or two characters. Leave blank for automatic initials.",
        );
        ui.add_space(space::S24);
        let presentation =
            self.presentation()
                .normalized()
                .unwrap_or_else(|_| ContextPresentation {
                    display_name: (!self.display_name.trim().is_empty())
                        .then(|| self.display_name.trim().to_owned()),
                    badge: None,
                });
        self.preview(ui, &presentation);
        ui.add_space(space::S8);
        supporting_text(
            ui,
            &format!(
                "Automatic badge: {}",
                automatic_badge(presentation.display_name(&self.context_name))
            ),
        );
        if let Some(error) = &self.error {
            ui.add_space(space::S16);
            super::templates::show_validation_error(ui, "Context settings need attention", error);
        }
    }

    fn footer(ui: &mut egui::Ui, saving: bool) -> (bool, bool) {
        components::blade_body_footer(ui, |ui| {
            let save = Button::primary(if saving { "Saving…" } else { "Save changes" })
                .enabled(!saving)
                .show(ui)
                .clicked();
            let cancel = Button::secondary("Cancel")
                .enabled(!saving)
                .show(ui)
                .clicked();
            (save, cancel)
        })
    }

    fn preview(&self, ui: &mut egui::Ui, presentation: &ContextPresentation) {
        let theme = ui.tokens();
        Card::new().show_inner(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new("PREVIEW")
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(theme.text.muted),
            );
            ui.add_space(space::S12);
            ui.horizontal(|ui| {
                let (rail, _) = ui.allocate_exact_size(
                    egui::vec2(68.0, CONTEXT_PREVIEW_RAIL_HEIGHT),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(
                    rail,
                    egui::CornerRadius::same(radius::KEY),
                    theme.nav.rail,
                );
                let selected = egui::Rect::from_center_size(rail.center(), egui::vec2(52.0, 48.0));
                ui.painter().rect_filled(
                    selected,
                    egui::CornerRadius::same(radius::KEY),
                    theme.nav.item_selected,
                );
                Avatar::new(&presentation.badge(&self.context_name))
                    .identity()
                    .paint(ui, rail.center());
                ui.add_space(space::S16);
                let width = ui.available_width();
                ui.allocate_ui_with_layout(
                    egui::vec2(width, CONTEXT_ROW_TEXT_HEIGHT),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_size(egui::vec2(width, CONTEXT_ROW_TEXT_HEIGHT));
                        context_identity(ui, &self.context_name, presentation);
                    },
                );
            });
        });
    }
}

impl GlobalBladeContent for ContextEditorBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        components::blade_title(ui, None, "Edit context");
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let theme = ui.tokens();
        let status = context.context_presentation_save(&self.context_name);
        let saving = status.is_some_and(|save| save.result.is_none());
        // Reopened editors join the pending save rather than capturing its older values.
        if self.request_id.is_none()
            && let Some(save) = status.filter(|save| save.result.is_none())
        {
            self.display_name = save.presentation.display_name.clone().unwrap_or_default();
            self.badge = save.presentation.badge.clone().unwrap_or_default();
            self.request_id = Some(save.request_id);
        }

        if layer.is_foreground
            && let Some(request_id) = self.request_id
            && let Some(save) = status.filter(|save| save.request_id == request_id)
            && let Some(result) = &save.result
        {
            self.request_id = None;
            match result {
                Ok(()) => self.effect = Some(ContextEditorEffect::Back),
                Err(error) => self.error = Some(error.clone()),
            }
        }
        ui.label(
            egui::RichText::new("Customize how this context appears in your workspace.")
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(theme.text.secondary),
        );
        ui.add_space(space::S24);
        // The form scrolls above the blade footer, before it in keyboard order.
        let form_height = (ui.available_height() - super::FOOTER_HEIGHT).max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), form_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                components::scroll::vertical()
                    .id_salt(("context-editor", &self.context_name))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!saving, |ui| self.form(ui));
                    });
            },
        );
        let (save, cancel) = Self::footer(ui, saving);
        if cancel {
            // Discard cancelled drafts even if the user later navigates Forward.
            if let Some((_, presentation)) = context
                .contexts()
                .into_iter()
                .find(|(name, _)| *name == self.context_name)
            {
                self.display_name = presentation.display_name.clone().unwrap_or_default();
                self.badge = presentation.badge.clone().unwrap_or_default();
            }
            self.error = None;
            self.request_id = None;
            self.effect = Some(ContextEditorEffect::Back);
        } else if save {
            match self.presentation().normalized() {
                Ok(presentation) => {
                    let request_id = Uuid::now_v7();
                    self.request_id = Some(request_id);
                    self.error = None;
                    self.effect = Some(ContextEditorEffect::Save(SaveContextPresentation {
                        request_id,
                        context_name: self.context_name.clone(),
                        presentation,
                    }));
                }
                Err(error) => self.error = Some(error),
            }
        }
        GlobalBladeRenderResult::default()
    }

    fn take_effect(&mut self) -> Option<Box<dyn GlobalBladeEffect>> {
        self.effect
            .take()
            .map(|effect| Box::new(effect) as Box<dyn GlobalBladeEffect>)
    }
}

#[derive(Debug)]
enum ContextEditorEffect {
    Back,
    Save(SaveContextPresentation),
}

impl GlobalBladeEffect for ContextEditorEffect {
    fn apply(
        self: Box<Self>,
        context: &mut GlobalBladeEffectContext<'_>,
        navigation: &mut GlobalBladeNavigation<'_>,
    ) {
        match *self {
            Self::Back => navigation.go_back(),
            Self::Save(command) => {
                context.ui_state.context_presentation_saves.insert(
                    command.context_name.clone(),
                    ContextPresentationSave {
                        request_id: command.request_id,
                        presentation: command.presentation.clone(),
                        result: None,
                    },
                );
                navigation.commands_to_send().push(Box::new(command));
            }
        }
        context.ctx.request_repaint();
    }
}
