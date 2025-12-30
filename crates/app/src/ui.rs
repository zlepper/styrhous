use crate::sorted_name::SortedName;
use crate::cluster_connection_manager::ClusterConnection;
use crate::helpers::SetExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::worker::{Worker, WorkerCommand, WorkerResult, WorkerTrait};
use components::{NarrowSidebar, TailwindCombobox, TailwindTable, TableRowBuilder, WideSidebar};
use components::icons::folder_icon;
use components::colors::gray;
use itertools::Itertools;
use std::collections::{BTreeMap, HashMap, HashSet};
use tracing::{error, info};
use crate::api_resource::ApiResource;

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

#[derive(Default)]
pub struct UiState {
    clusters: HashMap<i32, ClusterState>,
    next_cluster_key: i32,
    selected_cluster: Option<i32>,
}

/// Key for identifying a resource watcher (api_resource + namespace)
pub type ResourceWatchKey = (ApiResource, String);

/// State for a single resource watch
#[derive(Debug, Default)]
pub struct ResourceWatchState {
    /// Resources indexed by UID
    pub resources: BTreeMap<String, MinimalResource>,
    /// Whether initial sync is complete
    pub is_synced: bool,
}

/// State for the YAML editing bottom panel
#[derive(Debug, Clone)]
pub struct YamlPanelState {
    pub api_resource: ApiResource,
    pub namespace: String,
    pub resource_name: String,
    pub original_yaml: String,
    pub edited_yaml: String,
    pub panel_height: f32,
}

impl YamlPanelState {
    pub fn is_modified(&self) -> bool {
        self.original_yaml != self.edited_yaml
    }
}

/// State for pending delete confirmation
#[derive(Debug, Clone)]
pub struct PendingDelete {
    pub resource_uid: String,
    pub resource_name: String,
    pub timestamp: std::time::Instant,
}

/// Action triggered by context menu or action buttons
#[derive(Debug, Clone)]
pub enum ResourceAction {
    EditYaml { name: String, namespace: String },
    Delete { name: String, namespace: String },
    MarkForDelete { uid: String, name: String },
}

#[derive(Debug)]
pub struct ClusterState {
    pub name: String,
    pub cluster: Option<String>,
    pub cluster_key: i32,
    pub namespaces: BTreeMap<SortedName, MinimalNamespace>,
    pub connection: ClusterConnectionState,
    pub selected_namespaces: HashSet<String>,
    pub api_resource_groups: BTreeMap<String, ApiResourceGroupState>,
    pub selected_api_resource: Option<ApiResource>,
    /// Resource cache - persists across API resource selection changes
    pub resource_cache: HashMap<ResourceWatchKey, ResourceWatchState>,
    /// Track active watchers
    pub active_watchers: HashSet<ResourceWatchKey>,
    /// YAML editing bottom panel state
    pub yaml_panel: Option<YamlPanelState>,
    /// Pending delete confirmation
    pub pending_delete: Option<PendingDelete>,
}

#[derive(Debug)]
pub struct ApiResourceGroupState {
    pub open: bool,
    pub api_resources: Vec<ApiResource>,
}

#[derive(Debug)]
pub enum ClusterConnectionState {
    Disconnected,
    Connecting,
    Connected(ClusterConnection),
}


impl UiState {
    fn update<W: WorkerTrait>(&mut self, worker: &mut W) {
        while let Some(result) = worker.get_next_message() {
            match result {
                WorkerResult::CommandFailed { error, command } => {
                    error!("Command '{:?}' failed with error: {}", command, error);
                }
                WorkerResult::KubernetesClustersUpdated(clusters) => {
                    self.clusters = clusters
                        .into_iter()
                        .map(|c| {
                            self.next_cluster_key += 1;
                            (
                                self.next_cluster_key,
                                ClusterState {
                                    cluster_key: self.next_cluster_key,
                                    name: c.name,
                                    cluster: c.cluster,
                                    namespaces: BTreeMap::new(),
                                    connection: ClusterConnectionState::Disconnected,
                                    selected_namespaces: HashSet::new(),
                                    selected_api_resource: None,
                                    api_resource_groups: BTreeMap::new(),
                                    resource_cache: HashMap::new(),
                                    active_watchers: HashSet::new(),
                                    yaml_panel: None,
                                    pending_delete: None,
                                },
                            )
                        })
                        .collect();
                }
                WorkerResult::KubernetesNamespacesAdded {
                    cluster_key,
                    namespace,
                } => {
                    info!("Added kubernetes namespace: {}", namespace);
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster
                            .namespaces
                            .insert(SortedName::new(&namespace.name), namespace);
                    }
                }
                WorkerResult::KubernetesNamespacesDeleted {
                    cluster_key,
                    namespace_name,
                } => {
                    info!("Deleting kubernetes namespace: {}", namespace_name);
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.namespaces.remove(&namespace_name.into());
                    }
                }
                WorkerResult::KubernetesNamespacesReplaced {
                    cluster_key,
                    namespaces,
                } => {
                    info!("Kubernetes namespaces replaced: {}", namespaces.len());
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.namespaces = namespaces
                            .into_iter()
                            .map(|ns| (SortedName::new(&ns.name), ns))
                            .collect();
                    }
                }
                WorkerResult::KubernetesApisLoaded { api_resources, cluster_key } => {
                    info!("Kubernetes API loaded");

                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.api_resource_groups = BTreeMap::new();
                        for resource in api_resources {
                            cluster.api_resource_groups.entry(resource.group.clone())
                                .and_modify(|e| e.api_resources.push(resource.clone()))
                                .or_insert_with(|| ApiResourceGroupState {
                                    open: false,
                                    api_resources: vec![resource.clone()],
                                });
                        }

                        for (_, resources) in &mut cluster.api_resource_groups {
                            resources.api_resources.sort_by(|a, b| a.name.cmp(&b.name))
                        }
                    }
                }
                WorkerResult::KubernetesClusterConnectionCreated {
                    cluster_key,
                    runner,
                } => {
                    info!("Cluster connection created");
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.connection = ClusterConnectionState::Connected(runner);
                    }
                }
                WorkerResult::KubernetesResourceAdded {
                    cluster_key,
                    api_resource,
                    namespace,
                    resource,
                } => {
                    info!("Resource added: {}", resource.name);
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        let key = (api_resource, namespace);
                        let state = cluster.resource_cache.entry(key).or_default();
                        state.resources.insert(resource.uid.clone(), resource);
                    }
                }
                WorkerResult::KubernetesResourceDeleted {
                    cluster_key,
                    api_resource,
                    namespace,
                    resource_uid,
                } => {
                    info!("Resource deleted: {}", resource_uid);
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        let key = (api_resource, namespace);
                        if let Some(state) = cluster.resource_cache.get_mut(&key) {
                            state.resources.remove(&resource_uid);
                        }
                    }
                }
                WorkerResult::KubernetesResourcesReplaced {
                    cluster_key,
                    api_resource,
                    namespace,
                    resources,
                } => {
                    info!("Resources replaced: {} resources", resources.len());
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        let key = (api_resource, namespace);
                        let state = cluster.resource_cache.entry(key).or_default();
                        state.resources = resources
                            .into_iter()
                            .map(|r| (r.uid.clone(), r))
                            .collect();
                        state.is_synced = true;
                    }
                }
                WorkerResult::KubernetesResourceWatchStarted {
                    cluster_key,
                    api_resource,
                    namespace,
                } => {
                    info!("Resource watch started for {}/{}", api_resource.group, api_resource.name);
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.active_watchers.insert((api_resource, namespace));
                    }
                }
                WorkerResult::ResourceYamlFetched {
                    cluster_key,
                    api_resource,
                    namespace,
                    resource_name,
                    yaml,
                } => {
                    info!("YAML fetched for {}", resource_name);
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.yaml_panel = Some(YamlPanelState {
                            api_resource,
                            namespace,
                            resource_name,
                            original_yaml: yaml.clone(),
                            edited_yaml: yaml,
                            panel_height: 300.0,
                        });
                    }
                }
                WorkerResult::ResourceDeleteCompleted {
                    cluster_key,
                    resource_name,
                    ..
                } => {
                    info!("Resource deleted: {}", resource_name);
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.pending_delete = None;
                    }
                    // Note: The watcher will send KubernetesResourceDeleted to update the cache
                }
                WorkerResult::ResourceApplyCompleted {
                    cluster_key,
                    resource_name,
                    ..
                } => {
                    info!("Resource applied: {}", resource_name);
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        // Close the YAML panel after successful apply
                        cluster.yaml_panel = None;
                    }
                    // Note: The watcher will send updates to refresh the cache
                }
            }
        }
    }
}

impl<W: WorkerTrait> MyEguiApp<W> {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Install image loaders for SVG icons
        egui_extras::install_image_loaders(&cc.egui_ctx);
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

        // Cluster selection sidebar (narrow, icon-only style with avatar initials)
        egui::SidePanel::left("cluster-panel")
            .exact_width(72.0)
            .frame(egui::Frame::NONE.fill(egui::Color32::WHITE))
            .show(ctx, |ui| {
                NarrowSidebar::new().show(ui, |sidebar| {
                    for (cluster_key, cluster) in &self.ui_state.clusters {
                        let initial = cluster.name.chars().next()
                            .unwrap_or('?').to_uppercase().to_string();
                        let selected = self.ui_state.selected_cluster == Some(*cluster_key);

                        if sidebar.avatar_item(&cluster.name, &initial, selected).clicked() {
                            info!("Cluster '{}' selected", cluster.name);
                            if let ClusterConnectionState::Disconnected = cluster.connection {
                                info!("Connecting to cluster");
                                self.worker.send_command(WorkerCommand::ConnectToCluster {
                                    cluster: cluster.name.clone(),
                                    cluster_key: *cluster_key,
                                });
                            }
                            self.ui_state.selected_cluster = Some(*cluster_key);
                        }
                    }
                });
            });

        // Track clicked API resource to apply mutation after UI rendering
        let mut clicked_api_resource: Option<ApiResource> = None;

        // API resource tree sidebar (shown before CentralPanel, only when cluster is selected)
        if let Some(selected_cluster_id) = self.ui_state.selected_cluster {
            if let Some(cluster) = self.ui_state.clusters.get(&selected_cluster_id) {
                egui::SidePanel::left("api-selector")
                    .default_width(256.0)
                    .min_width(200.0)
                    .frame(egui::Frame::NONE.fill(egui::Color32::WHITE))
                    .show(ctx, |ui| {
                        WideSidebar::new().show(ui, |sidebar| {
                            for (api_group_name, api_resources) in &cluster.api_resource_groups {
                                let display_name = if api_group_name.is_empty() {
                                    "core"
                                } else {
                                    api_group_name.as_str()
                                };

                                sidebar.expandable(display_name, folder_icon(), false, |sidebar| {
                                    for api_resource in &api_resources.api_resources {
                                        let selected = cluster.selected_api_resource
                                            .as_ref()
                                            .is_some_and(|r| r == api_resource);

                                        if sidebar.child_item(&api_resource.name, selected).clicked() {
                                            clicked_api_resource = Some(api_resource.clone());
                                        }
                                    }
                                });
                            }
                        });
                    });
            }
        }

        // Track commands to send (deferred to avoid borrow issues)
        let mut commands_to_send: Vec<WorkerCommand> = Vec::new();

        // Track if we should close the YAML panel (deferred action)
        let mut close_yaml_panel = false;

        // Bottom panel for YAML editing (must be rendered before CentralPanel)
        if let Some(selected_cluster_id) = self.ui_state.selected_cluster {
            if let Some(cluster) = self.ui_state.clusters.get_mut(&selected_cluster_id) {
                if let Some(ref mut yaml_panel) = cluster.yaml_panel {
                    egui::TopBottomPanel::bottom("yaml-panel")
                        .resizable(true)
                        .min_height(100.0)
                        .default_height(yaml_panel.panel_height)
                        .frame(egui::Frame::new()
                            .fill(egui::Color32::WHITE)
                            .stroke(egui::Stroke::new(1.0, gray::_200))
                            .inner_margin(8.0))
                        .show(ctx, |ui| {
                            // Header with resource name and buttons
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("Edit: {}", yaml_panel.resource_name))
                                        .strong()
                                        .size(14.0)
                                );

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // Close button with accessibility label
                                    let close_button = ui.button("Close YAML Editor");
                                    if close_button.clicked() {
                                        if yaml_panel.is_modified() {
                                            // TODO: Add confirmation dialog
                                            info!("Discarding unsaved YAML changes");
                                        }
                                        close_yaml_panel = true;
                                    }

                                    // Save button (disabled if no changes) with accessibility
                                    let save_button = ui.add_enabled(
                                        yaml_panel.is_modified(),
                                        egui::Button::new("Save YAML"),
                                    );
                                    if save_button.clicked() {
                                        commands_to_send.push(WorkerCommand::ApplyResourceYaml {
                                            cluster_key: cluster.cluster_key,
                                            api_resource: yaml_panel.api_resource.clone(),
                                            namespace: yaml_panel.namespace.clone(),
                                            resource_name: yaml_panel.resource_name.clone(),
                                            yaml: yaml_panel.edited_yaml.clone(),
                                        });
                                    }

                                    // Show modified indicator
                                    if yaml_panel.is_modified() {
                                        ui.label(
                                            egui::RichText::new("Modified")
                                                .color(egui::Color32::from_rgb(234, 179, 8)) // yellow-500
                                                .size(12.0)
                                        );
                                    }
                                });
                            });

                            ui.separator();

                            // YAML editor with accessibility
                            egui::ScrollArea::both()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let editor = egui::TextEdit::multiline(&mut yaml_panel.edited_yaml)
                                        .font(egui::TextStyle::Monospace)
                                        .code_editor()
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(20)
                                        .hint_text("YAML Editor");
                                    ui.add(editor);
                                });
                        });
                }
            }
        }

        // Apply deferred YAML panel close
        if close_yaml_panel {
            if let Some(selected_cluster_id) = self.ui_state.selected_cluster {
                if let Some(cluster) = self.ui_state.clusters.get_mut(&selected_cluster_id) {
                    cluster.yaml_panel = None;
                }
            }
        }

        // Central panel with main content
        if let Some(selected_cluster_id) = self.ui_state.selected_cluster {
            if let Some(cluster) = self.ui_state.clusters.get_mut(&selected_cluster_id) {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(egui::Color32::WHITE))
                    .show(ctx, |ui| {
                    // Namespace selector with fuzzy filtering
                    ui.horizontal(|ui| {
                        let selected_text = cluster.selected_namespaces.iter().join(", ");
                        let namespaces: Vec<_> = cluster.namespaces.values().collect();

                        TailwindCombobox::from_label("Namespace")
                            .placeholder("Search namespaces...")
                            .selected_text(selected_text)
                            .width(350.0)
                            .filter_by(|ns: &&MinimalNamespace| ns.get_name_to_display())
                            .show_items(ui, &namespaces, |cb, ns| {
                                let is_selected = cluster.selected_namespaces.contains(&ns.name);
                                if cb.item(ns.get_name_to_display(), is_selected).clicked() {
                                    let was_selected = cluster.selected_namespaces.contains(&ns.name);
                                    cluster.selected_namespaces.toggle(ns.name.clone());

                                    // Start watcher for newly selected namespace if API resource is selected
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
                    });

                    ui.add_space(16.0);

                    // Resource table
                    if let Some(api_resource) = &cluster.selected_api_resource {
                        // Collect resources from all selected namespaces
                        let show_namespace_column = cluster.selected_namespaces.len() > 1;
                        let mut all_resources: Vec<&MinimalResource> = Vec::new();

                        for ns in &cluster.selected_namespaces {
                            let key = (api_resource.clone(), ns.clone());
                            if let Some(state) = cluster.resource_cache.get(&key) {
                                all_resources.extend(state.resources.values());
                            }
                        }

                        // Sort by name
                        all_resources.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                        // Track pending action for deferred handling
                        let mut pending_action: Option<ResourceAction> = None;

                        // Get pending_delete state for coloring delete buttons
                        let pending_delete_uid = cluster.pending_delete.as_ref().map(|pd| pd.resource_uid.clone());

                        // Build and show table
                        let mut table = TailwindTable::new(format!("resource-table-{}", api_resource.name))
                            .column("name", "Name", |col| col.sortable().initial_width(250.0));

                        if show_namespace_column {
                            table = table.column("namespace", "Namespace", |col| col.sortable().initial_width(120.0));
                        }

                        table = table
                            .column("status", "Status", |col| col.sortable().initial_width(100.0))
                            .column("ready", "Ready", |col| col.initial_width(80.0))
                            .column("age", "Age", |col| col.sortable().initial_width(80.0))
                            .column("actions", "Actions", |col| col.initial_width(300.0));

                        table.show(ui, &all_resources, |ui, resource, col_index| {
                            // Adjust column indices based on whether namespace column is shown
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
                                TableRowBuilder::text(ui, resource.display_status(), false);
                            } else if col_index == ready_idx {
                                TableRowBuilder::text(ui, resource.display_ready(), false);
                            } else if col_index == age_idx {
                                TableRowBuilder::text(ui, &resource.age(), false);
                            } else if col_index == actions_idx {
                                ui.horizontal(|ui| {
                                    ui.add_space(4.0);

                                    // Edit YAML button - native egui button for proper accessibility
                                    let edit_label = format!("Edit {}", resource.name);
                                    if ui.button(&edit_label).clicked() && pending_action.is_none() {
                                        pending_action = Some(ResourceAction::EditYaml {
                                            name: resource.name.clone(),
                                            namespace: resource.namespace.clone().unwrap_or_default(),
                                        });
                                    }

                                    ui.add_space(4.0);

                                    // Delete button with confirmation - native egui button
                                    let is_pending_delete = pending_delete_uid.as_ref()
                                        .is_some_and(|uid| uid == &resource.uid);

                                    let delete_label = if is_pending_delete {
                                        format!("Confirm delete {}", resource.name)
                                    } else {
                                        format!("Delete {}", resource.name)
                                    };

                                    // Style the button red when pending delete
                                    let button = if is_pending_delete {
                                        egui::Button::new(
                                            egui::RichText::new(&delete_label)
                                                .color(egui::Color32::from_rgb(220, 38, 38))
                                        )
                                    } else {
                                        egui::Button::new(&delete_label)
                                    };

                                    if ui.add(button).clicked() && pending_action.is_none() {
                                        if is_pending_delete {
                                            // Second click - actually delete
                                            pending_action = Some(ResourceAction::Delete {
                                                name: resource.name.clone(),
                                                namespace: resource.namespace.clone().unwrap_or_default(),
                                            });
                                        } else {
                                            // First click - mark for deletion
                                            pending_action = Some(ResourceAction::MarkForDelete {
                                                uid: resource.uid.clone(),
                                                name: resource.name.clone(),
                                            });
                                        }
                                    }
                                });
                            }
                        });

                        // Handle pending action after table rendering
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
                                ResourceAction::Delete { name, namespace } => {
                                    commands_to_send.push(WorkerCommand::DeleteResource {
                                        cluster_key: cluster.cluster_key,
                                        api_resource: api_resource.clone(),
                                        namespace,
                                        resource_name: name,
                                    });
                                    cluster.pending_delete = None;
                                }
                                ResourceAction::MarkForDelete { uid, name } => {
                                    cluster.pending_delete = Some(PendingDelete {
                                        resource_uid: uid,
                                        resource_name: name,
                                        timestamp: std::time::Instant::now(),
                                    });
                                    // Request repaint after 3 seconds to clear confirmation
                                    ui.ctx().request_repaint_after(std::time::Duration::from_secs(3));
                                }
                            }
                        }

                        // Clear pending delete after timeout
                        if let Some(pending) = &cluster.pending_delete {
                            if pending.timestamp.elapsed() > std::time::Duration::from_secs(3) {
                                cluster.pending_delete = None;
                            }
                        }
                    } else {
                        ui.label("Select an API resource from the sidebar to view resources.");
                    }
                });

                // Apply clicked API resource selection and start watchers
                if let Some(api_resource) = clicked_api_resource {
                    // Start watchers for all selected namespaces
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
        }

        // Send deferred commands
        for command in commands_to_send {
            self.worker.send_command(command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster_connection_manager::Cluster;
    use crate::worker::MockWorker;
    use egui_kittest::kittest::Queryable;
    use egui_kittest::Harness;

    #[test]
    fn test_ui_flow() {
        let mut harness = Harness::new_eframe(|_cc| MyEguiApp::<MockWorker>::default());

        // Install image loaders for SVG icons in tests
        egui_extras::install_image_loaders(&harness.ctx);

        // Initial empty state
        harness.run();
        harness.snapshot("01_empty_state");

        // Clusters arrive from worker
        harness.state_mut().worker.results.push_back(
            WorkerResult::KubernetesClustersUpdated(vec![
                Cluster {
                    name: "dev".into(),
                    cluster: None,
                },
                Cluster {
                    name: "prod".into(),
                    cluster: Some("production".into()),
                },
            ]),
        );
        harness.run();
        harness.snapshot("02_clusters_loaded");

        // Select the dev cluster (key 1)
        harness.state_mut().select_cluster(1);
        harness.run();
        harness.snapshot("03_cluster_selected_empty");

        // Add namespaces
        harness.state_mut().worker.results.push_back(
            WorkerResult::KubernetesNamespacesReplaced {
                cluster_key: 1,
                namespaces: vec![
                    MinimalNamespace { name: "default".into(), display_name: None },
                    MinimalNamespace { name: "kube-system".into(), display_name: None },
                    MinimalNamespace { name: "monitoring".into(), display_name: Some("Monitoring Stack".into()) },
                ],
            },
        );
        harness.run();
        harness.snapshot("04_namespaces_loaded");

        // Add API resources
        harness.state_mut().worker.results.push_back(
            WorkerResult::KubernetesApisLoaded {
                cluster_key: 1,
                api_resources: vec![
                    // Core resources (empty group displayed as "core")
                    ApiResource { group: "".into(), version: "v1".into(), kind: "Pod".into(), name: "pods".into() },
                    ApiResource { group: "".into(), version: "v1".into(), kind: "Service".into(), name: "services".into() },
                    ApiResource { group: "".into(), version: "v1".into(), kind: "ConfigMap".into(), name: "configmaps".into() },
                    // apps group
                    ApiResource { group: "apps".into(), version: "v1".into(), kind: "Deployment".into(), name: "deployments".into() },
                    ApiResource { group: "apps".into(), version: "v1".into(), kind: "StatefulSet".into(), name: "statefulsets".into() },
                    // networking.k8s.io group
                    ApiResource { group: "networking.k8s.io".into(), version: "v1".into(), kind: "Ingress".into(), name: "ingresses".into() },
                ],
            },
        );
        harness.run();
        harness.snapshot("05_api_resources_loaded");
    }

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn test_real_cluster_connection() {
        let mut harness = Harness::new_eframe(|_cc| MyEguiApp::<Worker>::default());

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
    /// Requires a Kind cluster to be running locally.
    /// Run with: cargo test -p kubernetes-dev-ui test_resource_watcher_integration -- --ignored
    #[test]
    #[ignore]
    fn test_resource_watcher_integration() {
        let mut harness = Harness::new_eframe(|_cc| MyEguiApp::<Worker>::default());
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
        wait_for(&mut harness, |app| {
            if app.ui_state.clusters.is_empty() { None } else { Some(()) }
        }, 5000).expect("Clusters should load");

        // 2. Click on the Kind cluster via accessibility
        harness.get_by_label("kind-kind").click();
        harness.run();

        // Get cluster_key for later use
        let cluster_key = harness.state().ui_state.selected_cluster
            .expect("Cluster should be selected after click");

        // 3. Wait for namespaces to load
        wait_for(&mut harness, |app| {
            let cluster = app.ui_state.clusters.get(&cluster_key);
            if let Some(c) = cluster {
                if !c.namespaces.is_empty() {
                    return Some(());
                }
            }
            None
        }, 10000).expect("Namespaces should load");

        // 4. Wait for API resources to load
        wait_for(&mut harness, |app| {
            app.ui_state.clusters.get(&cluster_key)
                .filter(|c| !c.api_resource_groups.is_empty())
                .map(|_| ())
        }, 5000).expect("API resources should load");

        // 5. Click on namespace combobox to open it
        harness.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace").click();
        harness.run();

        // 6. Click on kube-system namespace
        harness.get_by_label("kube-system").click();
        harness.run();

        // Verify namespace was selected
        let namespaces_selected = harness.state().ui_state.clusters.get(&cluster_key)
            .map(|c| c.selected_namespaces.contains("kube-system"))
            .unwrap_or(false);
        assert!(namespaces_selected, "kube-system namespace should be selected after click. Selected: {:?}",
            harness.state().ui_state.clusters.get(&cluster_key).map(|c| &c.selected_namespaces));

        // 7. Click on "core" group to expand it (it should default to closed)
        harness.get_by_label("core").click();
        harness.run();
        harness.run(); // Extra run to ensure expandable section is fully rendered

        // 8. Click "pods" using accesskit action (handles off-screen elements)
        harness.get_by_label("pods").click_accesskit();
        harness.run();

        // Use the actual selected API resource (group/version may differ from hardcoded values)
        let pods_resource = harness.state().ui_state.clusters.get(&cluster_key)
            .and_then(|c| c.selected_api_resource.clone())
            .expect("pods API resource should be selected");

        // 9. Wait for resources to sync
        wait_for(&mut harness, |app| {
            app.ui_state.selected_cluster.and_then(|k| {
                app.ui_state.clusters.get(&k)
                    .and_then(|c| c.resource_cache.get(&(pods_resource.clone(), "kube-system".to_string())))
                    .filter(|s| s.is_synced)
                    .map(|_| ())
            })
        }, 10000).expect("Resources should sync");

        // 10. Verify we have pods
        let resource_count = harness.state().ui_state.selected_cluster
            .and_then(|k| harness.state().ui_state.clusters.get(&k))
            .and_then(|c| c.resource_cache.get(&(pods_resource.clone(), "kube-system".to_string())))
            .map(|s| s.resources.len())
            .unwrap_or(0);

        assert!(resource_count > 0, "Should have at least one pod, got {}", resource_count);

        // 11. Check for known pods (coredns is always in kube-system on Kind)
        let has_coredns = harness.state().ui_state.selected_cluster
            .and_then(|k| harness.state().ui_state.clusters.get(&k))
            .and_then(|c| c.resource_cache.get(&(pods_resource.clone(), "kube-system".to_string())))
            .map(|s| s.resources.values().any(|r| r.name.starts_with("coredns")))
            .unwrap_or(false);

        assert!(has_coredns, "Should have coredns pod");

        // 12. Take a snapshot for visual verification
        harness.snapshot("integration_resource_table");
    }

    /// Integration test for resource actions (Edit YAML, Delete) against a real Kind cluster.
    /// Creates a test ConfigMap, edits it, then deletes it.
    /// Run with: cargo test -p kubernetes-dev-ui test_resource_actions_integration -- --ignored
    #[test]
    #[ignore]
    fn test_resource_actions_integration() {
        use kube::{Api, Client};
        use k8s_openapi::api::core::v1::ConfigMap;
        use std::collections::BTreeMap;

        // Create a tokio runtime for direct kube-rs operations
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        // Use a deterministic name so snapshots don't change on every run
        let test_configmap_name = "test-cm-integration".to_string();

        let client = rt.block_on(async {
            Client::try_default().await.expect("Failed to create kube client")
        });

        let configmaps: Api<ConfigMap> = Api::namespaced(client.clone(), "default");

        // Cleanup: Delete the test ConfigMap if it exists from a previous run
        rt.block_on(async {
            let _ = configmaps.delete(&test_configmap_name, &Default::default()).await;
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
            configmaps.create(&Default::default(), &test_cm).await
                .expect("Failed to create test ConfigMap");
        });

        // Start the UI test
        // Note: The test deletes the ConfigMap as part of testing delete functionality
        let mut harness = Harness::new_eframe(|_cc| MyEguiApp::<Worker>::default());
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
        wait_for(&mut harness, |app| {
            if app.ui_state.clusters.is_empty() { None } else { Some(()) }
        }, 5000).expect("Clusters should load");

        // 2. Click on the Kind cluster
        harness.get_by_label("kind-kind").click();
        harness.run();

        let cluster_key = harness.state().ui_state.selected_cluster
            .expect("Cluster should be selected");

        // 3. Wait for namespaces and API resources to load
        wait_for(&mut harness, |app| {
            app.ui_state.clusters.get(&cluster_key)
                .filter(|c| !c.namespaces.is_empty() && !c.api_resource_groups.is_empty())
                .map(|_| ())
        }, 10000).expect("Namespaces and API resources should load");

        // 4. Select "default" namespace
        harness.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace").click();
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
        let configmaps_resource = harness.state().ui_state.clusters.get(&cluster_key)
            .and_then(|c| c.selected_api_resource.clone())
            .expect("configmaps API resource should be selected");

        // 6. Wait for ConfigMaps to sync
        wait_for(&mut harness, |app| {
            app.ui_state.clusters.get(&cluster_key)
                .and_then(|c| c.resource_cache.get(&(configmaps_resource.clone(), "default".to_string())))
                .filter(|s| s.is_synced)
                .map(|_| ())
        }, 10000).expect("ConfigMaps should sync");

        // 7. Verify our test ConfigMap appears
        let has_test_cm = harness.state().ui_state.clusters.get(&cluster_key)
            .and_then(|c| c.resource_cache.get(&(configmaps_resource.clone(), "default".to_string())))
            .map(|s| s.resources.values().any(|r| r.name == test_configmap_name))
            .unwrap_or(false);

        assert!(has_test_cm, "Test ConfigMap '{}' should appear in the resource list", test_configmap_name);

        // Run extra frames to ensure table is fully rendered with accessibility info
        for _ in 0..3 {
            harness.run();
        }

        // 8. Click the Edit button for our ConfigMap (real UI click via accessibility)
        let edit_button_label = format!("Edit {}", test_configmap_name);
        // Use click_accesskit() for potentially off-screen elements
        harness.get_by_label(&edit_button_label).click_accesskit();
        harness.run();

        // Wait for YAML panel to open
        wait_for(&mut harness, |app| {
            app.ui_state.clusters.get(&cluster_key)
                .and_then(|c| c.yaml_panel.as_ref())
                .filter(|p| p.resource_name == test_configmap_name)
                .map(|_| ())
        }, 5000).expect("YAML panel should open after clicking Edit button");

        // 9. Modify the YAML content
        // Note: We modify the state directly since text selection/replacement via kittest
        // is complex. The UI button clicks are the critical integration points.
        {
            let cluster = harness.state_mut().ui_state.clusters.get_mut(&cluster_key).unwrap();
            if let Some(ref mut panel) = cluster.yaml_panel {
                panel.edited_yaml = panel.edited_yaml.replace("original-value", "edited-value");
            }
        }
        harness.run();

        // 10. Click Save button (real UI click)
        harness.get_by_label("Save YAML").click();
        harness.run();

        // Wait for apply to complete (panel closes)
        wait_for(&mut harness, |app| {
            app.ui_state.clusters.get(&cluster_key)
                .filter(|c| c.yaml_panel.is_none())
                .map(|_| ())
        }, 5000).expect("YAML panel should close after clicking Save");

        // 11. Verify the change persisted via kube-rs
        let cm_after_edit = rt.block_on(async {
            configmaps.get(&test_configmap_name).await
                .expect("Failed to get ConfigMap after edit")
        });

        let edited_value = cm_after_edit.data
            .as_ref()
            .and_then(|d| d.get("key1"))
            .map(|s| s.as_str());

        assert_eq!(edited_value, Some("edited-value"),
            "ConfigMap should have edited value, got: {:?}", edited_value);

        // Run extra frames to ensure table is re-rendered after save
        for _ in 0..5 {
            harness.run();
        }

        // 12. Click Delete button - first click marks for deletion
        // Use click_accesskit() since the button may be off-screen in the table
        let delete_button_label = format!("Delete {}", test_configmap_name);
        harness.get_by_label(&delete_button_label).click_accesskit();
        harness.run();

        // Verify it's now pending delete (UI shows confirm state)
        let is_pending = harness.state().ui_state.clusters.get(&cluster_key)
            .and_then(|c| c.pending_delete.as_ref())
            .is_some_and(|pd| pd.resource_name == test_configmap_name);
        assert!(is_pending, "Resource should be marked for deletion after first click");

        // 13. Click Delete button again - second click confirms deletion
        // The label changes to "Confirm delete {name}" when pending
        let confirm_delete_label = format!("Confirm delete {}", test_configmap_name);
        harness.get_by_label(&confirm_delete_label).click_accesskit();
        harness.run();

        // Wait for resource to be removed from cache (watcher will notify)
        wait_for(&mut harness, |app| {
            let cache = app.ui_state.clusters.get(&cluster_key)
                .and_then(|c| c.resource_cache.get(&(configmaps_resource.clone(), "default".to_string())));
            if let Some(state) = cache {
                if !state.resources.values().any(|r| r.name == test_configmap_name) {
                    return Some(());
                }
            }
            None
        }, 10000).expect("ConfigMap should be removed from cache after delete");

        // 14. Verify deletion via kube-rs
        let cm_exists = rt.block_on(async {
            configmaps.get(&test_configmap_name).await.is_ok()
        });

        assert!(!cm_exists, "ConfigMap should be deleted from the cluster");

        // Cleanup is done by the delete test itself, no need for manual cleanup
    }
}
