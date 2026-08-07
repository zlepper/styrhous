use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::ClusterConnection;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::resource_catalog::{ResourceNavigation, build_resource_navigation};
use crate::sorted_name::SortedName;
use crate::worker::{WorkerResult, WorkerTrait};
use std::collections::{BTreeMap, HashMap, HashSet};
use tracing::{error, info};

#[derive(Default)]
pub(super) struct UiState {
    pub(super) clusters: HashMap<i32, ClusterState>,
    pub(super) next_cluster_key: i32,
    pub(super) selected_cluster: Option<i32>,
}

/// Key for identifying a resource watcher (API resource + namespace).
pub(super) type ResourceWatchKey = (ApiResource, String);

#[derive(Debug, Default)]
pub(super) struct ResourceWatchState {
    pub(super) resources: BTreeMap<String, MinimalResource>,
    pub(super) is_synced: bool,
}

#[derive(Debug, Clone)]
pub(super) struct YamlPanelState {
    pub(super) api_resource: ApiResource,
    pub(super) namespace: String,
    pub(super) resource_name: String,
    pub(super) original_yaml: String,
    pub(super) edited_yaml: String,
    pub(super) panel_height: f32,
}

impl YamlPanelState {
    pub(super) fn is_modified(&self) -> bool {
        self.original_yaml != self.edited_yaml
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingDelete {
    pub(super) resource_name: String,
    pub(super) namespace: String,
}

#[derive(Debug, Clone)]
pub(super) enum ResourceAction {
    EditYaml { name: String, namespace: String },
    RequestDelete { name: String, namespace: String },
}

#[derive(Debug)]
pub(super) struct ClusterState {
    pub(super) name: String,
    pub(super) cluster: Option<String>,
    pub(super) cluster_key: i32,
    pub(super) namespaces: BTreeMap<SortedName, MinimalNamespace>,
    pub(super) connection: ClusterConnectionState,
    pub(super) selected_namespaces: HashSet<String>,
    pub(super) resource_navigation: ResourceNavigation,
    pub(super) selected_api_resource: Option<ApiResource>,
    pub(super) resource_cache: HashMap<ResourceWatchKey, ResourceWatchState>,
    pub(super) active_watchers: HashSet<ResourceWatchKey>,
    pub(super) yaml_panel: Option<YamlPanelState>,
    pub(super) pending_delete: Option<PendingDelete>,
}

#[derive(Debug)]
pub(super) enum ClusterConnectionState {
    Disconnected,
    Connecting,
    /// A live connection is present in production. Snapshot fixtures can represent
    /// the same visible state without constructing a Kubernetes client.
    Connected(Option<ClusterConnection>),
}

impl UiState {
    pub(super) fn select_cluster(
        &mut self,
        cluster_key: i32,
    ) -> Option<crate::worker::WorkerCommand> {
        self.selected_cluster = Some(cluster_key);

        let cluster = self.clusters.get(&cluster_key)?;
        matches!(cluster.connection, ClusterConnectionState::Disconnected).then(|| {
            crate::worker::WorkerCommand::ConnectToCluster {
                cluster: cluster.name.clone(),
                cluster_key,
            }
        })
    }

    pub(super) fn select_api_resource(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };

        for namespace in &cluster.selected_namespaces {
            let key = (api_resource.clone(), namespace.clone());
            if !cluster.active_watchers.contains(&key) {
                commands_to_send.push(crate::worker::WorkerCommand::StartResourceWatch {
                    cluster_key: cluster.cluster_key,
                    api_resource: api_resource.clone(),
                    namespace: namespace.clone(),
                });
            }
        }
        cluster.selected_api_resource = Some(api_resource);
    }

    pub(super) fn toggle_namespace(
        &mut self,
        cluster_key: i32,
        namespace: String,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };

        let was_selected = !cluster.selected_namespaces.insert(namespace.clone());
        if was_selected {
            cluster.selected_namespaces.remove(&namespace);
            return;
        }

        let Some(api_resource) = &cluster.selected_api_resource else {
            return;
        };
        let key = (api_resource.clone(), namespace.clone());
        if !cluster.active_watchers.contains(&key) {
            commands_to_send.push(crate::worker::WorkerCommand::StartResourceWatch {
                cluster_key: cluster.cluster_key,
                api_resource: api_resource.clone(),
                namespace,
            });
        }
    }

    pub(super) fn update<W: WorkerTrait>(&mut self, worker: &mut W) {
        while let Some(result) = worker.get_next_message() {
            match result {
                WorkerResult::CommandFailed {
                    error: message,
                    command,
                } => {
                    error!("Command '{command:?}' failed with error: {message}");
                }
                WorkerResult::KubernetesClustersUpdated(clusters) => {
                    self.clusters = clusters
                        .into_iter()
                        .map(|cluster| {
                            self.next_cluster_key += 1;
                            (
                                self.next_cluster_key,
                                ClusterState {
                                    cluster_key: self.next_cluster_key,
                                    name: cluster.name,
                                    cluster: cluster.cluster,
                                    namespaces: BTreeMap::new(),
                                    connection: ClusterConnectionState::Disconnected,
                                    selected_namespaces: HashSet::new(),
                                    selected_api_resource: None,
                                    resource_navigation: ResourceNavigation::default(),
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
                    info!("Added kubernetes namespace: {namespace}");
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
                    info!("Deleting kubernetes namespace: {namespace_name}");
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
                            .map(|namespace| (SortedName::new(&namespace.name), namespace))
                            .collect();
                    }
                }
                WorkerResult::KubernetesApisLoaded {
                    api_resources,
                    cluster_key,
                } => {
                    info!("Kubernetes API loaded");
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.resource_navigation = build_resource_navigation(api_resources);
                    }
                }
                WorkerResult::KubernetesClusterConnectionCreated {
                    cluster_key,
                    runner,
                } => {
                    info!("Cluster connection created");
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.connection = ClusterConnectionState::Connected(Some(runner));
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
                        let watch = cluster
                            .resource_cache
                            .entry((api_resource, namespace))
                            .or_default();
                        watch.resources.insert(resource.uid.clone(), resource);
                    }
                }
                WorkerResult::KubernetesResourceDeleted {
                    cluster_key,
                    api_resource,
                    namespace,
                    resource_uid,
                } => {
                    info!("Resource deleted: {resource_uid}");
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        if let Some(watch) =
                            cluster.resource_cache.get_mut(&(api_resource, namespace))
                        {
                            watch.resources.remove(&resource_uid);
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
                        let watch = cluster
                            .resource_cache
                            .entry((api_resource, namespace))
                            .or_default();
                        watch.resources = resources
                            .into_iter()
                            .map(|resource| (resource.uid.clone(), resource))
                            .collect();
                        watch.is_synced = true;
                    }
                }
                WorkerResult::KubernetesResourceWatchStarted {
                    cluster_key,
                    api_resource,
                    namespace,
                } => {
                    info!(
                        "Resource watch started for {}/{}",
                        api_resource.group, api_resource.name
                    );
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
                    info!("YAML fetched for {resource_name}");
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
                    info!("Resource deleted: {resource_name}");
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.pending_delete = None;
                    }
                }
                WorkerResult::ResourceApplyCompleted {
                    cluster_key,
                    resource_name,
                    ..
                } => {
                    info!("Resource applied: {resource_name}");
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.yaml_panel = None;
                    }
                }
            }
        }
    }
}
