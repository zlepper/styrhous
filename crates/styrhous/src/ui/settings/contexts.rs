use super::*;
use crate::context_presentation::{ContextPresentation, automatic_badge};
use crate::ui::blade_text;
use crate::ui::state::ContextPresentationSave;
use crate::worker::SaveContextPresentation;
use components::Avatar;
use components::design::size;
use components::tooltip::elided_label;
use uuid::Uuid;

const CONTEXT_ROW_HEIGHT: f32 = 112.0;
const CONTEXT_AVATAR_SIZE: f32 = 60.0;
const CONTEXT_ACTION_WIDTH: f32 = 96.0;
/// The display name over the context name, [`context_identity`]'s block.
const CONTEXT_ROW_TEXT_HEIGHT: f32 =
    type_scale::TITLE.line_height() + space::S4 + type_scale::MONO.line_height();
const CONTEXT_BADGE_INPUT_WIDTH: f32 = 96.0;
/// The badge column: the input and room for its help line.
const CONTEXT_BADGE_COLUMN_WIDTH: f32 = 160.0;

#[derive(Debug)]
pub(crate) struct ContextsSettingsBlade;

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
    fn description(&self) -> components::BladeDescription {
        components::BladeDescription::named("Contexts")
    }

    fn sidebar_destination(&self) -> Option<crate::ui::global_blade::SidebarDestination> {
        Some(crate::ui::global_blade::SidebarDestination::Settings)
    }

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
    /// What the context had when the editor opened, to tell what changed.
    original: ContextPresentation,
    display_name: String,
    badge: String,
    colour: Option<u8>,
    error: Option<ContextEditorError>,
    request_id: Option<Uuid>,
    effect: Option<ContextEditorEffect>,
}

/// Why the editor shows a danger callout.
#[derive(Debug)]
struct ContextEditorError {
    title: &'static str,
    message: String,
}

impl ContextEditorBlade {
    fn new(context_name: String, presentation: ContextPresentation) -> Self {
        Self {
            context_name,
            display_name: presentation.display_name.clone().unwrap_or_default(),
            badge: presentation.badge.clone().unwrap_or_default(),
            colour: presentation.colour,
            original: presentation,
            error: None,
            request_id: None,
            effect: None,
        }
    }

    fn presentation(&self) -> ContextPresentation {
        ContextPresentation {
            display_name: Some(self.display_name.clone()),
            badge: Some(self.badge.clone()),
            colour: self.colour,
        }
    }

    /// The save button's words: what it saves when only one thing changed.
    fn save_label(&self, saving: bool) -> &'static str {
        if saving {
            return "Saving…";
        }
        let draft = self.presentation().normalized().ok();
        let original = &self.original;
        let changed = draft.map(|draft| {
            // A badge typed as the automatic one asks for nothing new.
            let automatic = automatic_badge(draft.display_name(&self.context_name));
            let badge_changed =
                draft.badge != original.badge && draft.badge.as_deref() != Some(&automatic);
            [
                draft.display_name != original.display_name,
                badge_changed,
                draft.colour != original.colour,
            ]
        });
        match changed {
            Some([true, false, false]) => "Save display name",
            Some([false, true, false]) => "Save badge",
            Some([false, false, true]) => "Save colour",
            _ => "Save changes",
        }
    }

    fn form(&mut self, ui: &mut egui::Ui, automatic_colour: usize) {
        let theme = ui.tokens();
        ui.spacing_mut().item_spacing.y = 0.0;
        blade_text::blade_caption_with_gap(ui, "Kubeconfig context", space::S6);
        // As tall as the code block, so the Copy button centres on it.
        let row_height = type_scale::MONO_SMALL.line_height() + 12.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), row_height),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = space::S8;
                if Button::ghost("Copy")
                    .size(ButtonSize::Sm)
                    .leading_icon(icons::Icon::ClipboardDocument)
                    .accessibility_label(format!("Copy {}", self.context_name))
                    .show(ui)
                    .clicked()
                {
                    ui.ctx().copy_text(self.context_name.clone());
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    components::code_block::CodeBlock::new(&self.context_name)
                        .compact()
                        .show(ui);
                });
            },
        );
        if let Some(error) = &self.error {
            blade_text::gap(ui, space::S20);
            components::Callout::new(components::design::theme::Tone::Danger, error.title)
                .body(&error.message)
                .show(ui);
        }
        blade_text::gap(ui, space::S24);
        blade_text::blade_section(ui, "Appearance", None);
        blade_text::gap(ui, space::S8);
        FormField::new("Display name")
            .help("Leave blank to use the context name.")
            .show(ui, |ui| {
                TextInput::new(&mut self.display_name)
                    .id_salt(("context-display-name", &self.context_name))
                    .accessibility_label("Display name")
                    .placeholder(&self.context_name)
                    .show(ui);
            });
        blade_text::gap(ui, space::S20);
        let presentation =
            self.presentation()
                .normalized()
                .unwrap_or_else(|_| ContextPresentation {
                    display_name: (!self.display_name.trim().is_empty())
                        .then(|| self.display_name.trim().to_owned()),
                    ..Default::default()
                });
        let automatic_badge = automatic_badge(presentation.display_name(&self.context_name));
        let colours = theme.components.avatars();
        let shown_colour = self.colour.map_or(automatic_colour, usize::from);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 0.0),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.x = space::S24;
                let help = format!("1–2 characters. Automatic: {automatic_badge}");
                ui.allocate_ui_with_layout(
                    egui::vec2(CONTEXT_BADGE_COLUMN_WIDTH, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(CONTEXT_BADGE_COLUMN_WIDTH);
                        FormField::new("Badge").help(&help).show(ui, |ui| {
                            TextInput::new(&mut self.badge)
                                .width(CONTEXT_BADGE_INPUT_WIDTH)
                                .id_salt(("context-badge", &self.context_name))
                                .accessibility_label("Badge")
                                .placeholder("Automatic")
                                .show(ui);
                        });
                    },
                );
                let help = if self.colour.is_some() {
                    "Click the chosen colour again for automatic"
                } else {
                    "Automatic: follows the context's place in the list"
                };
                FormField::new("Colour").help(help).show(ui, |ui| {
                    if let Some(clicked) =
                        components::Swatches::new(&colours, Some(shown_colour), "Colour").show(ui)
                    {
                        // Choosing the chosen colour again returns to automatic.
                        self.colour = (self.colour != Some(clicked as u8)).then_some(clicked as u8);
                    }
                });
            },
        );
        blade_text::gap(ui, space::S20);
        self.preview(ui, &presentation, shown_colour);
    }

    fn footer(&self, ui: &mut egui::Ui, saving: bool) -> (bool, bool) {
        components::blade_body_footer(ui, |ui| {
            let save = Button::primary(self.save_label(saving))
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

    /// How the context will look in the cluster card (A09 "Preview").
    fn preview(&self, ui: &mut egui::Ui, presentation: &ContextPresentation, colour: usize) {
        let theme = ui.tokens();
        blade_text::blade_caption_with_gap(ui, "Preview", space::S6);
        let height = size::AVATAR + space::S12 * 2.0;
        let (row, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(row, egui::CornerRadius::same(radius::CARD), theme.nav.rail);
        let avatar = egui::Rect::from_min_size(
            row.min + egui::vec2(space::S16, space::S12),
            egui::Vec2::splat(size::AVATAR),
        );
        Avatar::new(&presentation.badge(&self.context_name))
            .category(colour)
            .status(components::design::theme::Tone::Success)
            .selected(true)
            .paint(ui, avatar.center());
        let text_left = avatar.right() + space::S12;
        let name_role = type_scale::UI_STRONG;
        let mono_role = type_scale::MONO.with_size(11.0, 16.0);
        let top = row.center().y - (name_role.line_height() + mono_role.line_height()) / 2.0;
        for (role, text, colour, top) in [
            (
                name_role,
                presentation.display_name(&self.context_name),
                theme.text.primary,
                top,
            ),
            (
                mono_role,
                self.context_name.as_str(),
                theme.nav.text_muted,
                top + name_role.line_height(),
            ),
        ] {
            let rect = egui::Rect::from_min_size(
                egui::pos2(text_left, top),
                egui::vec2(
                    (row.right() - space::S16 - text_left).max(0.0),
                    role.line_height(),
                ),
            );
            let mut line = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            elided_label(&mut line, role, text, colour);
        }
    }
}

impl GlobalBladeContent for ContextEditorBlade {
    fn description(&self) -> components::BladeDescription {
        components::BladeDescription::named("Edit context")
    }

    fn sidebar_destination(&self) -> Option<crate::ui::global_blade::SidebarDestination> {
        Some(crate::ui::global_blade::SidebarDestination::Settings)
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        components::blade_title_with_actions(ui, None, "Edit context", |ui| {
            blade_text::blade_more_actions(ui);
        });
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
            self.colour = save.presentation.colour;
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
                Err(error) => {
                    self.error = Some(ContextEditorError {
                        title: "Changes can’t be saved",
                        message: error.clone(),
                    });
                }
            }
        }
        // A context without a chosen colour wears the one of its place.
        // The sidebar counts contexts by lower-cased name, then key.
        let mut names = context
            .contexts()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        names.sort_by_cached_key(|name| name.to_lowercase());
        let automatic_colour = names
            .iter()
            .position(|name| *name == self.context_name)
            .unwrap_or_default();
        let automatic_colour = crate::context_presentation::automatic_colour(automatic_colour);
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
                        ui.add_enabled_ui(!saving, |ui| self.form(ui, automatic_colour));
                    });
            },
        );
        let (save, cancel) = self.footer(ui, saving);
        if cancel {
            // Discard cancelled drafts even if the user later navigates Forward.
            if let Some((_, presentation)) = context
                .contexts()
                .into_iter()
                .find(|(name, _)| *name == self.context_name)
            {
                self.display_name = presentation.display_name.clone().unwrap_or_default();
                self.badge = presentation.badge.clone().unwrap_or_default();
                self.colour = presentation.colour;
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
                Err(message) => {
                    self.error = Some(ContextEditorError {
                        title: "Context settings need attention",
                        message,
                    });
                }
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
