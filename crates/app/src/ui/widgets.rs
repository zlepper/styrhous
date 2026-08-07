use components::colors::{SUCCESS, gray};
use components::{TableRowBuilder, WorkspaceEmptyState};

pub(super) fn display_resource_title(resource_name: &str) -> String {
    let mut characters = resource_name.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => "Resources".to_owned(),
    }
}

pub(super) fn resource_status(ui: &mut egui::Ui, status: &str) {
    let color = match status {
        "Running" | "Succeeded" | "Active" | "Bound" => SUCCESS,
        "Pending" | "ContainerCreating" | "Terminating" => egui::Color32::from_rgb(202, 138, 4),
        "Failed" | "Unknown" => egui::Color32::from_rgb(220, 38, 38),
        _ => gray::_400,
    };
    ui.horizontal(|ui| {
        ui.add_space(-13.0);
        let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(dot_rect.center(), 4.0, color);
        ui.add_space(5.0);
        TableRowBuilder::text(ui, status, false);
    });
}

pub(super) fn workspace_empty_state(ui: &mut egui::Ui, title: &str, message: &str) {
    WorkspaceEmptyState::new(title, message).show(ui);
}
