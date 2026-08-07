use super::state::{ClusterConnectionState, UiState};
use crate::worker::WorkerCommand;
use components::NarrowSidebar;
use components::colors::{CLUSTER_RAIL_BACKGROUND, SUCCESS, gray};
use tracing::info;

pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
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
                            ClusterConnectionState::Connected(_) => ("Connected", SUCCESS),
                            ClusterConnectionState::Connecting => {
                                ("Connecting", egui::Color32::from_rgb(202, 138, 4))
                            }
                            ClusterConnectionState::Failed(_) => {
                                ("Connection failed", egui::Color32::from_rgb(220, 38, 38))
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
                            if let Some(command) = ui_state.select_cluster(cluster_key) {
                                info!("Connecting to cluster");
                                commands_to_send.push(command);
                            }
                        }
                    }
                });
        });
}
