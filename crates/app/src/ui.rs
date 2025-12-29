use crate::sorted_name::SortedName;
use crate::cluster_connection_manager::ClusterConnection;
use crate::helpers::SetExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::worker::{Worker, WorkerCommand, WorkerResult, WorkerTrait};
use components::{NarrowSidebar, TailwindCombobox, WideSidebar};
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

#[derive(Debug)]
pub struct ClusterState {
    pub name: String,
    pub cluster: Option<String>,
    pub cluster_key: i32,
    pub namespaces: BTreeMap<SortedName, MinimalNamespace>,
    pub connection: ClusterConnectionState,
    pub selected_namespaces: HashSet<String>,
    pub api_resource_groups: BTreeMap<String, ApiResourceGroupState>,
    pub selected_api_resource: Option<ApiResource>
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
        self.ui_state.selected_cluster = Some(cluster_key);
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
                                    cluster.selected_namespaces.toggle(ns.name.clone());
                                }
                            });
                    });

                    ui.heading("Hello World!");
                });

                // Apply clicked API resource selection
                if let Some(api_resource) = clicked_api_resource {
                    cluster.selected_api_resource = Some(api_resource);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster_connection_manager::Cluster;
    use crate::worker::MockWorker;
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
}
