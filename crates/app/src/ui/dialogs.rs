use super::state::UiState;
use crate::worker::WorkerCommand;
use components::colors::gray;
use components::design::{status, typography};

pub(super) fn show_delete_confirmation(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(cluster) = ui_state.clusters.get(&cluster_id) else {
        return;
    };
    let Some(pending) = cluster.pending_delete.clone() else {
        return;
    };
    let Some(api_resource) = cluster.selected_api_resource.clone() else {
        return;
    };
    let cluster_key = cluster.cluster_key;

    let mut cancel = false;
    let mut confirm = false;
    egui::Window::new("Delete resource?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.label(
                egui::RichText::new(format!("Delete {}?", pending.resource_name))
                    .strong()
                    .color(gray::_900),
            );
            ui.add_space(8.0);
            let scope_text = pending.namespace.as_deref().map_or_else(
                || "This will delete the cluster-wide resource.".to_owned(),
                |namespace| format!("This will delete the resource from namespace {namespace}."),
            );
            ui.label(
                egui::RichText::new(scope_text)
                    .font(typography::body())
                    .color(gray::_600),
            );
            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(format!("Delete {}", pending.resource_name))
                                .color(egui::Color32::WHITE),
                        )
                        .fill(status::DANGER),
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });

    if cancel {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_delete = None;
        }
    } else if confirm {
        commands_to_send.push(WorkerCommand::DeleteResource {
            cluster_key,
            api_resource,
            namespace: pending.namespace,
            resource_name: pending.resource_name,
        });
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_delete = None;
        }
    }
}
