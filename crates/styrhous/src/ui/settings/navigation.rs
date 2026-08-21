use super::destinations::{SettingsDestination, settings_destination_card};
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
            egui::RichText::new("Manage application preferences and add Kubernetes contexts.")
                .font(typography::body())
                .color(gray::_600),
        );
        ui.add_space(spacing::XL);
        ui.separator();
        ui.add_space(spacing::XL + spacing::XS);
        ui.label(
            egui::RichText::new("CONFIGURATION")
                .font(typography::semibold(typography::BODY_SIZE))
                .color(gray::_500),
        );
        ui.add_space(spacing::MD);
        let application_settings = settings_destination_card(
            ui,
            SettingsDestination {
                label: "Open application settings",
                description: "Configure terminal launching, debug images, and application updates.",
                icon: icons::settings_destination_application_icon(),
            },
        );
        ui.add_space(spacing::XL - spacing::XS);
        let cluster_discovery = settings_destination_card(
            ui,
            SettingsDestination {
                label: "Open cluster discovery",
                description: "Find and add clusters available through Azure CLI or Tailscale.",
                icon: icons::settings_destination_discovery_icon(),
            },
        );
        GlobalBladeRenderResult {
            next_content: application_settings
                .then(|| {
                    Box::new(TerminalSettingsBlade::new(
                        context.terminal_launch_settings().clone(),
                    )) as Box<dyn GlobalBladeContent>
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
