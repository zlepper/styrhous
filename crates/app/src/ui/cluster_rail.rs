use super::state::{ClusterConnectionState, UiState};
use crate::terminal_launcher::TerminalLaunchSettings;
use crate::worker::WorkerCommand;
use components::colors::{CLUSTER_RAIL_BACKGROUND, gray};
use components::design::{spacing, status};
use components::{NarrowSidebar, icons};
use tracing::info;

pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
    terminal_settings: &TerminalLaunchSettings,
) {
    egui::SidePanel::left("cluster-panel")
        .exact_width(68.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(CLUSTER_RAIL_BACKGROUND))
        .show(ctx, |ui| {
            NarrowSidebar::new()
                .dark_background(CLUSTER_RAIL_BACKGROUND)
                .show(ui, |sidebar| {
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
                            ClusterConnectionState::Connected(_) => ("Connected", status::SUCCESS),
                            ClusterConnectionState::Connecting => ("Connecting", status::WARNING),
                            ClusterConnectionState::Failed(_) => {
                                ("Connection failed", status::CRITICAL)
                            }
                            ClusterConnectionState::Disconnected => ("Disconnected", gray::_500),
                        };
                        let tooltip = format!("{cluster_name} — {status_label}");

                        let response = sidebar.avatar_item_with_tooltip(
                            &cluster_name,
                            &initial,
                            &tooltip,
                            selected,
                        );
                        let marker_center = response.rect.center() + egui::vec2(10.0, 10.0);
                        sidebar
                            .ui_mut()
                            .painter()
                            .circle_filled(marker_center, 4.0, status_color);

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
                    sidebar.ui_mut().with_layout(
                        egui::Layout::bottom_up(egui::Align::Center),
                        |ui| {
                            ui.add_space(spacing::SM);
                            let response = ui.add(
                                egui::Button::image(
                                    icons::settings_icon()
                                        .fit_to_exact_size(egui::Vec2::splat(21.0)),
                                )
                                .frame(false),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    "Settings",
                                )
                            });
                            if response.on_hover_text("Settings").clicked() {
                                ui_state.open_terminal_settings(terminal_settings);
                            }
                        },
                    );
                });
        });
}
