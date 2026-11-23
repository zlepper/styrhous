use super::state::UiState;
use crate::api_resource::ApiResource;
use crate::resource_catalog::CuratedNavigationEntry;
use components::design::theme::{self, ThemeExt, ThemeScope};
use components::design::{size, space, type_scale};
use components::{SideItem, side_caption};

/// Width of the resource navigator beside the cluster rail.
const NAVIGATOR_WIDTH: f32 = 292.0;

pub(super) fn show(ui: &mut egui::Ui, ui_state: &mut UiState) -> Option<ApiResource> {
    let selected_cluster_id = ui_state.selected_cluster?;
    let cluster = ui_state.clusters.get(&selected_cluster_id)?;
    let cluster_name = cluster.presentation.display_name(&cluster.name).to_owned();
    let resource_navigation = cluster.resource_navigation.clone();
    let selected_api_resource = cluster.selected_api_resource.clone();
    let discovering = !cluster.api_discovery.complete
        && !matches!(
            cluster.connection,
            super::state::ClusterConnectionState::Failed(_)
        );
    let mut clicked_api_resource = None;
    let nav = ui.tokens().nav;
    let is_selected =
        |api_resource: &ApiResource| selected_api_resource.as_ref() == Some(api_resource);

    egui::Panel::left("api-selector")
        .exact_size(NAVIGATOR_WIDTH)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(nav.surface)
                .inner_margin(egui::Margin::same(size::SIDEBAR_PADDING as i8)),
        )
        .show(ui, |ui| {
            theme::scope(ui, ThemeScope::Sidebar, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                ui.add_space(space::S8);
                type_scale::HEADING.label(ui, &cluster_name, nav.text);
                ui.add_space(space::S12);
                components::scroll::vertical()
                    .id_salt("resource-navigator")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                        for entry in &resource_navigation.curated_entries {
                            match entry {
                                CuratedNavigationEntry::Resource(api_resource) => {
                                    if SideItem::plain(&api_resource.display_name())
                                        .selected(is_selected(api_resource))
                                        .show(ui)
                                        .clicked()
                                    {
                                        clicked_api_resource = Some(api_resource.clone());
                                    }
                                }
                                CuratedNavigationEntry::Section(section) => {
                                    let node_id = format!("section:{}", section.name);
                                    let open =
                                        ui_state.resource_navigation_node_is_expanded(&node_id);
                                    if SideItem::group(section.name, open).show(ui).clicked() {
                                        ui_state
                                            .set_resource_navigation_node_expanded(node_id, !open);
                                    }
                                    if open {
                                        for api_resource in &section.api_resources {
                                            if SideItem::child(&api_resource.display_name())
                                                .selected(is_selected(api_resource))
                                                .show(ui)
                                                .clicked()
                                            {
                                                clicked_api_resource = Some(api_resource.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !resource_navigation.other_api_groups.is_empty() {
                            let groups = resource_navigation.other_api_groups.len();
                            let detail = if groups == 1 {
                                "1 group".to_owned()
                            } else {
                                format!("{groups} groups")
                            };
                            side_caption(ui, "Other resources", Some(&detail));
                            for (api_group_name, api_resources) in
                                &resource_navigation.other_api_groups
                            {
                                let node_id = format!("other-resource-group:{api_group_name}");
                                let open = ui_state.resource_navigation_node_is_expanded(&node_id);
                                if SideItem::api_group(api_group_name, open)
                                    .count(api_resources.len())
                                    .show(ui)
                                    .clicked()
                                {
                                    ui_state.set_resource_navigation_node_expanded(node_id, !open);
                                }
                                if open {
                                    for api_resource in api_resources {
                                        if SideItem::child(&api_resource.display_name())
                                            .selected(is_selected(api_resource))
                                            .show(ui)
                                            .clicked()
                                        {
                                            clicked_api_resource = Some(api_resource.clone());
                                        }
                                    }
                                }
                            }
                        }
                        if discovering {
                            ui.add_space(space::S8);
                            components::StatusLabel::new(
                                "Discovering API groups…",
                                components::design::theme::Tone::Neutral,
                            )
                            .show(ui);
                        }
                    });
            });
        });

    clicked_api_resource
}
