use super::destinations::{
    SettingsDestination, settings_destination_card, settings_destination_divider,
    settings_destination_group,
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
        ui.label(
            egui::RichText::new("Settings")
                .font(typography::page_title())
                .color(gray::_900),
        );
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        ui.label(
            egui::RichText::new("Manage your workspace and application preferences.")
                .font(typography::body())
                .color(gray::_600),
        );
        ui.add_space(spacing::XL);
        ui.separator();
        ui.add_space(spacing::XL + spacing::XS);
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
        let license_status = match licensing.status() {
            crate::licensing::LicenseStatus::Licensed(_) => "Active",
            crate::licensing::LicenseStatus::Offline(_) => "Offline",
            crate::licensing::LicenseStatus::Checking => "Checking…",
            crate::licensing::LicenseStatus::AwaitingApproval(_) => "Pending",
            _ => "Needs attention",
        };
        components::scroll::vertical()
            .id_salt("settings-destinations")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                settings_destination_group(ui, "KUBERNETES", |ui| {
                    contexts = settings_destination_card(
                        ui,
                        SettingsDestination {
                            title: "Contexts",
                            label: "Open context settings",
                            description: "Customize display names and selector badges.",
                            icon: icons::settings_destination_contexts_icon(),
                            detail: &count,
                        },
                    );
                    settings_destination_divider(ui);
                    namespace_selector = settings_destination_card(
                        ui,
                        SettingsDestination {
                            title: "Namespace selector",
                            label: "Open namespace selector settings",
                            description: "Choose how namespaces are displayed.",
                            icon: icons::settings_destination_namespace_icon(),
                            detail: "",
                        },
                    );
                    settings_destination_divider(ui);
                    cluster_discovery = settings_destination_card(
                        ui,
                        SettingsDestination {
                            title: "Cluster discovery",
                            label: "Open cluster discovery",
                            description: "Find and add clusters from your cloud accounts.",
                            icon: icons::settings_destination_discovery_icon(),
                            detail: "",
                        },
                    );
                });
                ui.add_space(spacing::XXL + spacing::SM);
                settings_destination_group(ui, "APPLICATION", |ui| {
                    application_settings = settings_destination_card(
                        ui,
                        SettingsDestination {
                            title: "Application preferences",
                            label: "Open application settings",
                            description: "Terminal, debug images, and updates.",
                            icon: icons::settings_destination_application_icon(),
                            detail: "",
                        },
                    );
                    settings_destination_divider(ui);
                    license = settings_destination_card(
                        ui,
                        SettingsDestination {
                            title: "License & account",
                            label: "Open license and account settings",
                            description: &license_description,
                            icon: icons::settings_destination_account_icon(),
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
}

impl TerminalSettingsBlade {
    pub(crate) fn new(draft: TerminalLaunchSettings) -> Self {
        Self { draft, error: None }
    }
}

impl GlobalBladeContent for TerminalSettingsBlade {
    #[cfg(test)]
    fn terminal_settings(&self) -> Option<&TerminalSettingsBlade> {
        Some(self)
    }
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        ui.label(
            egui::RichText::new("Application settings")
                .font(typography::page_title())
                .color(gray::_900),
        );
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
        ui.add_space(spacing::XL);
        ui.separator();
        ui.add_space(spacing::XL);
        let content_height = (ui.available_height() - FOOTER_HEIGHT).max(120.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), content_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                components::scroll::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        show_terminal_launcher(ui, &mut self.draft, &mut self.error);
                        ui.add_space(spacing::XL);
                        ui.separator();
                        ui.add_space(spacing::XL);
                        show_debug_image_presets(ui, &mut self.draft, &mut self.error);
                    });
            },
        );
        ui.separator();
        ui.add_space(spacing::SM);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            save |= TailwindButton::new("Save changes").show(ui).clicked();
            reset |= TailwindButton::secondary("Reset")
                .size(ButtonSize::Md)
                .show(ui)
                .clicked();
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
