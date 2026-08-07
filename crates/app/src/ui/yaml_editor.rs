use super::state::UiState;
use crate::worker::WorkerCommand;
use components::WorkspaceDrawer;
use components::colors::{WHITE, gray, indigo};
use tracing::info;

pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    let Some(selected_cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(cluster) = ui_state.clusters.get_mut(&selected_cluster_id) else {
        return;
    };
    let Some(yaml_panel) = &mut cluster.yaml_panel else {
        return;
    };

    let mut close = false;
    egui::TopBottomPanel::bottom("yaml-panel")
        .resizable(true)
        .min_height(100.0)
        .default_height(yaml_panel.panel_height)
        .frame(WorkspaceDrawer::frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Edit YAML · {}", yaml_panel.resource_name))
                        .strong()
                        .size(14.0)
                        .color(WHITE),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} / {}",
                        yaml_panel.api_resource.kind, yaml_panel.namespace
                    ))
                    .size(12.0)
                    .color(gray::_400),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Close YAML Editor").color(gray::_100),
                            )
                            .fill(gray::_700),
                        )
                        .clicked()
                    {
                        if yaml_panel.is_modified() {
                            info!("Discarding unsaved YAML changes");
                        }
                        close = true;
                    }
                    if ui
                        .add_enabled(
                            yaml_panel.is_modified(),
                            egui::Button::new(egui::RichText::new("Save YAML").color(WHITE))
                                .fill(indigo::_600),
                        )
                        .clicked()
                    {
                        commands_to_send.push(WorkerCommand::ApplyResourceYaml {
                            cluster_key: cluster.cluster_key,
                            api_resource: yaml_panel.api_resource.clone(),
                            namespace: yaml_panel.namespace.clone(),
                            resource_name: yaml_panel.resource_name.clone(),
                            yaml: yaml_panel.edited_yaml.clone(),
                        });
                    }
                    if yaml_panel.is_modified() {
                        ui.label(
                            egui::RichText::new("Modified")
                                .color(egui::Color32::from_rgb(234, 179, 8))
                                .size(12.0),
                        );
                    }
                });
            });
            ui.painter().line_segment(
                [ui.min_rect().left_bottom(), ui.min_rect().right_bottom()],
                egui::Stroke::new(1.0, gray::_700),
            );
            ui.add_space(8.0);
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut yaml_panel.edited_yaml)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .text_color(WorkspaceDrawer::text_color())
                            .background_color(WorkspaceDrawer::editor_background())
                            .desired_width(f32::INFINITY)
                            .desired_rows(20)
                            .hint_text("YAML Editor"),
                    );
                });
        });

    if close {
        cluster.yaml_panel = None;
    }
}
