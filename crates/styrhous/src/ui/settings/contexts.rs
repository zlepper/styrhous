use super::*;
use crate::context_presentation::{ContextPresentation, automatic_badge};
use crate::ui::state::ContextPresentationSave;
use crate::worker::SaveContextPresentation;
use components::TextAvatar;
use uuid::Uuid;

const CONTEXT_ROW_HEIGHT: f32 = 112.0;
const CONTEXT_AVATAR_SIZE: f32 = 60.0;
const CONTEXT_ACTION_WIDTH: f32 = 96.0;
const CONTEXT_ROW_TEXT_HEIGHT: f32 = 44.0;
const CONTEXT_BADGE_INPUT_WIDTH: f32 = 184.0;
const CONTEXT_PREVIEW_RAIL_HEIGHT: f32 = 112.0;

#[derive(Debug)]
pub(super) struct ContextsSettingsBlade;

fn header(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .font(typography::page_title())
            .color(gray::_900),
    );
}

fn introduction(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(typography::body())
            .color(gray::_600),
    );
    ui.add_space(spacing::XL);
    ui.separator();
    ui.add_space(spacing::XL + spacing::XS);
}

fn supporting_text(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(typography::body())
            .color(gray::_500),
    );
}

fn context_identity(ui: &mut egui::Ui, name: &str, presentation: &ContextPresentation) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(presentation.display_name(name))
                .font(typography::semibold(typography::SECTION_SIZE))
                .color(gray::_900),
        )
        .truncate(),
    )
    .on_hover_text(presentation.display_name(name));
    ui.add(
        egui::Label::new(
            egui::RichText::new(name)
                .font(typography::monospace())
                .color(gray::_500),
        )
        .truncate(),
    )
    .on_hover_text(name);
}

impl GlobalBladeContent for ContextsSettingsBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        header(ui, "Contexts");
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        introduction(ui, "Give each context a familiar name and badge.");
        let contexts = context.contexts();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Your contexts")
                    .font(typography::semibold(typography::SECTION_SIZE))
                    .color(gray::_900),
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
        ui.add_space(spacing::XL);
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
                        egui::Frame::new()
                            .stroke(surface::muted_border())
                            .corner_radius(radius::surface())
                            .show(ui, |ui| {
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
                                                ui.add_space(spacing::XL);
                                                TextAvatar::new(&presentation.badge(name))
                                                    .accent()
                                                    .size(CONTEXT_AVATAR_SIZE)
                                                    .font(typography::semibold(
                                                        typography::PAGE_TITLE_SIZE,
                                                    ))
                                                    .show(ui);
                                                ui.add_space(spacing::XL);
                                                let text_width = (ui.available_width()
                                                    - CONTEXT_ACTION_WIDTH
                                                    - spacing::XL * 2.0)
                                                    .max(0.0);
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(text_width, CONTEXT_ROW_TEXT_HEIGHT),
                                                    egui::Layout::top_down(egui::Align::Min),
                                                    |ui| {
                                                        ui.set_width(text_width);
                                                        ui.spacing_mut().item_spacing.y =
                                                            spacing::XS;
                                                        context_identity(ui, name, presentation);
                                                    },
                                                );
                                                ui.add_space(spacing::XL);
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(CONTEXT_ACTION_WIDTH, 40.0),
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.set_min_width(CONTEXT_ACTION_WIDTH);
                                                        if TailwindButton::image_and_text(
                                                            icons::pencil_icon(),
                                                            "Edit",
                                                        )
                                                        .variant(ButtonVariant::Secondary)
                                                        .accessibility_label(format!(
                                                            "Edit context {name}"
                                                        ))
                                                        .show(ui)
                                                        .clicked()
                                                        {
                                                            next_content = Some(Box::new(
                                                                ContextEditorBlade::new(
                                                                    (*name).to_owned(),
                                                                    (*presentation).clone(),
                                                                ),
                                                            )
                                                                as Box<dyn GlobalBladeContent>);
                                                        }
                                                    },
                                                );
                                                ui.add_space(spacing::XL);
                                            },
                                        );
                                    });
                                }
                            });
                    });
            },
        );
        ui.add_space(spacing::XL);
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
    footer_height: Option<f32>,
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
            footer_height: None,
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
        egui::Frame::new()
            .fill(gray::_50)
            .corner_radius(radius::surface())
            .inner_margin(spacing::LG)
            .show(ui, |ui| {
                ui.set_width((ui.available_width()).max(0.0));
                ui.label(
                    egui::RichText::new("KUBECONFIG CONTEXT")
                        .font(typography::metadata())
                        .color(gray::_500),
                );
                ui.add_space(spacing::XS);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&self.context_name)
                            .font(typography::monospace())
                            .color(gray::_600),
                    )
                    .truncate(),
                )
                .on_hover_text(&self.context_name);
            });
        ui.add_space(spacing::XL);
        ui.separator();
        ui.add_space(spacing::XL);
        ui.label(
            egui::RichText::new("Appearance")
                .font(typography::semibold(typography::SECTION_SIZE))
                .color(gray::_900),
        );
        ui.add_space(spacing::XL);
        ui.label("Display name");
        TailwindTextInput::new(&mut self.display_name)
            .id_salt(("context-display-name", &self.context_name))
            .accessibility_label("Display name")
            .hint_text(&self.context_name)
            .show(ui);
        ui.add_space(spacing::XS);
        supporting_text(ui, "Leave blank to use the kubeconfig context name.");
        ui.add_space(spacing::XL);
        ui.label("Badge");
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(CONTEXT_BADGE_INPUT_WIDTH, 28.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    TailwindTextInput::new(&mut self.badge)
                        .id_salt(("context-badge", &self.context_name))
                        .accessibility_label("Badge")
                        .hint_text("Automatic")
                        .show(ui);
                },
            );
            ui.add_space(spacing::SM);
            supporting_text(ui, &format!("{} / 2", self.badge.trim().chars().count()));
        });
        ui.add_space(spacing::XS);
        supporting_text(
            ui,
            "One or two characters. Leave blank for automatic initials.",
        );
        ui.add_space(spacing::XL);
        let presentation =
            self.presentation()
                .normalized()
                .unwrap_or_else(|_| ContextPresentation {
                    display_name: (!self.display_name.trim().is_empty())
                        .then(|| self.display_name.trim().to_owned()),
                    badge: None,
                });
        self.preview(ui, &presentation);
        ui.add_space(spacing::SM);
        supporting_text(
            ui,
            &format!(
                "Automatic badge: {}",
                automatic_badge(presentation.display_name(&self.context_name))
            ),
        );
        if let Some(error) = &self.error {
            ui.add_space(spacing::LG);
            super::templates::show_validation_error(ui, "Context settings need attention", error);
        }
    }

    fn footer(ui: &mut egui::Ui, saving: bool) -> (bool, bool) {
        ui.separator();
        ui.add_space(spacing::SM);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 0.0),
            egui::Layout::right_to_left(egui::Align::Min),
            |ui| {
                let save = ui
                    .add_enabled_ui(!saving, |ui| {
                        TailwindButton::new(if saving { "Saving…" } else { "Save changes" })
                            .show(ui)
                    })
                    .inner
                    .clicked();
                let cancel = ui
                    .add_enabled_ui(!saving, |ui| TailwindButton::secondary("Cancel").show(ui))
                    .inner
                    .clicked();
                (save, cancel)
            },
        )
        .inner
    }

    fn preview(&self, ui: &mut egui::Ui, presentation: &ContextPresentation) {
        egui::Frame::new()
            .fill(gray::_50)
            .stroke(surface::muted_border())
            .corner_radius(radius::surface())
            .inner_margin(spacing::LG)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new("PREVIEW")
                        .font(typography::metadata())
                        .color(gray::_500),
                );
                ui.add_space(spacing::MD);
                ui.horizontal(|ui| {
                    let (rail, _) = ui.allocate_exact_size(
                        egui::vec2(68.0, CONTEXT_PREVIEW_RAIL_HEIGHT),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(
                        rail,
                        radius::subtle(),
                        components::colors::CLUSTER_RAIL_BACKGROUND,
                    );
                    let selected =
                        egui::Rect::from_center_size(rail.center(), egui::vec2(52.0, 48.0));
                    ui.painter()
                        .rect_filled(selected, radius::control(), indigo::_600);
                    TextAvatar::new(&presentation.badge(&self.context_name))
                        .dark()
                        .paint(ui, rail.center());
                    ui.add_space(spacing::LG);
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
        header(ui, "Edit context");
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
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
                .font(typography::body())
                .color(gray::_600),
        );
        ui.add_space(spacing::XL);
        // egui resolves the footer's intrinsic size in a layout pass. Reuse the
        // measured size so the live form stays before the actions in keyboard order.
        let footer_height = self.footer_height.unwrap_or(0.0);
        let form_height =
            (ui.available_height() - footer_height - ui.spacing().item_spacing.y).max(0.0);
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
        let footer = ui.scope(|ui| Self::footer(ui, saving));
        let measured_height = footer.response.rect.height();
        if self.footer_height != Some(measured_height) {
            self.footer_height = Some(measured_height);
            ui.ctx()
                .request_discard("context editor footer size changed");
            return GlobalBladeRenderResult::default();
        }
        let (save, cancel) = footer.inner;
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
