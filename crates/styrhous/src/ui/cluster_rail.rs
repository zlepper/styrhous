use super::state::{ClusterConnectionState, UiState};
use crate::licensing::LicenseStatus;
use crate::terminal_launcher::TerminalLaunchSettings;
use crate::updater::UpdateStatus;
use crate::worker::WorkerCommandBox;
use components::colors::{CLUSTER_RAIL_BACKGROUND, gray, indigo};
use components::design::{status, typography};
use components::{NarrowSidebar, icons};
use tracing::info;

const CLUSTER_RAIL_FOOTER_ITEMS: usize = 2;

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
    egui::Panel::left("cluster-panel")
        .exact_size(68.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(CLUSTER_RAIL_BACKGROUND))
        .show(ui, |ui| {
            NarrowSidebar::new()
                .dark_background(CLUSTER_RAIL_BACKGROUND)
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
                            let initial = cluster
                                .name
                                .chars()
                                .next()
                                .unwrap_or('?')
                                .to_uppercase()
                                .to_string();
                            let selected = ui_state.selected_cluster == Some(cluster_key);
                            let cluster_name = cluster.name.clone();
                            let (status_label, status_color) = match &cluster.connection {
                                ClusterConnectionState::Connected => ("Connected", status::SUCCESS),
                                ClusterConnectionState::Connecting => {
                                    ("Connecting", status::WARNING)
                                }
                                ClusterConnectionState::Failed(_) => {
                                    ("Connection failed", status::CRITICAL)
                                }
                                ClusterConnectionState::Disconnected => {
                                    ("Disconnected", gray::_500)
                                }
                            };
                            let tooltip = format!("{cluster_name} — {status_label}");

                            let response = sidebar.avatar_item_with_tooltip(
                                &cluster_name,
                                &initial,
                                &tooltip,
                                selected,
                            );
                            let marker_center = response.rect.center() + egui::vec2(10.0, 10.0);
                            sidebar.ui_mut().painter().circle_filled(
                                marker_center,
                                4.0,
                                status_color,
                            );

                            if response.clicked() {
                                info!("Cluster '{cluster_name}' selected");
                                if !selected {
                                    ui_state.close_all_resource_details(commands_to_send);
                                }
                                if let Some(command) = ui_state.select_cluster(cluster_key) {
                                    info!("Connecting to cluster");
                                    commands_to_send.push(command);
                                }
                            }
                        }
                    },
                    |sidebar| {
                        let notifications = sidebar.button_with_tooltip(
                            "Notifications",
                            icons::bell_icon(),
                            if unread_count == 0 {
                                "Notifications".to_owned()
                            } else {
                                format!("Notifications\n{unread_count} unread")
                            }
                            .as_str(),
                        );
                        if unread_count > 0 {
                            let center = notifications.rect.center() + egui::vec2(9.0, -9.0);
                            let sidebar_ui = sidebar.ui_mut();
                            let font = typography::semibold_or_proportional(sidebar_ui.ctx(), 10.0);
                            let painter = sidebar_ui.painter();
                            painter.circle_filled(center, 8.0, indigo::_500);
                            painter.text(
                                center,
                                egui::Align2::CENTER_CENTER,
                                unread_badge_text(unread_count),
                                font,
                                components::colors::WHITE,
                            );
                        }
                        open_notifications.set(notifications.clicked());

                        let response = sidebar.button_with_tooltip(
                            "Settings",
                            icons::settings_icon(),
                            &format!(
                                "Settings\n{}\n{}",
                                update_status.summary(),
                                license_status.summary()
                            ),
                        );
                        if update_status.shows_badge() || license_status.shows_warning() {
                            let marker_center = response.rect.center() + egui::vec2(8.0, -8.0);
                            sidebar.ui_mut().painter().circle_filled(
                                marker_center,
                                4.0,
                                status::WARNING,
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

fn unread_badge_text(unread_count: usize) -> String {
    if unread_count > 9 {
        "9+".to_owned()
    } else {
        unread_count.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::unread_badge_text;

    #[test]
    fn unread_badge_uses_a_plus_suffix_for_counts_above_nine() {
        assert_eq!(unread_badge_text(0), "0");
        assert_eq!(unread_badge_text(9), "9");
        assert_eq!(unread_badge_text(10), "9+");
    }
}
