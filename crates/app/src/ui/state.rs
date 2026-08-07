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
    pub(super) error: Option<String>,
}

#[derive(Debug, Default)]
pub(super) enum ClusterLoadState {
    #[default]
    Loading,
    Ready,
    Failed(String),
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
    pub(super) namespaces_load: ClusterLoadState,
    pub(super) api_resources_load: ClusterLoadState,
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
    Failed(String),
}

impl UiState {
    pub(super) fn select_cluster(
        &mut self,
        cluster_key: i32,
    ) -> Option<crate::worker::WorkerCommand> {
        self.selected_cluster = Some(cluster_key);

        let cluster = self.clusters.get_mut(&cluster_key)?;
        if matches!(
            &cluster.connection,
            ClusterConnectionState::Connected(_) | ClusterConnectionState::Connecting
        ) {
            return None;
        }

        cluster.connection = ClusterConnectionState::Connecting;
        cluster.namespaces_load = ClusterLoadState::Loading;
        cluster.api_resources_load = ClusterLoadState::Loading;
        cluster.namespaces.clear();
        cluster.resource_navigation = ResourceNavigation::default();
        cluster.selected_namespaces.clear();
        cluster.selected_api_resource = None;
        cluster.resource_cache.clear();
        cluster.active_watchers.clear();

        Some(crate::worker::WorkerCommand::ConnectToCluster {
            cluster: cluster.name.clone(),
            cluster_key,
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

        cluster.selected_api_resource = Some(api_resource);
        let api_resource = cluster
            .selected_api_resource
            .clone()
            .expect("selected API resource was just set");
        Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
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

        let Some(api_resource) = cluster.selected_api_resource.clone() else {
            return;
        };
        Self::request_resource_watch(cluster, &api_resource, namespace, commands_to_send);
    }

    pub(super) fn retry_selected_load(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let retry_connection = self.clusters.get(&cluster_key).is_some_and(|cluster| {
            matches!(&cluster.connection, ClusterConnectionState::Failed(_))
                || matches!(&cluster.namespaces_load, ClusterLoadState::Failed(_))
                || matches!(&cluster.api_resources_load, ClusterLoadState::Failed(_))
        });
        if retry_connection {
            if let Some(command) = self.select_cluster(cluster_key) {
                commands_to_send.push(command);
            }
            return;
        }

        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        let Some(api_resource) = cluster.selected_api_resource.clone() else {
            return;
        };
        Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
    }

    fn request_selected_resource_watches(
        cluster: &mut ClusterState,
        api_resource: &ApiResource,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let namespaces = cluster.selected_namespaces.clone();
        for namespace in namespaces {
            Self::request_resource_watch(cluster, api_resource, namespace, commands_to_send);
        }
    }

    fn request_resource_watch(
        cluster: &mut ClusterState,
        api_resource: &ApiResource,
        namespace: String,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let key = (api_resource.clone(), namespace.clone());
        let watch = cluster.resource_cache.entry(key.clone()).or_default();
        if watch.is_synced || cluster.active_watchers.contains(&key) {
            return;
        }
        watch.error = None;
        cluster.active_watchers.insert(key);
        commands_to_send.push(crate::worker::WorkerCommand::StartResourceWatch {
            cluster_key: cluster.cluster_key,
            api_resource: api_resource.clone(),
            namespace,
        });
    }

    pub(super) fn update<W: WorkerTrait>(
        &mut self,
        worker: &mut W,
    ) -> Vec<crate::worker::WorkerCommand> {
        let mut commands_to_send = Vec::new();
        while let Some(result) = worker.get_next_message() {
            match result {
                WorkerResult::CommandFailed {
                    error: message,
                    command,
                } => {
                    error!("Command '{command:?}' failed with error: {message}");
                    let message = format!("{message:#?}");
                    match command {
                        Some(crate::worker::WorkerCommand::ConnectToCluster {
                            cluster_key,
                            ..
                        }) => {
                            if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                                cluster.connection = ClusterConnectionState::Failed(message);
                            }
                        }
                        Some(crate::worker::WorkerCommand::StartResourceWatch {
                            cluster_key,
                            api_resource,
                            namespace,
                        }) => {
                            self.resource_watch_failed(
                                cluster_key,
                                api_resource,
                                namespace,
                                message,
                            );
                        }
                        _ => {}
                    }
                }
                WorkerResult::KubernetesClustersUpdated(clusters) => {
                    self.clusters.clear();
                    self.selected_cluster = None;
                    let mut current_cluster_key = None;
                    for cluster in clusters {
                        self.next_cluster_key += 1;
                        let cluster_key = self.next_cluster_key;
                        if cluster.is_current {
                            current_cluster_key = Some(cluster_key);
                        }
                        self.clusters.insert(
                            cluster_key,
                            ClusterState {
                                cluster_key,
                                name: cluster.name,
                                cluster: cluster.cluster,
                                namespaces: BTreeMap::new(),
                                connection: ClusterConnectionState::Disconnected,
                                namespaces_load: ClusterLoadState::Loading,
                                api_resources_load: ClusterLoadState::Loading,
                                selected_namespaces: HashSet::new(),
                                selected_api_resource: None,
                                resource_navigation: ResourceNavigation::default(),
                                resource_cache: HashMap::new(),
                                active_watchers: HashSet::new(),
                                yaml_panel: None,
                                pending_delete: None,
                            },
                        );
                    }
                    if let Some(cluster_key) = current_cluster_key {
                        if let Some(command) = self.select_cluster(cluster_key) {
                            commands_to_send.push(command);
                        }
                    }
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
                        cluster.namespaces_load = ClusterLoadState::Ready;
                    }
                }
                WorkerResult::KubernetesNamespacesLoadFailed { cluster_key, error } => {
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.namespaces_load = ClusterLoadState::Failed(error);
                    }
                }
                WorkerResult::KubernetesApisLoaded {
                    api_resources,
                    cluster_key,
                } => {
                    info!("Kubernetes API loaded");
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.resource_navigation = build_resource_navigation(api_resources);
                        cluster.api_resources_load = ClusterLoadState::Ready;
                    }
                }
                WorkerResult::KubernetesApisLoadFailed { cluster_key, error } => {
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.api_resources_load = ClusterLoadState::Failed(error);
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
                        watch.error = None;
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
                WorkerResult::KubernetesResourceWatchFailed {
                    cluster_key,
                    api_resource,
                    namespace,
                    error,
                } => {
                    self.resource_watch_failed(cluster_key, api_resource, namespace, error);
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
        commands_to_send
    }

    fn resource_watch_failed(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        error: String,
    ) {
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            let key = (api_resource, namespace);
            cluster.active_watchers.remove(&key);
            let watch = cluster.resource_cache.entry(key).or_default();
            watch.is_synced = false;
            watch.error = Some(error);
        }
    }
}
