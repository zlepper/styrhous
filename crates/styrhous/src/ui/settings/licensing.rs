use super::*;
use crate::licensing::{LicenseServer, LicenseStatus};

#[derive(Debug, Default)]
pub(crate) struct LicenseSettingsBlade {
    initialized: bool,
    custom_server: bool,
    custom_origin: String,
    display_name: String,
    pending_server_switch: Option<LicenseServer>,
    input_error: Option<String>,
    name_error: Option<String>,
}

impl GlobalBladeContent for LicenseSettingsBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        ui.label(
            egui::RichText::new("License & account")
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
        if !self.initialized {
            let settings = context.licensing().settings().clone();
            self.custom_server = matches!(settings.server, LicenseServer::Custom(_));
            self.custom_origin = match settings.server {
                LicenseServer::Hosted => String::new(),
                LicenseServer::Custom(origin) => origin,
            };
            self.display_name = settings.display_name;
            self.initialized = true;
        }

        let (status, keychain_warning, account_url, current_server) = {
            let licensing = context.licensing();
            (
                licensing.status().clone(),
                licensing.keychain_warning(),
                licensing.account_url(),
                licensing.settings().server.clone(),
            )
        };

        ui.label(
            egui::RichText::new(status.summary())
                .font(typography::body())
                .color(if status.shows_warning() {
                    status::WARNING_TEXT
                } else {
                    status::SUCCESS
                }),
        );
        if let Some(lease) = status.lease() {
            ui.add_space(spacing::SM);
            ui.label(format!("Available offline until: {}", lease.expiry_label()));
        }
        if keychain_warning {
            ui.add_space(spacing::SM);
            ui.label(
                egui::RichText::new(
                    "Secure credential storage is unavailable. This sign-in lasts only for this session.",
                )
                .color(status::WARNING_TEXT),
            );
        }

        ui.add_space(spacing::LG);
        show_account_actions(ui, context, &status, account_url);

        ui.add_space(spacing::XL);
        ui.separator();
        ui.add_space(spacing::LG);
        ui.label(
            egui::RichText::new("INSTALLATION")
                .font(typography::semibold(typography::BODY_SIZE))
                .color(gray::_500),
        );
        ui.add_space(spacing::SM);
        ui.label("Device name");
        TailwindTextInput::new(&mut self.display_name)
            .id_salt("license-display-name")
            .accessibility_label("License device display name")
            .show(ui);
        if TailwindButton::secondary("Save name").show(ui).clicked() {
            let trimmed = self.display_name.trim();
            if trimmed.is_empty() || trimmed.encode_utf16().count() > 120 {
                self.name_error = Some("Device name must contain 1 to 120 characters.".into());
            } else {
                context.licensing().set_display_name(trimmed.to_owned());
                self.name_error = None;
            }
        }

        ui.add_space(spacing::LG);
        if let Some(error) = &self.name_error {
            ui.label(egui::RichText::new(error).color(status::DANGER));
        }
        egui::CollapsingHeader::new("Advanced")
            .default_open(matches!(status, LicenseStatus::Unconfigured))
            .show(ui, |ui| {
                ui.add_space(spacing::LG);
                ui.label("License server");
                ui.radio_value(&mut self.custom_server, false, "Styrhous cloud");
                ui.radio_value(&mut self.custom_server, true, "Self-hosted server");
                if self.custom_server {
                    TailwindTextInput::new(&mut self.custom_origin)
                        .id_salt("custom-license-origin")
                        .hint_text("https://licenses.example.com")
                        .accessibility_label("Self-hosted license server URL")
                        .show(ui);
                }
                let draft_server = if self.custom_server {
                    LicenseServer::Custom(self.custom_origin.trim().to_owned())
                } else {
                    LicenseServer::Hosted
                };
                if self
                    .pending_server_switch
                    .as_ref()
                    .is_some_and(|pending| pending != &draft_server)
                {
                    self.pending_server_switch = None;
                }
                if draft_server != current_server
                    && TailwindButton::secondary("Apply server change")
                        .show(ui)
                        .clicked()
                {
                    match draft_server.origin() {
                        Ok(Some(_)) => {
                            self.pending_server_switch = Some(draft_server.clone());
                            self.input_error = None;
                        }
                        Ok(None) => {
                            self.input_error = Some(
                                "The hosted licensing service is not configured in this build."
                                    .into(),
                            );
                        }
                        Err(error) => self.input_error = Some(error.to_string()),
                    }
                }
                if let Some(pending_server) = self.pending_server_switch.clone() {
                    ui.add_space(spacing::SM);
                    ui.label(
                egui::RichText::new(
                    "Changing server signs out this installation and clears its cached lease.",
                )
                .color(status::WARNING_TEXT),
            );
                    ui.horizontal(|ui| {
                        if TailwindButton::danger("Confirm server change")
                            .show(ui)
                            .clicked()
                        {
                            context.licensing().switch_server(pending_server);
                            self.pending_server_switch = None;
                        }
                        if TailwindButton::secondary("Cancel").show(ui).clicked() {
                            self.pending_server_switch = None;
                        }
                    });
                }
                if let Some(error) = &self.input_error {
                    ui.label(egui::RichText::new(error).color(status::DANGER));
                }

                if let Some(lease) = status.lease() {
                    ui.add_space(spacing::LG);
                    ui.label(format!(
                        "License details: {} ({})",
                        lease.state, lease.reason_code
                    ));
                }
            });
        GlobalBladeRenderResult::default()
    }
}

fn show_account_actions(
    ui: &mut egui::Ui,
    context: &mut GlobalBladeRenderContext<'_>,
    status: &LicenseStatus,
    account_url: Option<String>,
) {
    if let LicenseStatus::AwaitingApproval(authorization) = status {
        ui.label("Enter this code in your browser:");
        ui.label(
            egui::RichText::new(&authorization.user_code)
                .font(typography::monospace())
                .strong(),
        );
        ui.label(format!(
            "This request expires in {} minutes.",
            authorization.expires_in / 60
        ));
        ui.horizontal(|ui| {
            if TailwindButton::new("Open approval page").show(ui).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(
                    authorization.verification_uri_complete.clone(),
                ));
            }
            if TailwindButton::secondary("Copy code").show(ui).clicked() {
                ui.ctx().copy_text(authorization.user_code.clone());
            }
            if TailwindButton::danger("Cancel sign-in").show(ui).clicked() {
                context.licensing().sign_out();
            }
        });
        ui.label(format!(
            "Approval server: {}",
            authorization.verification_uri
        ));
        return;
    }

    ui.horizontal_wrapped(|ui| {
        if matches!(
            status,
            LicenseStatus::SignedOut
                | LicenseStatus::Evaluation { .. }
                | LicenseStatus::Failed { .. }
        ) && !status.is_waiting_for_refresh()
            && TailwindButton::new("Sign in").show(ui).clicked()
        {
            context.licensing().sign_in();
        }
        if (matches!(
            status,
            LicenseStatus::Licensed(_) | LicenseStatus::Offline(_) | LicenseStatus::Failed { .. }
        ) || status.is_waiting_for_refresh())
            && TailwindButton::secondary("Refresh license")
                .show(ui)
                .clicked()
        {
            context.licensing().refresh();
        }
        if let Some(url) = account_url
            && TailwindButton::secondary("Manage account")
                .show(ui)
                .clicked()
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
        if (matches!(
            status,
            LicenseStatus::Checking
                | LicenseStatus::Licensed(_)
                | LicenseStatus::Offline(_)
                | LicenseStatus::Failed { .. }
        ) || status.is_waiting_for_refresh())
            && TailwindButton::danger("Sign out").show(ui).clicked()
        {
            context.licensing().sign_out();
        }
    });
}
