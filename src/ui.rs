use crate::SortedName::SortedName;
use crate::cluster_connection_manager::{Cluster, ClusterConnection};
use crate::helpers::SetExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::worker::{Worker, WorkerCommand, WorkerResult};
use egui::{Button, Direction, Layout, PopupCloseBehavior, Ui, Vec2, Widget};
use itertools::Itertools;
use std::collections::{BTreeMap, HashMap, HashSet};
use tracing::{error, info};
use crate::api_resource::ApiResource;

#[derive(Default)]
pub struct MyEguiApp {
    counter_value: i32,
    worker: Worker,
    ui_state: UiState,
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

impl ClusterState {
    pub fn display_select_button(&self, ui: &mut Ui) -> bool {
        Button::new(&self.name).corner_radius(5.0).ui(ui).clicked()
    }
}

impl UiState {
    fn update(&mut self, worker: &mut Worker) {
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
                WorkerResult::None => {}
            }
        }
    }
}

impl MyEguiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self::default()
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.worker.start();

        self.ui_state.update(&mut self.worker);

        egui::SidePanel::left("left_panel")
            .exact_width(64f32)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    for (cluster_key, cluster) in &mut self.ui_state.clusters {
                        if cluster.display_select_button(ui) {
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
                })
            });

        if let Some(selected_cluster_id) = self.ui_state.selected_cluster {
            if let Some(cluster) = self.ui_state.clusters.get_mut(&selected_cluster_id) {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.horizontal(|ui| {
                            ui.set_width(350.0);
                            let selected_namespaces_label =
                                cluster.selected_namespaces.iter().join(", ");
                            egui::ComboBox::from_label("Namespace")
                                .selected_text(selected_namespaces_label)
                                .truncate()
                                .width(350.0)
                                .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
                                .show_ui(ui, |ui| {
                                    for (_, ns) in &cluster.namespaces {
                                        let response = ui.selectable_label(
                                            cluster.selected_namespaces.contains(&ns.name),
                                            ns.get_name_to_display(),
                                        );
                                        if response.clicked() {
                                            if response.ctx.input(|i| i.modifiers.shift) {
                                                cluster.selected_namespaces.toggle(ns.name.clone());
                                            } else {
                                                cluster.selected_namespaces = HashSet::new();
                                                cluster.selected_namespaces.insert(ns.name.clone());
                                                ui.close();
                                            }
                                        }
                                    }
                                });
                        });
                    });



                    egui::SidePanel::left("api-selector").show(ui.ctx(), |ui| {

                        ui.vertical(|ui| {
                            for (api_group_name, api_resources) in &mut cluster.api_resource_groups {
                                ui.collapsing(api_group_name, |ui| {
                                    ui.vertical(|ui| {
                                        for api_resource in &api_resources.api_resources {
                                            if ui.selectable_label(false, &api_resource.name).clicked() {
                                                cluster.selected_api_resource = Some(api_resource.clone());
                                            }
                                        }
                                    });
                                });
                            }

                        })

                    });
                    ui.heading("Hello World!");

                    ui_counter(ui, &mut self.counter_value)
                });
            }
        }
    }
}

fn ui_counter(ui: &mut Ui, counter: &mut i32) {
    // Put the buttons and label on the same row:
    ui.horizontal(|ui| {
        if ui.button("−").clicked() {
            *counter -= 1;
        }
        ui.label(counter.to_string());
        if ui.button("+").clicked() {
            *counter += 1;
        }
    });
}
