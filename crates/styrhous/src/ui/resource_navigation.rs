use super::state::UiState;
use crate::api_resource::ApiResource;
use crate::resource_catalog::CuratedNavigationEntry;
use components::WideSidebar;
use components::colors::{NAVIGATION_BACKGROUND, WHITE};
use components::design::typography;

pub(super) fn show(ui: &mut egui::Ui, ui_state: &mut UiState) -> Option<ApiResource> {
    let selected_cluster_id = ui_state.selected_cluster?;
    let cluster = ui_state.clusters.get(&selected_cluster_id)?;
    let cluster_name = cluster.name.clone();
    let resource_navigation = cluster.resource_navigation.clone();
    let selected_api_resource = cluster.selected_api_resource.clone();
    let mut clicked_api_resource = None;

    egui::Panel::left("api-selector")
        .exact_size(292.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(NAVIGATION_BACKGROUND))
        .show(ui, |ui| {
            WideSidebar::new().dark().show(ui, |sidebar| {
                sidebar.ui_mut().add_space(23.0);
                sidebar.ui_mut().horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.label(
                        egui::RichText::new(&cluster_name)
                            .font(typography::page_title())
                            .color(WHITE),
                    );
                });
                sidebar.ui_mut().add_space(17.0);
                for entry in &resource_navigation.curated_entries {
                    match entry {
                        CuratedNavigationEntry::Resource(api_resource) => {
                            let selected = selected_api_resource.as_ref() == Some(api_resource);
                            if sidebar
                                .primary_text_item(api_resource.display_name(), selected)
                                .clicked()
                            {
                                clicked_api_resource = Some(api_resource.clone());
                            }
                        }
                        CuratedNavigationEntry::Section(section) => {
                            let node_id = format!("section:{}", section.name);
                            let response = sidebar.expandable_text(
                                section.name,
                                ui_state.resource_navigation_node_is_expanded(&node_id),
                                |sidebar| {
                                    for api_resource in &section.api_resources {
                                        let selected =
                                            selected_api_resource.as_ref() == Some(api_resource);
                                        if sidebar
                                            .child_item(api_resource.display_name(), selected)
                                            .clicked()
                                        {
                                            clicked_api_resource = Some(api_resource.clone());
                                        }
                                    }
                                },
                            );
                            ui_state
                                .set_resource_navigation_node_expanded(node_id, response.is_open);
                        }
                    }
                }
                if !resource_navigation.other_api_groups.is_empty() {
                    let other_resources_node = "other-resources";
                    let response = sidebar.expandable_text(
                        "Other Resources",
                        ui_state.resource_navigation_node_is_expanded(other_resources_node),
                        |sidebar| {
                            for (api_group_name, api_resources) in
                                &resource_navigation.other_api_groups
                            {
                                let node_id = format!("other-resource-group:{api_group_name}");
                                let response = sidebar.nested_expandable_text(
                                    format!("other-{api_group_name}"),
                                    api_group_name,
                                    ui_state.resource_navigation_node_is_expanded(&node_id),
                                    |sidebar| {
                                        for api_resource in api_resources {
                                            let selected = selected_api_resource.as_ref()
                                                == Some(api_resource);
                                            if sidebar
                                                .nested_child_item(
                                                    api_resource.display_name(),
                                                    selected,
                                                )
                                                .clicked()
                                            {
                                                clicked_api_resource = Some(api_resource.clone());
                                            }
                                        }
                                    },
                                );
                                ui_state.set_resource_navigation_node_expanded(
                                    node_id,
                                    response.is_open,
                                );
                            }
                        },
                    );
                    ui_state.set_resource_navigation_node_expanded(
                        other_resources_node,
                        response.is_open,
                    );
                }
            });
        });

    clicked_api_resource
}
