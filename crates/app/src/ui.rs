mod dialogs;
mod state;
mod widgets;

use crate::api_resource::ApiResource;
use crate::helpers::SetExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::worker::{Worker, WorkerCommand, WorkerTrait};
use components::colors::{
    CLUSTER_RAIL_BACKGROUND, NAVIGATION_BACKGROUND, TOOLBAR_BACKGROUND, WHITE, gray, indigo,
};
use components::{
    NarrowSidebar, TableRowBuilder, TailwindCombobox, TailwindTable, WideSidebar, WorkspaceDrawer,
    WorkspacePage, apply_light_theme, semibold_font,
};
use dialogs::show_delete_confirmation;
use state::{ClusterConnectionState, PendingDelete, ResourceAction, UiState};
use tracing::info;
use widgets::{connection_status, display_resource_title, resource_status, workspace_empty_state};

pub struct MyEguiApp<W: WorkerTrait = Worker> {
    worker: W,
    ui_state: UiState,
}

impl<W: WorkerTrait> Default for MyEguiApp<W> {
    fn default() -> Self {
        Self {
            worker: W::default(),
            ui_state: UiState::default(),
        }
    }
}

impl<W: WorkerTrait> MyEguiApp<W> {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Install image loaders for SVG icons
        egui_extras::install_image_loaders(&cc.egui_ctx);
        apply_light_theme(&cc.egui_ctx);
        Self::default()
    }

    #[cfg(test)]
    pub fn with_worker(worker: W) -> Self {
        Self {
            worker,
            ui_state: UiState::default(),
        }
    }

    #[cfg(test)]
    pub fn select_cluster(&mut self, cluster_key: i32) {
        // Set the selected cluster
        self.ui_state.selected_cluster = Some(cluster_key);

        // Also trigger connection if disconnected (like the UI does on click)
        if let Some(cluster) = self.ui_state.clusters.get(&cluster_key) {
            if let ClusterConnectionState::Disconnected = cluster.connection {
                self.worker.send_command(WorkerCommand::ConnectToCluster {
                    cluster: cluster.name.clone(),
                    cluster_key,
                });
            }
        }
    }

    #[cfg(test)]
    pub fn select_namespace(&mut self, cluster_key: i32, namespace: &str) {
        if let Some(cluster) = self.ui_state.clusters.get_mut(&cluster_key) {
            cluster.selected_namespaces.insert(namespace.to_string());
        }
    }

    #[cfg(test)]
    pub fn select_api_resource(&mut self, cluster_key: i32, api_resource: ApiResource) {
        if let Some(cluster) = self.ui_state.clusters.get_mut(&cluster_key) {
            // Start watcher for selected namespaces
            for namespace in cluster.selected_namespaces.clone() {
                let key = (api_resource.clone(), namespace.clone());
                if !cluster.active_watchers.contains(&key) {
                    self.worker.send_command(WorkerCommand::StartResourceWatch {
                        cluster_key,
                        api_resource: api_resource.clone(),
                        namespace: namespace.clone(),
                    });
                }
            }
            cluster.selected_api_resource = Some(api_resource);
        }
    }
}

impl<W: WorkerTrait> eframe::App for MyEguiApp<W> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.worker.start();

        self.ui_state.update(&mut self.worker);

        // Cluster rail: the deliberately compact, always-visible cluster switcher.
        egui::SidePanel::left("cluster-panel")
            .exact_width(68.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(CLUSTER_RAIL_BACKGROUND))
            .show(ctx, |ui| {
                NarrowSidebar::new()
                    .dark_background(CLUSTER_RAIL_BACKGROUND)
                    .show(ui, |sidebar| {
                        let mut cluster_keys: Vec<_> =
                            self.ui_state.clusters.keys().copied().collect();
                        cluster_keys.sort_unstable();
                        for cluster_key in cluster_keys {
                            let Some(cluster) = self.ui_state.clusters.get(&cluster_key) else {
                                continue;
                            };
                            let initial = cluster
                                .name
                                .chars()
                                .next()
                                .unwrap_or('?')
                                .to_uppercase()
                                .to_string();
                            let selected = self.ui_state.selected_cluster == Some(cluster_key);

                            if sidebar
                                .avatar_item(&cluster.name, &initial, selected)
                                .clicked()
                            {
                                info!("Cluster '{}' selected", cluster.name);
                                if let ClusterConnectionState::Disconnected = cluster.connection {
                                    info!("Connecting to cluster");
                                    self.worker.send_command(WorkerCommand::ConnectToCluster {
                                        cluster: cluster.name.clone(),
                                        cluster_key,
                                    });
                                }
                                self.ui_state.selected_cluster = Some(cluster_key);
                            }
                        }
                    });
            });

        // Track clicked API resource to apply mutation after UI rendering.
        let mut clicked_api_resource: Option<ApiResource> = None;

        // Resource navigation belongs to the selected cluster, so it only appears once a
        // cluster has been chosen.
        if let Some(selected_cluster_id) = self.ui_state.selected_cluster {
            if let Some(cluster) = self.ui_state.clusters.get(&selected_cluster_id) {
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
                            for (api_group_name, api_resources) in &cluster.api_resource_groups {
                                let display_name = if api_group_name.is_empty() {
                                    "core"
                                } else {
                                    api_group_name.as_str()
                                };

                                sidebar.expandable_text(display_name, false, |sidebar| {
                                    for api_resource in &api_resources.api_resources {
                                        let selected = cluster
                                            .selected_api_resource
                                            .as_ref()
                                            .is_some_and(|r| r == api_resource);

                                        if sidebar
                                            .child_item(&api_resource.name, selected)
                                            .clicked()
                                        {
                                            clicked_api_resource = Some(api_resource.clone());
                                        }
                                    }
                                });
                            }
                        });
                    });
            }
        }

        // Track commands to send (deferred to avoid borrow issues).
        let mut commands_to_send: Vec<WorkerCommand> = Vec::new();
        let mut close_yaml_panel = false;

        // The editor is a dark, dedicated drawer rather than an unrelated white pane.
        if let Some(selected_cluster_id) = self.ui_state.selected_cluster {
            if let Some(cluster) = self.ui_state.clusters.get_mut(&selected_cluster_id) {
                if let Some(ref mut yaml_panel) = cluster.yaml_panel {
                    egui::TopBottomPanel::bottom("yaml-panel")
                        .resizable(true)
                        .min_height(100.0)
                        .default_height(yaml_panel.panel_height)
                        .frame(WorkspaceDrawer::frame())
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Edit YAML · {}",
                                        yaml_panel.resource_name
                                    ))
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

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let close_button = ui.add(
                                            egui::Button::new(
                                                egui::RichText::new("Close YAML Editor")
                                                    .color(gray::_100),
                                            )
                                            .fill(gray::_700),
                                        );
                                        if close_button.clicked() {
                                            if yaml_panel.is_modified() {
                                                info!("Discarding unsaved YAML changes");
                                            }
                                            close_yaml_panel = true;
                                        }

                                        let save_button = ui.add_enabled(
                                            yaml_panel.is_modified(),
                                            egui::Button::new(
                                                egui::RichText::new("Save YAML").color(WHITE),
                                            )
                                            .fill(indigo::_600),
                                        );
                                        if save_button.clicked() {
                                            commands_to_send.push(
                                                WorkerCommand::ApplyResourceYaml {
                                                    cluster_key: cluster.cluster_key,
                                                    api_resource: yaml_panel.api_resource.clone(),
                                                    namespace: yaml_panel.namespace.clone(),
                                                    resource_name: yaml_panel.resource_name.clone(),
                                                    yaml: yaml_panel.edited_yaml.clone(),
                                                },
                                            );
                                        }

                                        if yaml_panel.is_modified() {
                                            ui.label(
                                                egui::RichText::new("Modified")
                                                    .color(egui::Color32::from_rgb(234, 179, 8))
                                                    .size(12.0),
                                            );
                                        }
                                    },
                                );
                            });

                            ui.painter().line_segment(
                                [ui.min_rect().left_bottom(), ui.min_rect().right_bottom()],
                                egui::Stroke::new(1.0, gray::_700),
                            );
                            ui.add_space(8.0);

                            egui::ScrollArea::both()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let editor =
                                        egui::TextEdit::multiline(&mut yaml_panel.edited_yaml)
                                            .font(egui::TextStyle::Monospace)
                                            .code_editor()
                                            .text_color(WorkspaceDrawer::text_color())
                                            .background_color(WorkspaceDrawer::editor_background())
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(20)
                                            .hint_text("YAML Editor");
                                    ui.add(editor);
                                });
                        });
                }
            }
        }

        if close_yaml_panel {
            if let Some(selected_cluster_id) = self.ui_state.selected_cluster {
                if let Some(cluster) = self.ui_state.clusters.get_mut(&selected_cluster_id) {
                    cluster.yaml_panel = None;
                }
            }
        }

        egui::CentralPanel::default()
            .frame(WorkspacePage::frame())
            .show(ctx, |ui| {
            WorkspacePage::show(ui, |ui| {
                let toolbar_rect = egui::Rect::from_min_size(
                    ui.max_rect().min,
                    egui::vec2(ui.available_width(), 102.0),
                );
                ui.painter().rect_filled(toolbar_rect, 0.0, TOOLBAR_BACKGROUND);
                if let Some(selected_cluster_id) = self.ui_state.selected_cluster {
                    if let Some(cluster) = self.ui_state.clusters.get_mut(&selected_cluster_id) {
                        let selected_api_resource = cluster.selected_api_resource.clone();
                        let mut all_resources: Vec<&MinimalResource> = Vec::new();
                        if let Some(api_resource) = &selected_api_resource {
                            for namespace in &cluster.selected_namespaces {
                                let key = (api_resource.clone(), namespace.clone());
                                if let Some(state) = cluster.resource_cache.get(&key) {
                                    all_resources.extend(state.resources.values());
                                }
                            }
                            all_resources.sort_by(|a, b| {
                                a.name.to_lowercase().cmp(&b.name.to_lowercase())
                            });
                        }

                        let resource_title = selected_api_resource
                            .as_ref()
                            .map(|resource| display_resource_title(&resource.name))
                            .unwrap_or_else(|| "Resources".to_owned());
                        let connection_label = match cluster.connection {
                            ClusterConnectionState::Disconnected => "Not connected",
                            ClusterConnectionState::Connecting => "Connecting",
                            ClusterConnectionState::Connected(_) => "Connected",
                        };

                        let selected_text = match cluster.selected_namespaces.len() {
                            0 => "Select namespaces".to_owned(),
                            1 => cluster.selected_namespaces.iter().next().cloned().unwrap_or_default(),
                            count => format!("{count} namespaces"),
                        };
                        let namespaces: Vec<_> = cluster.namespaces.values().collect();

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
                                    egui::RichText::new(format!("{} items", all_resources.len()))
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
                                    let is_selected = cluster.selected_namespaces.contains(&ns.name);
                                    if cb.item(ns.get_name_to_display(), is_selected).clicked() {
                                        let was_selected = cluster.selected_namespaces.contains(&ns.name);
                                        cluster.selected_namespaces.toggle(ns.name.clone());
                                        if !was_selected {
                                            if let Some(api_resource) = &cluster.selected_api_resource {
                                                let key = (api_resource.clone(), ns.name.clone());
                                                if !cluster.active_watchers.contains(&key) {
                                                    commands_to_send.push(WorkerCommand::StartResourceWatch {
                                                        cluster_key: cluster.cluster_key,
                                                        api_resource: api_resource.clone(),
                                                        namespace: ns.name.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                });
                            namespace_response.response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::ComboBox,
                                    ui.is_enabled(),
                                    "Namespace",
                                )
                            });
                            },
                        );
                        ui.add_space(20.0);
                        ui.separator();

                        if let Some(api_resource) = &selected_api_resource {
                            let show_namespace_column = cluster.selected_namespaces.len() > 1;

                            if cluster.selected_namespaces.is_empty() {
                                workspace_empty_state(ui, "Choose a namespace", "Select one or more namespaces to start watching resources.");
                            } else if all_resources.is_empty() {
                                workspace_empty_state(ui, "No resources found", "This resource type has no items in the selected namespace scope.");
                            } else {
                                let mut pending_action: Option<ResourceAction> = None;

                                let mut table = TailwindTable::new(format!("resource-table-{}", api_resource.name))
                                    .column("name", "Name", |col| {
                                        col.sortable().initial_width(798.0)
                                    });

                                if show_namespace_column {
                                    table = table.column("namespace", "Namespace", |col| col.sortable().initial_width(75.0));
                                }

                                table = table
                                    .column("status", "Status", |col| {
                                        col.sortable().initial_width(124.0)
                                    })
                                    .column("ready", "Ready", |col| col.initial_width(95.0))
                                    .column("age", "Age", |col| {
                                        col.sortable().initial_width(77.0)
                                    })
                                    .column("actions", "", |col| col.initial_width(82.0))
                                    .fill_available_height();

                                table.show(ui, &all_resources, |ui, resource, col_index| {
                                        let (name_idx, ns_idx, status_idx, ready_idx, age_idx, actions_idx) = if show_namespace_column {
                                            (0, Some(1), 2, 3, 4, 5)
                                        } else {
                                            (0, None, 1, 2, 3, 4)
                                        };

                                        if col_index == name_idx {
                                            TableRowBuilder::text(ui, &resource.name, true);
                                        } else if Some(col_index) == ns_idx {
                                            TableRowBuilder::text(ui, resource.namespace.as_deref().unwrap_or("-"), false);
                                        } else if col_index == status_idx {
                                            resource_status(ui, resource.display_status());
                                        } else if col_index == ready_idx {
                                            TableRowBuilder::text(ui, resource.display_ready(), false);
                                        } else if col_index == age_idx {
                                            TableRowBuilder::text(ui, &resource.age(), false);
                                        } else if col_index == actions_idx {
                                            let mut action_ui = ui.new_child(
                                                egui::UiBuilder::new()
                                                    .max_rect(
                                                        ui.max_rect()
                                                            .translate(egui::vec2(0.0, -6.0)),
                                                    )
                                                    .layout(egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    )),
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
                                                    pending_action = Some(ResourceAction::EditYaml {
                                                        name: resource.name.clone(),
                                                        namespace: resource.namespace.clone().unwrap_or_default(),
                                                    });
                                                    ui.close();
                                                }
                                                if ui.add(egui::Button::new(
                                                    egui::RichText::new("Delete").color(egui::Color32::from_rgb(185, 28, 28)),
                                                )).clicked() && pending_action.is_none() {
                                                    pending_action = Some(ResourceAction::RequestDelete {
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
                                    });

                                if let Some(action) = pending_action {
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
                            }
                        } else {
                            workspace_empty_state(ui, "Select a resource", "Choose an API resource from the navigator to inspect it.");
                        }
                    }
                } else {
                    workspace_empty_state(ui, "Choose a cluster", "Select a Kubernetes context from the cluster rail to begin exploring.");
                }
            });
            });

        show_delete_confirmation(ctx, &mut self.ui_state, &mut commands_to_send);

        // Apply clicked API resource selection and start watchers after the tree has rendered.
        if let (Some(selected_cluster_id), Some(api_resource)) =
            (self.ui_state.selected_cluster, clicked_api_resource)
        {
            if let Some(cluster) = self.ui_state.clusters.get_mut(&selected_cluster_id) {
                for namespace in &cluster.selected_namespaces {
                    let key = (api_resource.clone(), namespace.clone());
                    if !cluster.active_watchers.contains(&key) {
                        commands_to_send.push(WorkerCommand::StartResourceWatch {
                            cluster_key: cluster.cluster_key,
                            api_resource: api_resource.clone(),
                            namespace: namespace.clone(),
                        });
                    }
                }
                cluster.selected_api_resource = Some(api_resource);
            }
        }

        // Send deferred commands
        for command in commands_to_send {
            self.worker.send_command(command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::state::{ApiResourceGroupState, ClusterState, ResourceWatchState};
    use super::*;
    use crate::cluster_connection_manager::Cluster;
    use crate::worker::{MockWorker, WorkerResult};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use std::collections::{BTreeMap, HashMap, HashSet};

    const APP_SNAPSHOT_SIZE: egui::Vec2 = egui::vec2(1536.0, 1024.0);

    fn application_harness<W: WorkerTrait>() -> Harness<'static, MyEguiApp<W>> {
        Harness::builder()
            .with_size(APP_SNAPSHOT_SIZE)
            .build_eframe(|cc| MyEguiApp::<W>::new(cc))
    }

    fn fixture_cluster(cluster_key: i32, name: &str) -> ClusterState {
        ClusterState {
            name: name.into(),
            cluster: None,
            cluster_key,
            namespaces: BTreeMap::new(),
            connection: ClusterConnectionState::Disconnected,
            selected_namespaces: HashSet::new(),
            api_resource_groups: BTreeMap::new(),
            selected_api_resource: None,
            resource_cache: HashMap::new(),
            active_watchers: HashSet::new(),
            yaml_panel: None,
            pending_delete: None,
        }
    }

    fn fixture_api_resource(group: &str, kind: &str, name: &str) -> ApiResource {
        ApiResource {
            group: group.into(),
            version: "v1".into(),
            kind: kind.into(),
            name: name.into(),
        }
    }

    fn fixture_resource(index: usize, name: &str) -> MinimalResource {
        MinimalResource {
            uid: format!("fixture-{index}"),
            name: name.into(),
            namespace: Some("kube-system".into()),
            creation_timestamp: Some(time::OffsetDateTime::now_utc() - time::Duration::days(220)),
            phase: Some("Running".into()),
            ready_status: Some("1/1".into()),
        }
    }

    fn oracle_resource_table_state() -> UiState {
        let pods = fixture_api_resource("core", "Pod", "pods");
        let core_resources = [
            ("Binding", "bindings"),
            ("ComponentStatus", "componentstatuses"),
            ("ConfigMap", "configmaps"),
            ("Endpoints", "endpoints"),
            ("Event", "events"),
            ("LimitRange", "limitranges"),
            ("Namespace", "namespaces"),
            ("Node", "nodes"),
            ("PersistentVolumeClaim", "persistentvolumeclaims"),
            ("PersistentVolume", "persistentvolumes"),
            ("Pod", "pods"),
            ("PodTemplate", "podtemplates"),
            ("ReplicationController", "replicationcontrollers"),
            ("ResourceQuota", "resourcequotas"),
            ("Secret", "secrets"),
        ]
        .into_iter()
        .map(|(kind, name)| fixture_api_resource("core", kind, name))
        .collect();

        let mut kind = fixture_cluster(2, "kind-kind");
        kind.connection = ClusterConnectionState::Connected(None);
        kind.namespaces.insert(
            "kube-system".into(),
            MinimalNamespace {
                name: "kube-system".into(),
                display_name: None,
            },
        );
        kind.selected_namespaces.insert("kube-system".into());
        kind.selected_api_resource = Some(pods.clone());
        kind.api_resource_groups = BTreeMap::from([
            (
                "apps".into(),
                ApiResourceGroupState {
                    open: false,
                    api_resources: vec![fixture_api_resource("apps", "Deployment", "deployments")],
                },
            ),
            (
                "autoscaling".into(),
                ApiResourceGroupState {
                    open: false,
                    api_resources: vec![fixture_api_resource(
                        "autoscaling",
                        "HorizontalPodAutoscaler",
                        "horizontalpodautoscalers",
                    )],
                },
            ),
            (
                "batch".into(),
                ApiResourceGroupState {
                    open: false,
                    api_resources: vec![fixture_api_resource("batch", "Job", "jobs")],
                },
            ),
            (
                "core".into(),
                ApiResourceGroupState {
                    open: true,
                    api_resources: core_resources,
                },
            ),
        ]);
        kind.resource_cache.insert(
            (pods, "kube-system".into()),
            ResourceWatchState {
                resources: [
                    "coredns-66bc5c9577-ffw2s",
                    "coredns-66bc5c9577-z9gt9",
                    "etcd-kind-control-plane",
                    "kindnet-9qrlh",
                    "kube-apiserver-kind-control-plane",
                    "kube-controller-manager-kind-control-plane",
                    "kube-proxy-v86gd",
                    "kube-scheduler-kind-control-plane",
                ]
                .into_iter()
                .enumerate()
                .map(|(index, name)| (format!("fixture-{index}"), fixture_resource(index, name)))
                .collect(),
                is_synced: true,
            },
        );

        UiState {
            clusters: HashMap::from([
                (1, fixture_cluster(1, "dev")),
                (2, kind),
                (3, fixture_cluster(3, "kube-local")),
            ]),
            next_cluster_key: 3,
            selected_cluster: Some(2),
        }
    }

    #[test]
    fn oracle_resource_table_snapshot_uses_injected_cluster_state() {
        let mut harness = application_harness::<MockWorker>();
        harness.state_mut().ui_state = oracle_resource_table_state();
        harness.run();
        harness.get_by_label("core").click_accesskit();
        harness.run();

        harness.snapshot("oracle_resource_table_injected");
    }

    #[test]
    fn delete_confirmation_can_be_cancelled_without_sending_a_command() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let api_resource = ApiResource {
            group: "".into(),
            version: "v1".into(),
            kind: "ConfigMap".into(),
            name: "configmaps".into(),
        };
        let state = Rc::new(RefCell::new(UiState {
            clusters: HashMap::from([(
                1,
                ClusterState {
                    name: "dev".into(),
                    cluster: None,
                    cluster_key: 1,
                    namespaces: BTreeMap::new(),
                    connection: ClusterConnectionState::Disconnected,
                    selected_namespaces: HashSet::new(),
                    api_resource_groups: BTreeMap::new(),
                    selected_api_resource: Some(api_resource),
                    resource_cache: HashMap::new(),
                    active_watchers: HashSet::new(),
                    yaml_panel: None,
                    pending_delete: Some(PendingDelete {
                        resource_name: "important-config".into(),
                        namespace: "default".into(),
                    }),
                },
            )]),
            next_cluster_key: 1,
            selected_cluster: Some(1),
        }));
        let commands = Rc::new(RefCell::new(Vec::new()));
        let state_for_ui = state.clone();
        let commands_for_ui = commands.clone();

        let mut harness = Harness::new_ui(move |ui| {
            show_delete_confirmation(
                ui.ctx(),
                &mut state_for_ui.borrow_mut(),
                &mut commands_for_ui.borrow_mut(),
            );
        });

        harness.run();
        harness.get_by_label("Cancel").click_accesskit();
        harness.run();

        assert!(commands.borrow().is_empty());
        assert!(
            state
                .borrow()
                .clusters
                .get(&1)
                .and_then(|cluster| cluster.pending_delete.as_ref())
                .is_none()
        );
    }

    #[test]
    fn test_ui_flow() {
        let mut harness = application_harness::<MockWorker>();

        // Install image loaders for SVG icons in tests
        egui_extras::install_image_loaders(&harness.ctx);

        // Initial empty state
        harness.run();
        harness.snapshot("01_empty_state");

        // Clusters arrive from worker
        harness
            .state_mut()
            .worker
            .results
            .push_back(WorkerResult::KubernetesClustersUpdated(vec![
                Cluster {
                    name: "dev".into(),
                    cluster: None,
                },
                Cluster {
                    name: "prod".into(),
                    cluster: Some("production".into()),
                },
            ]));
        harness.run();
        harness.snapshot("02_clusters_loaded");

        // Select the dev cluster (key 1)
        harness.state_mut().select_cluster(1);
        harness.run();
        harness.snapshot("03_cluster_selected_empty");

        // Add namespaces
        harness
            .state_mut()
            .worker
            .results
            .push_back(WorkerResult::KubernetesNamespacesReplaced {
                cluster_key: 1,
                namespaces: vec![
                    MinimalNamespace {
                        name: "default".into(),
                        display_name: None,
                    },
                    MinimalNamespace {
                        name: "kube-system".into(),
                        display_name: None,
                    },
                    MinimalNamespace {
                        name: "monitoring".into(),
                        display_name: Some("Monitoring Stack".into()),
                    },
                ],
            });
        harness.run();
        harness.snapshot("04_namespaces_loaded");

        // Add API resources
        harness
            .state_mut()
            .worker
            .results
            .push_back(WorkerResult::KubernetesApisLoaded {
                cluster_key: 1,
                api_resources: vec![
                    // Core resources (empty group displayed as "core")
                    ApiResource {
                        group: "".into(),
                        version: "v1".into(),
                        kind: "Pod".into(),
                        name: "pods".into(),
                    },
                    ApiResource {
                        group: "".into(),
                        version: "v1".into(),
                        kind: "Service".into(),
                        name: "services".into(),
                    },
                    ApiResource {
                        group: "".into(),
                        version: "v1".into(),
                        kind: "ConfigMap".into(),
                        name: "configmaps".into(),
                    },
                    // apps group
                    ApiResource {
                        group: "apps".into(),
                        version: "v1".into(),
                        kind: "Deployment".into(),
                        name: "deployments".into(),
                    },
                    ApiResource {
                        group: "apps".into(),
                        version: "v1".into(),
                        kind: "StatefulSet".into(),
                        name: "statefulsets".into(),
                    },
                    // networking.k8s.io group
                    ApiResource {
                        group: "networking.k8s.io".into(),
                        version: "v1".into(),
                        kind: "Ingress".into(),
                        name: "ingresses".into(),
                    },
                ],
            });
        harness.run();
        harness.snapshot("05_api_resources_loaded");
    }

    #[test]
    fn test_real_cluster_connection() {
        let mut harness = application_harness::<Worker>();

        // Install image loaders for SVG icons
        egui_extras::install_image_loaders(&harness.ctx);

        // Run multiple frames to allow worker to start and load clusters
        for _ in 0..10 {
            harness.run();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        harness.snapshot("real_clusters");
    }

    /// Integration test for resource watcher using accessibility-based UI interactions.
    /// Nextest creates or reuses the default Kind cluster before running this test.
    /// Run with: cargo nextest run -p kubernetes-dev-ui test_resource_watcher_integration
    #[test]
    fn test_resource_watcher_integration() {
        let mut harness = application_harness::<Worker>();
        egui_extras::install_image_loaders(&harness.ctx);

        // Helper: run frames with sleep until condition is met or timeout
        fn wait_for<T>(
            harness: &mut Harness<MyEguiApp<Worker>>,
            condition: impl Fn(&MyEguiApp<Worker>) -> Option<T>,
            max_ms: u64,
        ) -> Option<T> {
            let start = std::time::Instant::now();
            while start.elapsed().as_millis() < max_ms as u128 {
                harness.run();
                if let Some(result) = condition(harness.state()) {
                    return Some(result);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            None
        }

        // 1. Wait for clusters to load
        wait_for(
            &mut harness,
            |app| {
                if app.ui_state.clusters.is_empty() {
                    None
                } else {
                    Some(())
                }
            },
            5000,
        )
        .expect("Clusters should load");

        // 2. Click on the Kind cluster via accessibility
        harness.get_by_label("kind-kind").click();
        harness.run();

        // Get cluster_key for later use
        let cluster_key = harness
            .state()
            .ui_state
            .selected_cluster
            .expect("Cluster should be selected after click");

        // 3. Wait for namespaces to load
        wait_for(
            &mut harness,
            |app| {
                let cluster = app.ui_state.clusters.get(&cluster_key);
                if let Some(c) = cluster {
                    if !c.namespaces.is_empty() {
                        return Some(());
                    }
                }
                None
            },
            10000,
        )
        .expect("Namespaces should load");

        // 4. Wait for API resources to load
        wait_for(
            &mut harness,
            |app| {
                app.ui_state
                    .clusters
                    .get(&cluster_key)
                    .filter(|c| !c.api_resource_groups.is_empty())
                    .map(|_| ())
            },
            5000,
        )
        .expect("API resources should load");

        // 5. Click on namespace combobox to open it
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
            .click();
        harness.run();

        // 6. Click on kube-system namespace
        harness.get_by_label("kube-system").click();
        harness.run();

        // Verify namespace was selected
        let namespaces_selected = harness
            .state()
            .ui_state
            .clusters
            .get(&cluster_key)
            .map(|c| c.selected_namespaces.contains("kube-system"))
            .unwrap_or(false);
        assert!(
            namespaces_selected,
            "kube-system namespace should be selected after click. Selected: {:?}",
            harness
                .state()
                .ui_state
                .clusters
                .get(&cluster_key)
                .map(|c| &c.selected_namespaces)
        );

        // 7. Click on "core" group to expand it (it should default to closed)
        harness.get_by_label("core").click_accesskit();
        harness.run();
        harness.run(); // Extra run to ensure expandable section is fully rendered

        // 8. The native oracle viewport keeps pods visible after expanding core.
        // Use the regular pointer interaction so the snapshot preserves the tree's
        // natural top-of-list scroll position.
        harness.get_by_label("pods").click();
        harness.run();

        // Use the actual selected API resource (group/version may differ from hardcoded values)
        let pods_resource = harness
            .state()
            .ui_state
            .clusters
            .get(&cluster_key)
            .and_then(|c| c.selected_api_resource.clone())
            .expect("pods API resource should be selected");

        // 9. Wait for resources to sync
        wait_for(
            &mut harness,
            |app| {
                app.ui_state.selected_cluster.and_then(|k| {
                    app.ui_state
                        .clusters
                        .get(&k)
                        .and_then(|c| {
                            c.resource_cache
                                .get(&(pods_resource.clone(), "kube-system".to_string()))
                        })
                        .filter(|s| s.is_synced)
                        .map(|_| ())
                })
            },
            10000,
        )
        .expect("Resources should sync");

        // 10. Verify we have pods
        let resource_count = harness
            .state()
            .ui_state
            .selected_cluster
            .and_then(|k| harness.state().ui_state.clusters.get(&k))
            .and_then(|c| {
                c.resource_cache
                    .get(&(pods_resource.clone(), "kube-system".to_string()))
            })
            .map(|s| s.resources.len())
            .unwrap_or(0);

        assert!(
            resource_count > 0,
            "Should have at least one pod, got {}",
            resource_count
        );

        // 11. Check for known pods (coredns is always in kube-system on Kind)
        let has_coredns = harness
            .state()
            .ui_state
            .selected_cluster
            .and_then(|k| harness.state().ui_state.clusters.get(&k))
            .and_then(|c| {
                c.resource_cache
                    .get(&(pods_resource.clone(), "kube-system".to_string()))
            })
            .map(|s| s.resources.values().any(|r| r.name.starts_with("coredns")))
            .unwrap_or(false);

        assert!(has_coredns, "Should have coredns pod");

        // 12. Take a snapshot for visual verification
        harness.snapshot("integration_resource_table");
    }

    /// Integration test for resource actions (Edit YAML, Delete) against a real Kind cluster.
    /// Creates a test ConfigMap, edits it, then deletes it.
    /// Nextest creates or reuses the default Kind cluster before running this test.
    /// Run with: cargo nextest run -p kubernetes-dev-ui test_resource_actions_integration
    #[test]
    fn test_resource_actions_integration() {
        use k8s_openapi::api::core::v1::ConfigMap;
        use kube::{Api, Client};
        use std::collections::BTreeMap;

        // Create a tokio runtime for direct kube-rs operations
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        // Use a deterministic name so snapshots don't change on every run
        let test_configmap_name = "test-cm-integration".to_string();

        let client = rt.block_on(async {
            Client::try_default()
                .await
                .expect("Failed to create kube client")
        });

        let configmaps: Api<ConfigMap> = Api::namespaced(client.clone(), "default");

        // Cleanup: Delete the test ConfigMap if it exists from a previous run
        rt.block_on(async {
            let _ = configmaps
                .delete(&test_configmap_name, &Default::default())
                .await;
        });

        // Create the test ConfigMap
        let test_cm = ConfigMap {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some(test_configmap_name.clone()),
                namespace: Some("default".to_string()),
                ..Default::default()
            },
            data: Some({
                let mut data = BTreeMap::new();
                data.insert("key1".to_string(), "original-value".to_string());
                data
            }),
            ..Default::default()
        };

        rt.block_on(async {
            configmaps
                .create(&Default::default(), &test_cm)
                .await
                .expect("Failed to create test ConfigMap");
        });

        // Start the UI test
        // Note: The test deletes the ConfigMap as part of testing delete functionality
        let mut harness = application_harness::<Worker>();
        egui_extras::install_image_loaders(&harness.ctx);

        // Helper: run frames with sleep until condition is met or timeout
        fn wait_for<T>(
            harness: &mut Harness<MyEguiApp<Worker>>,
            condition: impl Fn(&MyEguiApp<Worker>) -> Option<T>,
            max_ms: u64,
        ) -> Option<T> {
            let start = std::time::Instant::now();
            while start.elapsed().as_millis() < max_ms as u128 {
                harness.run();
                if let Some(result) = condition(harness.state()) {
                    return Some(result);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            None
        }

        // 1. Wait for clusters to load
        wait_for(
            &mut harness,
            |app| {
                if app.ui_state.clusters.is_empty() {
                    None
                } else {
                    Some(())
                }
            },
            5000,
        )
        .expect("Clusters should load");

        // 2. Click on the Kind cluster
        harness.get_by_label("kind-kind").click();
        harness.run();

        let cluster_key = harness
            .state()
            .ui_state
            .selected_cluster
            .expect("Cluster should be selected");

        // 3. Wait for namespaces and API resources to load
        wait_for(
            &mut harness,
            |app| {
                app.ui_state
                    .clusters
                    .get(&cluster_key)
                    .filter(|c| !c.namespaces.is_empty() && !c.api_resource_groups.is_empty())
                    .map(|_| ())
            },
            10000,
        )
        .expect("Namespaces and API resources should load");

        // 4. Select "default" namespace
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
            .click();
        harness.run();
        harness.get_by_label("default").click();
        harness.run();

        // 5. Expand "core" group and select "configmaps"
        harness.get_by_label("core").click();
        harness.run();
        harness.run();

        harness.get_by_label("configmaps").click_accesskit();
        harness.run();

        // Get the actual configmaps API resource
        let configmaps_resource = harness
            .state()
            .ui_state
            .clusters
            .get(&cluster_key)
            .and_then(|c| c.selected_api_resource.clone())
            .expect("configmaps API resource should be selected");

        // 6. Wait for ConfigMaps to sync
        wait_for(
            &mut harness,
            |app| {
                app.ui_state
                    .clusters
                    .get(&cluster_key)
                    .and_then(|c| {
                        c.resource_cache
                            .get(&(configmaps_resource.clone(), "default".to_string()))
                    })
                    .filter(|s| s.is_synced)
                    .map(|_| ())
            },
            10000,
        )
        .expect("ConfigMaps should sync");

        // 7. Verify our test ConfigMap appears
        let has_test_cm = harness
            .state()
            .ui_state
            .clusters
            .get(&cluster_key)
            .and_then(|c| {
                c.resource_cache
                    .get(&(configmaps_resource.clone(), "default".to_string()))
            })
            .map(|s| s.resources.values().any(|r| r.name == test_configmap_name))
            .unwrap_or(false);

        assert!(
            has_test_cm,
            "Test ConfigMap '{}' should appear in the resource list",
            test_configmap_name
        );

        // Run extra frames to ensure table is fully rendered with accessibility info
        for _ in 0..3 {
            harness.run();
        }

        // 8. Open the row actions menu and edit the ConfigMap through it.
        let actions_button_label = format!("More actions for {}", test_configmap_name);
        harness
            .get_by_label(&actions_button_label)
            .click_accesskit();
        harness.run();
        harness.get_by_label("Edit YAML").click_accesskit();
        harness.run();

        // Wait for YAML panel to open
        wait_for(
            &mut harness,
            |app| {
                app.ui_state
                    .clusters
                    .get(&cluster_key)
                    .and_then(|c| c.yaml_panel.as_ref())
                    .filter(|p| p.resource_name == test_configmap_name)
                    .map(|_| ())
            },
            5000,
        )
        .expect("YAML panel should open after clicking Edit button");

        // 9. Modify the YAML content
        // Note: We modify the state directly since text selection/replacement via kittest
        // is complex. The UI button clicks are the critical integration points.
        {
            let cluster = harness
                .state_mut()
                .ui_state
                .clusters
                .get_mut(&cluster_key)
                .unwrap();
            if let Some(ref mut panel) = cluster.yaml_panel {
                panel.edited_yaml = panel.edited_yaml.replace("original-value", "edited-value");
            }
        }
        harness.run();

        // 10. Click Save button (real UI click)
        harness.get_by_label("Save YAML").click();
        harness.run();

        // Wait for apply to complete (panel closes)
        wait_for(
            &mut harness,
            |app| {
                app.ui_state
                    .clusters
                    .get(&cluster_key)
                    .filter(|c| c.yaml_panel.is_none())
                    .map(|_| ())
            },
            5000,
        )
        .expect("YAML panel should close after clicking Save");

        // 11. Verify the change persisted via kube-rs
        let cm_after_edit = rt.block_on(async {
            configmaps
                .get(&test_configmap_name)
                .await
                .expect("Failed to get ConfigMap after edit")
        });

        let edited_value = cm_after_edit
            .data
            .as_ref()
            .and_then(|d| d.get("key1"))
            .map(|s| s.as_str());

        assert_eq!(
            edited_value,
            Some("edited-value"),
            "ConfigMap should have edited value, got: {:?}",
            edited_value
        );

        // Run extra frames to ensure table is re-rendered after save
        for _ in 0..5 {
            harness.run();
        }

        // 12. Request deletion from the row actions menu.
        let actions_button_label = format!("More actions for {}", test_configmap_name);
        harness
            .get_by_label(&actions_button_label)
            .click_accesskit();
        harness.run();

        harness.get_by_label("Delete").click_accesskit();
        harness.run();

        // Verify the explicit confirmation dialog is now open.
        let is_pending = harness
            .state()
            .ui_state
            .clusters
            .get(&cluster_key)
            .and_then(|c| c.pending_delete.as_ref())
            .is_some_and(|pd| pd.resource_name == test_configmap_name);
        assert!(
            is_pending,
            "Resource should be marked for deletion after first click"
        );

        // 13. Confirm deletion in the dialog.
        let confirm_delete_label = format!("Delete {}", test_configmap_name);
        harness
            .get_by_label(&confirm_delete_label)
            .click_accesskit();
        harness.run();

        // Wait for resource to be removed from cache (watcher will notify)
        wait_for(
            &mut harness,
            |app| {
                let cache = app.ui_state.clusters.get(&cluster_key).and_then(|c| {
                    c.resource_cache
                        .get(&(configmaps_resource.clone(), "default".to_string()))
                });
                if let Some(state) = cache {
                    if !state
                        .resources
                        .values()
                        .any(|r| r.name == test_configmap_name)
                    {
                        return Some(());
                    }
                }
                None
            },
            10000,
        )
        .expect("ConfigMap should be removed from cache after delete");

        // 14. Verify deletion via kube-rs
        let cm_exists = rt.block_on(async { configmaps.get(&test_configmap_name).await.is_ok() });

        assert!(!cm_exists, "ConfigMap should be deleted from the cluster");

        // Cleanup is done by the delete test itself, no need for manual cleanup
    }
}
