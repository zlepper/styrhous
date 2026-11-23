use super::destinations::{
    SettingsDestination, settings_destination_card, settings_destination_group,
};
use super::terminal::{
    show_debug_image_presets, show_settings_introduction, show_terminal_launcher,
    show_update_status,
};
use super::*;

#[derive(Debug, Default)]

pub(crate) struct SettingsHomeBlade;

impl GlobalBladeContent for SettingsHomeBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        components::blade_title(ui, None, "Settings");
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let theme = ui.tokens();
        ui.label(
            egui::RichText::new("Manage your workspace and application preferences.")
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(theme.text.secondary),
        );
        ui.add_space(space::S24);
        ui.separator();
        ui.add_space(space::S24 + space::S4);
        let context_count = context.contexts().len();
        let count = format!(
            "{context_count} context{}",
            if context_count == 1 { "" } else { "s" }
        );
        let mut contexts = false;
        let mut namespace_selector = false;
        let mut cluster_discovery = false;
        let mut application_settings = false;
        let mut license = false;
        let licensing = context.licensing();
        let license_description = format!(
            "Manage your license and account. {}",
            licensing.settings().server.label()
        );
        let license_status = licensing.status().short_label();
        components::scroll::vertical()
            .id_salt("settings-destinations")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                settings_destination_group(ui, "KUBERNETES", |group| {
                    contexts = settings_destination_card(
                        group,
                        SettingsDestination {
                            title: "Contexts",
                            label: "Open context settings",
                            description: "Customize display names and selector badges.",
                            icon: icons::Icon::CircleStack,
                            detail: &count,
                        },
                    );
                    namespace_selector = settings_destination_card(
                        group,
                        SettingsDestination {
                            title: "Namespace selector",
                            label: "Open namespace selector settings",
                            description: "Choose how namespaces are displayed.",
                            icon: icons::Icon::AdjustmentsHorizontal,
                            detail: "",
                        },
                    );
                    cluster_discovery = settings_destination_card(
                        group,
                        SettingsDestination {
                            title: "Cluster discovery",
                            label: "Open cluster discovery",
                            description: "Find and add clusters from your cloud accounts.",
                            icon: icons::Icon::Share,
                            detail: "",
                        },
                    );
                });
                ui.add_space(space::S32 + space::S8);
                settings_destination_group(ui, "APPLICATION", |group| {
                    application_settings = settings_destination_card(
                        group,
                        SettingsDestination {
                            title: "Application preferences",
                            label: "Open application settings",
                            description: "Terminal, debug images, and updates.",
                            icon: icons::Icon::WrenchScrewdriver,
                            detail: "",
                        },
                    );
                    license = settings_destination_card(
                        group,
                        SettingsDestination {
                            title: "License & account",
                            label: "Open license and account settings",
                            description: &license_description,
                            icon: icons::Icon::UserCircle,
                            detail: license_status,
                        },
                    );
                });
            });
        GlobalBladeRenderResult {
            next_content: contexts
                .then(|| Box::new(ContextsSettingsBlade) as Box<dyn GlobalBladeContent>)
                .or_else(|| {
                    application_settings.then(|| {
                        Box::new(TerminalSettingsBlade::new(
                            context.terminal_launch_settings().clone(),
                        )) as Box<dyn GlobalBladeContent>
                    })
                })
                .or_else(|| {
                    license.then(|| {
                        Box::new(LicenseSettingsBlade::default()) as Box<dyn GlobalBladeContent>
                    })
                })
                .or_else(|| {
                    namespace_selector.then(|| {
                        Box::new(NamespaceSelectorSettingsBlade::new(
                            context.namespace_selector_settings().clone(),
                        )) as Box<dyn GlobalBladeContent>
                    })
                })
                .or_else(|| {
                    cluster_discovery.then(|| {
                        Box::new(ManagedClusterDiscoveryBlade::default())
                            as Box<dyn GlobalBladeContent>
                    })
                }),
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub(crate) struct TerminalSettingsBlade {
    pub(crate) draft: TerminalLaunchSettings,
    pub(crate) error: Option<String>,
    /// Saving returns to the blade this one was opened over (an inspector)
    /// instead of closing the blades.
    back_on_save: bool,
    /// Saved; go back on the next effect pass.
    go_back: bool,
}

impl TerminalSettingsBlade {
    pub(crate) fn new(draft: TerminalLaunchSettings) -> Self {
        Self {
            draft,
            error: None,
            back_on_save: false,
            go_back: false,
        }
    }

    /// Settings opened over another blade (an inspector's "Configure
    /// presets…"): saving returns to it.
    pub(crate) fn returning_on_save(self) -> Self {
        Self {
            back_on_save: true,
            ..self
        }
    }
}

/// Go back to the previous blade.
#[derive(Debug)]
struct GoBack;

impl GlobalBladeEffect for GoBack {
    fn apply(
        self: Box<Self>,
        _context: &mut GlobalBladeEffectContext<'_>,
        navigation: &mut GlobalBladeNavigation<'_>,
    ) {
        navigation.go_back();
    }
}

impl GlobalBladeContent for TerminalSettingsBlade {
    #[cfg(test)]
    fn terminal_settings(&self) -> Option<&TerminalSettingsBlade> {
        Some(self)
    }

    fn take_effect(&mut self) -> Option<Box<dyn GlobalBladeEffect>> {
        std::mem::take(&mut self.go_back).then(|| Box::new(GoBack) as Box<dyn GlobalBladeEffect>)
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        components::blade_title(ui, None, "Application settings");
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let mut save = false;
        let mut reset = false;
        show_settings_introduction(ui);
        show_update_status(ui, context.update_status());
        ui.add_space(space::S24);
        ui.separator();
        ui.add_space(space::S24);
        // The form scrolls in whatever room the blade footer leaves.
        let content_height = (ui.available_height() - FOOTER_HEIGHT).max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), content_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                components::scroll::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        show_terminal_launcher(ui, &mut self.draft, &mut self.error);
                        ui.add_space(space::S24);
                        ui.separator();
                        ui.add_space(space::S24);
                        show_debug_image_presets(ui, &mut self.draft, &mut self.error);
                    });
            },
        );
        components::blade_body_footer(ui, |ui| {
            save |= Button::primary("Save changes").show(ui).clicked();
            reset |= Button::secondary("Reset").show(ui).clicked();
        });
        if reset {
            self.draft = TerminalLaunchSettings::default();
            self.error = None;
        }
        if save {
            match self.draft.validate() {
                Ok(()) => {
                    *context.terminal_launch_settings() = self.draft.clone();
                    self.error = None;
                    if self.back_on_save {
                        self.go_back = true;
                        return GlobalBladeRenderResult::default();
                    }
                    return GlobalBladeRenderResult {
                        close: true,
                        ..Default::default()
                    };
                }
                Err(error) => self.error = Some(error),
            }
        }
        GlobalBladeRenderResult::default()
    }
}
