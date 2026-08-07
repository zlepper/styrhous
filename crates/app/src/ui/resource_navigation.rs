use super::state::UiState;
use crate::api_resource::ApiResource;
use components::colors::{NAVIGATION_BACKGROUND, WHITE};
use components::{WideSidebar, semibold_font};

pub(super) fn show(ctx: &egui::Context, ui_state: &UiState) -> Option<ApiResource> {
    let selected_cluster_id = ui_state.selected_cluster?;
    let cluster = ui_state.clusters.get(&selected_cluster_id)?;
    let mut clicked_api_resource = None;

    egui::SidePanel::left("api-selector")
        .exact_width(292.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(NAVIGATION_BACKGROUND))
        .show(ctx, |ui| {
            WideSidebar::new().dark().show(ui, |sidebar| {
                sidebar.ui_mut().add_space(23.0);
                sidebar.ui_mut().horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.label(
                        egui::RichText::new(&cluster.name)
                            .font(semibold_font(20.0))
                            .color(WHITE),
                    );
                });
                sidebar.ui_mut().add_space(17.0);
                for section in &cluster.resource_navigation.curated_sections {
                    sidebar.expandable_text(section.name, false, |sidebar| {
                        for api_resource in &section.api_resources {
                            let selected =
                                cluster.selected_api_resource.as_ref() == Some(api_resource);
                            if sidebar.child_item(&api_resource.name, selected).clicked() {
                                clicked_api_resource = Some(api_resource.clone());
                            }
                        }
                    });
                }
                for (api_group_name, api_resources) in &cluster.resource_navigation.other_api_groups
                {
                    sidebar.expandable_text(api_group_name, false, |sidebar| {
                        sidebar.nested_expandable_text(
                            format!("other-{api_group_name}"),
                            "Other",
                            false,
                            |sidebar| {
                                for api_resource in api_resources {
                                    let selected = cluster.selected_api_resource.as_ref()
                                        == Some(api_resource);
                                    if sidebar
                                        .nested_child_item(&api_resource.name, selected)
                                        .clicked()
                                    {
                                        clicked_api_resource = Some(api_resource.clone());
                                    }
                                }
                            },
                        );
                    });
                }
            });
        });

    clicked_api_resource
}
