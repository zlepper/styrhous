use crate::sorted_name::SortedName;
use crate::cluster_connection_manager::ClusterConnection;
use crate::helpers::SetExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::worker::{Worker, WorkerCommand, WorkerResult, WorkerTrait};
use components::{NarrowSidebar, TailwindCombobox, TailwindTable, TableRowBuilder, WideSidebar};
use components::icons::folder_icon;
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

                        // Build and show table
                        let mut table = TailwindTable::new(format!("resource-table-{}", api_resource.name))
                            .column("name", "Name", |col| col.sortable().initial_width(250.0));

                        if show_namespace_column {
                            table = table.column("namespace", "Namespace", |col| col.sortable().initial_width(120.0));
                        }

                        table = table
                            .column("status", "Status", |col| col.sortable().initial_width(100.0))
                            .column("ready", "Ready", |col| col.initial_width(80.0))
                            .column("age", "Age", |col| col.sortable().initial_width(80.0));

                        table.show(ui, &all_resources, |ui, resource, col_index| {
                            // Adjust column indices based on whether namespace column is shown
                            let (name_idx, ns_idx, status_idx, ready_idx, age_idx) = if show_namespace_column {
                                (0, Some(1), 2, 3, 4)
                            } else {
                                (0, None, 1, 2, 3)
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
                            }
                        });
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
}
