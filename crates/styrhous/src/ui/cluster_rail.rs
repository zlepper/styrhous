use super::state::{ClusterConnectionState, UiState};
use crate::licensing::LicenseStatus;
use crate::terminal_launcher::TerminalLaunchSettings;
use crate::updater::UpdateStatus;
use crate::worker::WorkerCommandBox;
use components::design::size;
use components::design::theme::{ThemeExt, Tone};
use components::{CountBadge, NarrowSidebar, corner_rect, icons, paint_status_dot};
use tracing::info;

const CLUSTER_RAIL_FOOTER_ITEMS: usize = 2;
/// Where a count badge sits on a rail button: points from its right and top.
const RAIL_BADGE_INSET: egui::Vec2 = egui::vec2(3.0, 4.0);
/// Where the settings warning lamp sits on its rail button: points from its
/// right and top.
const RAIL_LAMP_INSET: egui::Vec2 = egui::vec2(8.0, 8.0);

/// The 40 point rail button centred in a rail item's `rect`.
fn rail_button(rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(size::RAIL_BUTTON))
}

pub(super) fn show(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
    terminal_settings: &TerminalLaunchSettings,
    update_status: &UpdateStatus,
    license_status: &LicenseStatus,
) {
    let open_settings = std::cell::Cell::new(false);
    let open_notifications = std::cell::Cell::new(false);
    let unread_count = ui_state.unread_operation_count();
    let theme = ui.tokens();
    egui::Panel::left("cluster-panel")
        .exact_size(68.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(theme.nav.rail))
        .show(ui, |ui| {
            NarrowSidebar::new()
                .footer_items(CLUSTER_RAIL_FOOTER_ITEMS)
                .show_with_footer(
                    ui,
                    |sidebar| {
                        let mut cluster_keys: Vec<_> = ui_state.clusters.keys().copied().collect();
                        cluster_keys.sort_unstable();
                        for cluster_key in cluster_keys {
                            let Some(cluster) = ui_state.clusters.get(&cluster_key) else {
                                continue;
                            };
                            let initial = cluster.presentation.badge(&cluster.name);
                            let selected = ui_state.selected_cluster == Some(cluster_key);
                            let cluster_name =
                                cluster.presentation.display_name(&cluster.name).to_owned();
                            let (status_label, status_tone) = match &cluster.connection {
                                ClusterConnectionState::Connected => ("Connected", Tone::Success),
                                ClusterConnectionState::Connecting => ("Connecting", Tone::Warning),
                                ClusterConnectionState::Failed(_) => {
                                    ("Connection failed", Tone::Danger)
                                }
                                ClusterConnectionState::Disconnected => {
                                    ("Disconnected", Tone::Neutral)
                                }
                            };
                            let tooltip = if cluster_name == cluster.name {
                                format!("{cluster_name} — {status_label}")
                            } else {
                                format!("{cluster_name}\n{} — {status_label}", cluster.name)
                            };

                            let accessible_name = if cluster_name == cluster.name {
                                cluster_name.clone()
                            } else {
                                format!("{cluster_name} ({})", cluster.name)
                            };
                            let response = sidebar.avatar_item_with_tooltip(
                                &accessible_name,
                                &initial,
                                &tooltip,
                                selected,
                                status_tone,
                            );

                            if response.clicked() {
                                info!("Cluster '{cluster_name}' selected");
                                if !selected {
                                    ui_state.close_all_resource_details(commands_to_send);
                                }
                                ui_state.select_cluster(cluster_key, commands_to_send);
                            }
                        }
                    },
                    |sidebar| {
                        let notifications = sidebar.button_with_tooltip(
                            notifications_label(unread_count),
                            icons::Icon::Bell.image(),
                            &notifications_label(unread_count).replace(", ", "\n"),
                        );
                        if unread_count > 0 {
                            let sidebar_ui = sidebar.ui_mut();
                            let badge = CountBadge::new(unread_count).tone(Tone::Accent);
                            let badge_size = badge.size(sidebar_ui);
                            // `.rbtn .count`: 4 points from the top and 3 from the
                            // right of the 40 point rail button.
                            let badge_rect = corner_rect(
                                rail_button(notifications.rect),
                                egui::Align2::RIGHT_TOP,
                                badge_size,
                                RAIL_BADGE_INSET,
                            );
                            badge.paint(sidebar_ui, badge_rect, theme.nav.rail);
                        }
                        open_notifications.set(notifications.clicked());

                        let response = sidebar.button_with_tooltip(
                            "Settings",
                            icons::Icon::Cog6Tooth.image(),
                            &format!(
                                "Settings\n{}\n{}",
                                update_status.summary(),
                                license_status.summary()
                            ),
                        );
                        if update_status.shows_badge() || license_status.shows_warning() {
                            let marker = corner_rect(
                                rail_button(response.rect),
                                egui::Align2::RIGHT_TOP,
                                egui::Vec2::splat(size::STATUS_DOT),
                                RAIL_LAMP_INSET,
                            );
                            paint_status_dot(
                                sidebar.ui_mut().painter(),
                                &theme,
                                marker,
                                Tone::Warning,
                            );
                        }
                        open_settings.set(response.clicked());
                    },
                );
        });
    if open_notifications.get() {
        ui_state.open_notification_history(commands_to_send);
    }
    if open_settings.get() {
        let _ = terminal_settings;
        ui_state.open_settings_home(commands_to_send);
    }
}

/// The accessible name of the notifications rail button, which carries the
/// unread count its badge shows.
fn notifications_label(unread_count: usize) -> String {
    if unread_count == 0 {
        "Notifications".to_owned()
    } else {
        format!("Notifications, {unread_count} unread")
    }
}
