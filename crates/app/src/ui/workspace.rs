use super::state::{ClusterConnectionState, PendingDelete, ResourceAction, UiState};
use super::widgets::{
    connection_status, display_resource_title, resource_status, workspace_empty_state,
};
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::worker::WorkerCommand;
use components::colors::{TOOLBAR_BACKGROUND, gray};
use components::{TableRowBuilder, TailwindCombobox, TailwindTable, WorkspacePage};

pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    let mut toggled_namespace = None;
    egui::CentralPanel::default()
        .frame(WorkspacePage::frame())
        .show(ctx, |ui| {
            WorkspacePage::show(ui, |ui| {
                let toolbar_rect = egui::Rect::from_min_size(
                    ui.max_rect().min,
                    egui::vec2(ui.available_width(), 102.0),
                );
                ui.painter()
                    .rect_filled(toolbar_rect, 0.0, TOOLBAR_BACKGROUND);

                let Some(selected_cluster_id) = ui_state.selected_cluster else {
                    workspace_empty_state(
                        ui,
                        "Choose a cluster",
                        "Select a Kubernetes context from the cluster rail to begin exploring.",
                    );
                    return;
                };
                let Some(cluster) = ui_state.clusters.get_mut(&selected_cluster_id) else {
                    workspace_empty_state(
                        ui,
                        "Choose a cluster",
                        "Select a Kubernetes context from the cluster rail to begin exploring.",
                    );
                    return;
                };

                let selected_api_resource = cluster.selected_api_resource.clone();
                let all_resources = selected_resources(cluster, selected_api_resource.as_ref());
                show_toolbar(
                    ui,
                    cluster,
                    selected_api_resource.as_ref(),
                    all_resources.len(),
                    &mut toggled_namespace,
                );
                ui.add_space(20.0);
                ui.separator();

                let Some(api_resource) = &selected_api_resource else {
                    workspace_empty_state(
                        ui,
                        "Select a resource",
                        "Choose an API resource from the navigator to inspect it.",
                    );
                    return;
                };
                if cluster.selected_namespaces.is_empty() {
                    workspace_empty_state(
                        ui,
                        "Choose a namespace",
                        "Select one or more namespaces to start watching resources.",
                    );
                } else if all_resources.is_empty() {
                    workspace_empty_state(
                        ui,
                        "No resources found",
                        "This resource type has no items in the selected namespace scope.",
                    );
                } else if let Some(action) = show_resource_table(
                    ui,
                    api_resource,
                    &all_resources,
                    cluster.selected_namespaces.len() > 1,
                ) {
                    match action {
                        ResourceAction::EditYaml { name, namespace } => {
                            commands_to_send.push(WorkerCommand::GetResourceYaml {
                                cluster_key: cluster.cluster_key,
                                api_resource: api_resource.clone(),
                                namespace,
                                resource_name: name,
                            });
                        }
                        ResourceAction::RequestDelete { name, namespace } => {
                            cluster.pending_delete = Some(PendingDelete {
                                resource_name: name,
                                namespace,
                            });
                        }
                    }
                }
            });
        });

    if let (Some(cluster_key), Some(namespace)) = (ui_state.selected_cluster, toggled_namespace) {
        ui_state.toggle_namespace(cluster_key, namespace, commands_to_send);
    }
}

fn selected_resources<'a>(
    cluster: &'a super::state::ClusterState,
    api_resource: Option<&crate::api_resource::ApiResource>,
) -> Vec<&'a MinimalResource> {
    let Some(api_resource) = api_resource else {
        return Vec::new();
    };
    let mut resources = Vec::new();
    for namespace in &cluster.selected_namespaces {
        if let Some(state) = cluster
            .resource_cache
            .get(&(api_resource.clone(), namespace.clone()))
        {
            resources.extend(state.resources.values());
        }
    }
    resources.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    resources
}

fn show_toolbar(
    ui: &mut egui::Ui,
    cluster: &super::state::ClusterState,
    selected_api_resource: Option<&crate::api_resource::ApiResource>,
    resource_count: usize,
    toggled_namespace: &mut Option<String>,
) {
    let resource_title = selected_api_resource
        .map(|resource| display_resource_title(&resource.name))
        .unwrap_or_else(|| "Resources".to_owned());
    let connection_label = match cluster.connection {
        ClusterConnectionState::Disconnected => "Not connected",
        ClusterConnectionState::Connecting => "Connecting",
        ClusterConnectionState::Connected(_) => "Connected",
    };
    let selected_text = match cluster.selected_namespaces.len() {
        0 => "Select namespaces".to_owned(),
        1 => cluster
            .selected_namespaces
            .iter()
            .next()
            .cloned()
            .unwrap_or_default(),
        count => format!("{count} namespaces"),
    };
    let namespaces: Vec<&MinimalNamespace> = cluster.namespaces.values().collect();

    ui.add_space(26.0);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 50.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add_space(37.0);
            ui.label(
                egui::RichText::new(&cluster.name)
                    .size(20.0)
                    .color(gray::_700),
            );
            ui.add_space(4.0);
            connection_status(ui, connection_label);
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new(resource_title)
                    .size(20.0)
                    .color(gray::_900),
            );
            if selected_api_resource.is_some() {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("{resource_count} items"))
                        .size(18.0)
                        .color(gray::_500),
                );
            }
            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Namespace")
                    .size(17.0)
                    .color(gray::_700),
            );
            ui.add_space(7.0);
            let namespace_response = TailwindCombobox::new("namespace-selector")
                .placeholder("Search namespaces...")
                .selected_text(selected_text)
                .width(230.0)
                .filter_by(|ns: &&MinimalNamespace| ns.get_name_to_display())
                .show_items(ui, &namespaces, |cb, ns| {
                    if cb
                        .item(
                            ns.get_name_to_display(),
                            cluster.selected_namespaces.contains(&ns.name),
                        )
                        .clicked()
                    {
                        *toggled_namespace = Some(ns.name.clone());
                    }
                });
            namespace_response.response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), "Namespace")
            });
        },
    );
}

fn show_resource_table(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    resources: &[&MinimalResource],
    show_namespace_column: bool,
) -> Option<ResourceAction> {
    let mut pending_action = None;
    let mut table = TailwindTable::new(format!("resource-table-{}", api_resource.name)).column(
        "name",
        "Name",
        |col| col.sortable().initial_width(798.0),
    );
    if show_namespace_column {
        table = table.column("namespace", "Namespace", |col| {
            col.sortable().initial_width(75.0)
        });
    }
    table = table
        .column("status", "Status", |col| {
            col.sortable().initial_width(124.0)
        })
        .column("ready", "Ready", |col| col.initial_width(95.0))
        .column("age", "Age", |col| col.sortable().initial_width(77.0))
        .column("actions", "", |col| col.initial_width(82.0))
        .fill_available_height();

    table.show(ui, resources, |ui, resource, column_index| {
        let (namespace_index, status_index, ready_index, age_index, actions_index) =
            if show_namespace_column {
                (Some(1), 2, 3, 4, 5)
            } else {
                (None, 1, 2, 3, 4)
            };
        match column_index {
            0 => TableRowBuilder::text(ui, &resource.name, true),
            index if Some(index) == namespace_index => {
                TableRowBuilder::text(ui, resource.namespace.as_deref().unwrap_or("-"), false);
            }
            index if index == status_index => resource_status(ui, resource.display_status()),
            index if index == ready_index => {
                TableRowBuilder::text(ui, resource.display_ready(), false)
            }
            index if index == age_index => TableRowBuilder::text(ui, &resource.age(), false),
            index if index == actions_index => {
                show_resource_actions(ui, resource, &mut pending_action)
            }
            _ => {}
        }
    });
    pending_action
}

fn show_resource_actions(
    ui: &mut egui::Ui,
    resource: &MinimalResource,
    pending_action: &mut Option<ResourceAction>,
) {
    let mut action_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(ui.max_rect().translate(egui::vec2(0.0, -6.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    #[allow(deprecated)]
    let menu = egui::menu::menu_custom_button(
        &mut action_ui,
        egui::Button::new("...")
            .min_size(egui::vec2(26.0, 29.0))
            .fill(egui::Color32::from_gray(238))
            .corner_radius(4),
        |ui| {
            if ui.button("Edit YAML").clicked() && pending_action.is_none() {
                *pending_action = Some(ResourceAction::EditYaml {
                    name: resource.name.clone(),
                    namespace: resource.namespace.clone().unwrap_or_default(),
                });
                ui.close();
            }
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("Delete").color(egui::Color32::from_rgb(185, 28, 28)),
                ))
                .clicked()
                && pending_action.is_none()
            {
                *pending_action = Some(ResourceAction::RequestDelete {
                    name: resource.name.clone(),
                    namespace: resource.namespace.clone().unwrap_or_default(),
                });
                ui.close();
            }
        },
    );
    menu.response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("More actions for {}", resource.name),
        )
    });
}
