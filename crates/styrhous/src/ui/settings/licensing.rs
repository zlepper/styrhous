use super::*;
use crate::licensing::{LicenseServer, LicenseStatus, VerifiedLease};
use crate::ui::blade_text::{blade_section, gap};
use components::design::theme::Tone;
use components::property_grid::Property;

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
    fn sidebar_destination(&self) -> Option<crate::ui::global_blade::SidebarDestination> {
        Some(crate::ui::global_blade::SidebarDestination::Settings)
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        components::blade_title_with_actions(ui, None, "License & account", |ui| {
            crate::ui::blade_text::blade_more_actions(ui);
        });
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let theme = ui.tokens();
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

        let (status, keychain_warning, account_url, current_server, last_check) = {
            let licensing = context.licensing();
            (
                licensing.status().clone(),
                licensing.keychain_warning(),
                licensing.account_url(),
                licensing.settings().server.clone(),
                licensing.settings().last_check_unix,
            )
        };

        ui.spacing_mut().item_spacing.y = 0.0;
        show_status_callout(ui, &status);
        if keychain_warning {
            gap(ui, space::S8);
            components::Callout::new(Tone::Warning, "Secure credential storage is unavailable")
                .body("This sign-in lasts only for this session.")
                .show(ui);
        }
        gap(ui, space::S16);
        let now = crate::ui::clock::now(ui.ctx());
        Card::new().padding(0).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(lease) = status.lease() {
                egui::Frame::new().inner_margin(space::S16).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    show_lease_properties(ui, lease, &current_server, last_check, now);
                });
                let theme = ui.tokens();
                let y = ui.cursor().top();
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    y + 0.5,
                    stroke::subtle_divider(&theme),
                );
                ui.add_space(components::design::size::HAIRLINE);
            }
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(space::S16 as i8, space::S12 as i8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    show_account_actions(ui, context, &status, account_url);
                });
        });

        gap(ui, space::S24);
        blade_section(ui, "Installation", None);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 0.0),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.x = space::S8;
                ui.allocate_ui_with_layout(
                    egui::vec2(LICENSE_DEVICE_NAME_WIDTH, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(LICENSE_DEVICE_NAME_WIDTH);
                        FormField::new("Device name")
                            .help("Shown in your account’s device list.")
                            .error(self.name_error.as_deref())
                            .show(ui, |ui| {
                                TextInput::new(&mut self.display_name)
                                    .id_salt("license-display-name")
                                    .accessibility_label("License device display name")
                                    .show(ui);
                            });
                    },
                );
                ui.vertical(|ui| {
                    // Level with the input, under the label.
                    ui.add_space(type_scale::FIELD_LABEL.line_height() + space::LABEL_TO_CONTROL);
                    if Button::secondary("Save name").show(ui).clicked() {
                        let trimmed = self.display_name.trim();
                        if trimmed.is_empty() || trimmed.encode_utf16().count() > 120 {
                            self.name_error =
                                Some("Device name must contain 1 to 120 characters.".into());
                        } else {
                            context.licensing().set_display_name(trimmed.to_owned());
                            self.name_error = None;
                        }
                    }
                });
            },
        );

        gap(ui, space::S20);
        components::card::Disclosure::new("license-advanced", "Advanced")
            .meta("License server")
            .default_open(matches!(status, LicenseStatus::Unconfigured))
            .session_only()
            .show(ui, |ui| {
                self.show_server_settings(ui, context, &status, &current_server);
            });
        let _ = theme;
        GlobalBladeRenderResult::default()
    }
}

impl LicenseSettingsBlade {
    /// The license server choice, its confirmation and the lease details.
    fn show_server_settings(
        &mut self,
        ui: &mut egui::Ui,
        context: &mut GlobalBladeRenderContext<'_>,
        status: &LicenseStatus,
        current_server: &LicenseServer,
    ) {
        let theme = ui.tokens();
        FormField::new("License server").show(ui, |ui| {
            Radio::new(&mut self.custom_server, false, "Styrhous cloud").show(ui);
            Radio::new(&mut self.custom_server, true, "Self-hosted server").show(ui);
            if self.custom_server {
                gap(ui, space::S8);
                TextInput::new(&mut self.custom_origin)
                    .id_salt("custom-license-origin")
                    .placeholder("https://licenses.example.com")
                    .accessibility_label("Self-hosted license server URL")
                    .show(ui);
            }
        });
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
        if &draft_server != current_server {
            gap(ui, space::S12);
            if Button::secondary("Apply server change").show(ui).clicked() {
                match draft_server.origin() {
                    Ok(Some(_)) => {
                        self.pending_server_switch = Some(draft_server.clone());
                        self.input_error = None;
                    }
                    Ok(None) => {
                        self.input_error = Some(
                            "The hosted licensing service is not configured in this build.".into(),
                        );
                    }
                    Err(error) => self.input_error = Some(error.to_string()),
                }
            }
        }
        if let Some(pending_server) = self.pending_server_switch.clone() {
            gap(ui, space::S12);
            components::Callout::new(Tone::Warning, "Changing server signs out this installation")
                .body("It also clears the cached lease.")
                .show(ui);
            gap(ui, space::S8);
            ui.horizontal(|ui| {
                if Button::danger_quiet("Confirm server change")
                    .show(ui)
                    .clicked()
                {
                    context.licensing().switch_server(pending_server);
                    self.pending_server_switch = None;
                }
                if Button::secondary("Cancel").show(ui).clicked() {
                    self.pending_server_switch = None;
                }
            });
        }
        if let Some(error) = &self.input_error {
            gap(ui, space::S8);
            type_scale::UI.label(ui, error, theme.danger.text);
        }
        if let Some(lease) = status.lease() {
            gap(ui, space::S16);
            type_scale::META.label(
                ui,
                &format!("License details: {} ({})", lease.state, lease.reason_code),
                theme.text.muted,
            );
        }
    }
}

/// The callout at the top of the blade: what the license is doing, if that
/// is worth saying (a healthy license needs no callout).
fn show_status_callout(ui: &mut egui::Ui, status: &LicenseStatus) {
    let (tone, title, body) = match status {
        LicenseStatus::Licensed(lease) | LicenseStatus::Offline(lease)
            if lease.state == "grace" =>
        {
            (
                Tone::Warning,
                format!("Grace period until {}", long_date(crate::ui::clock::zone(ui.ctx()), lease.expires_at)),
                "The licensing service couldn’t be reached. Styrhous keeps working offline until the grace period ends."
                    .to_owned(),
            )
        }
        LicenseStatus::Licensed(_) => return,
        LicenseStatus::AwaitingApproval(_) => return,
        status => {
            let tone = if status.shows_warning() {
                Tone::Warning
            } else {
                Tone::Info
            };
            (
                tone,
                status.summary(),
                status.free_use_note().unwrap_or_default().to_owned(),
            )
        }
    };
    let callout = components::Callout::new(tone, &title);
    if body.is_empty() {
        callout.show(ui);
    } else {
        callout.body(&body).show(ui);
    }
}

/// The lease as a grid of properties: what the app knows of it, with an
/// em dash and its reason for what the licensing service does not report.
fn show_lease_properties(
    ui: &mut egui::Ui,
    lease: &VerifiedLease,
    server: &LicenseServer,
    last_check: Option<u64>,
    now: time::OffsetDateTime,
) {
    let plan = plan_label(&lease.state);
    let offline_until = lease.expiry_label();
    let last_check = last_check.map(|unix| time_ago(now, unix));
    let server = match server {
        LicenseServer::Hosted => "Styrhous cloud",
        LicenseServer::Custom(_) => "Self-hosted server",
    };
    let properties = [
        Property::text("Plan", plan),
        // The lease carries no seat counts or account email.
        Property::unavailable("Seats"),
        Property::unavailable("Account"),
        match &last_check {
            Some(last_check) => Property::text("Last check", last_check.as_str()),
            None => Property::unavailable("Last check"),
        },
        Property::text("Offline until", offline_until),
        Property::text("Server", server),
    ];
    components::property_grid::PropertyGrid::new(&properties)
        .max_columns(3)
        .show(ui);
}

/// "commercial" → "Commercial". A grace period only follows a commercial
/// subscription, so it reads as that plan.
fn plan_label(state: &str) -> String {
    let state = if state == "grace" {
        "commercial"
    } else {
        state
    };
    let mut characters = state.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default()
}

/// `unix` as the date in `zone`, "1 January 2100".
fn long_date(zone: crate::ui::clock::Zone, unix: u64) -> String {
    zone.local_unix(unix as i64).map_or_else(
        || "an unknown time".into(),
        |value| format!("{} {} {}", value.day(), value.month(), value.year()),
    )
}

/// "just now", "5 minutes ago", "2 days ago".
fn time_ago(now: time::OffsetDateTime, unix: u64) -> String {
    let elapsed = now.unix_timestamp() - unix as i64;
    let (count, unit) = match elapsed {
        ..60 => return "just now".to_owned(),
        60..3600 => (elapsed / 60, "minute"),
        3600..86_400 => (elapsed / 3600, "hour"),
        _ => (elapsed / 86_400, "day"),
    };
    format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" })
}

/// The account buttons for `status`: refresh, manage and sign in at the left,
/// sign out at the right. A pending browser approval shows its code instead.
fn show_account_actions(
    ui: &mut egui::Ui,
    context: &mut GlobalBladeRenderContext<'_>,
    status: &LicenseStatus,
    account_url: Option<String>,
) {
    let theme = ui.tokens();
    if let LicenseStatus::AwaitingApproval(authorization) = status {
        type_scale::UI.label(ui, "Enter this code in your browser:", theme.text.secondary);
        gap(ui, space::S4);
        ui.label(
            egui::RichText::new(&authorization.user_code)
                .font(egui::FontId::monospace(type_scale::HEADING.size()))
                .color(theme.text.primary),
        );
        gap(ui, space::S4);
        type_scale::UI.label(
            ui,
            &format!(
                "This request expires in {} minutes.",
                authorization.expires_in / 60
            ),
            theme.text.secondary,
        );
        gap(ui, space::S16);
        ui.horizontal(|ui| {
            if Button::primary("Open approval page")
                .trailing_icon(components::icons::Icon::ArrowTopRightOnSquare)
                .show(ui)
                .clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(
                    authorization.verification_uri_complete.clone(),
                ));
            }
            if Button::secondary("Copy code").show(ui).clicked() {
                ui.ctx().copy_text(authorization.user_code.clone());
            }
            if Button::danger_quiet("Cancel sign-in").show(ui).clicked() {
                context.licensing().sign_out();
            }
        });
        gap(ui, space::S8);
        type_scale::META.label(
            ui,
            &format!("Approval server: {}", authorization.verification_uri),
            theme.text.muted,
        );
        return;
    }

    let can_sign_out = matches!(
        status,
        LicenseStatus::Checking
            | LicenseStatus::Licensed(_)
            | LicenseStatus::Offline(_)
            | LicenseStatus::Evaluation { .. }
            | LicenseStatus::Revoked
            | LicenseStatus::Failed { .. }
    );
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), components::design::size::CONTROL_MD),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = space::S8;
            // For a revoked installation, signing out dismisses the warning and returns to free use.
            if can_sign_out && Button::danger_quiet("Sign out").show(ui).clicked() {
                context.licensing().sign_out();
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = space::S8;
                // Evaluation means signed in without entitlement, so it offers a refresh, not sign-in.
                if matches!(
                    status,
                    LicenseStatus::SignedOut
                        | LicenseStatus::Revoked
                        | LicenseStatus::Failed { .. }
                ) && Button::primary("Sign in").show(ui).clicked()
                {
                    context.licensing().sign_in();
                }
                if matches!(
                    status,
                    LicenseStatus::Licensed(_)
                        | LicenseStatus::Offline(_)
                        | LicenseStatus::Evaluation { .. }
                        | LicenseStatus::Failed { .. }
                ) && Button::secondary("Refresh license")
                    .leading_icon(components::icons::Icon::ArrowPath)
                    .show(ui)
                    .clicked()
                {
                    context.licensing().refresh();
                }
                if let Some(url) = account_url
                    && Button::secondary("Manage account")
                        .trailing_icon(components::icons::Icon::ArrowTopRightOnSquare)
                        .show(ui)
                        .clicked()
                {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            });
        },
    );
}
