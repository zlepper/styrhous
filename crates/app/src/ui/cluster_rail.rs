use super::state::UiState;
use crate::worker::WorkerCommand;
use components::NarrowSidebar;
use components::colors::CLUSTER_RAIL_BACKGROUND;
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

                        if sidebar
                            .avatar_item(&cluster_name, &initial, selected)
                            .clicked()
                        {
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
