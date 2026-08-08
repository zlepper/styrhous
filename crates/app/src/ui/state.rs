use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::ClusterConnection;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::resource_catalog::{ResourceNavigation, build_resource_navigation};
use crate::resource_detail::{
    ManagedResource, ResourceDetail, ResourceDetailPayload, ResourceEvent,
};
use crate::resource_table::CustomResourceColumn;
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

/// Key for identifying a resource watcher (API resource + optional namespace).
pub(super) type ResourceWatchKey = (ApiResource, Option<String>);

#[derive(Debug, Default)]
pub(super) struct ResourceWatchState {
    pub(super) resources: BTreeMap<String, MinimalResource>,
    pub(super) is_synced: bool,
    pub(super) error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ResourceSearchState {
    pub(super) query: String,
    pub(super) regex_mode: bool,
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
    pub(super) namespace: Option<String>,
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
    pub(super) namespace: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ResourceDetailPanelState {
    pub(super) api_resource: ApiResource,
    pub(super) namespace: Option<String>,
    pub(super) resource_name: String,
    pub(super) resource_uid: String,
    /// Stable identity of this visit in the inspector history.
    pub(super) history_entry_id: u64,
    pub(super) selection_generation: u64,
    pub(super) detail: Option<ResourceDetail>,
    pub(super) events: Vec<ResourceEvent>,
    pub(super) detail_error: Option<String>,
    pub(super) events_error: Option<String>,
    pub(super) managed_resources: Vec<ManagedResource>,
    pub(super) managed_resources_error: Option<String>,
    pub(super) data_editor: Option<ResourceDataEditorState>,
    pub(super) back_stack: Vec<ResourceDetailHistoryEntry>,
    pub(super) forward_stack: Vec<ResourceDetailHistoryEntry>,
    pub(super) transition: Option<ResourceDetailTransition>,
    /// Avoid treating the row click which opened the overlay as a scrim dismissal.
    pub(super) dismiss_on_outside_click: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ResourceDetailTransition {
    Opening,
    Forward,
    Back,
    Closing,
}

#[derive(Debug, Clone)]
pub(super) struct ResourceDetailHistoryEntry {
    /// Distinguishes repeated visits to the same Kubernetes resource.
    pub(super) history_entry_id: u64,
    pub(super) api_resource: ApiResource,
    pub(super) namespace: Option<String>,
    pub(super) resource_name: String,
    pub(super) resource_uid: String,
    pub(super) detail: Option<ResourceDetail>,
    pub(super) events: Vec<ResourceEvent>,
    pub(super) detail_error: Option<String>,
    pub(super) events_error: Option<String>,
    pub(super) managed_resources: Vec<ManagedResource>,
    pub(super) managed_resources_error: Option<String>,
    pub(super) data_editor: Option<ResourceDataEditorState>,
}

impl ResourceDetailPanelState {
    fn history_entry(&self) -> ResourceDetailHistoryEntry {
        ResourceDetailHistoryEntry {
            history_entry_id: self.history_entry_id,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            resource_uid: self.resource_uid.clone(),
            detail: self.detail.clone(),
            events: self.events.clone(),
            detail_error: self.detail_error.clone(),
            events_error: self.events_error.clone(),
            managed_resources: self.managed_resources.clone(),
            managed_resources_error: self.managed_resources_error.clone(),
            data_editor: self.data_editor.clone(),
        }
    }

    fn replace_current(&mut self, entry: ResourceDetailHistoryEntry, selection_generation: u64) {
        self.history_entry_id = entry.history_entry_id;
        self.api_resource = entry.api_resource;
        self.namespace = entry.namespace;
        self.resource_name = entry.resource_name;
        self.resource_uid = entry.resource_uid;
        self.selection_generation = selection_generation;
        self.detail = entry.detail;
        self.events = entry.events;
        self.detail_error = entry.detail_error;
        self.events_error = entry.events_error;
        self.managed_resources = entry.managed_resources;
        self.managed_resources_error = entry.managed_resources_error;
        self.data_editor = entry.data_editor;
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResourceDataEditorState {
    /// The last resource data map accepted from the live watcher. Secret entries
    /// which cannot be represented as UTF-8 are deliberately absent.
    pub(super) server_values: BTreeMap<String, String>,
    pub(super) resource_version: String,
    pub(super) draft_values: BTreeMap<String, String>,
    pub(super) pending_external_values: Option<BTreeMap<String, String>>,
    pub(super) pending_external_resource_version: Option<String>,
    pub(super) revealed_secret_keys: HashSet<String>,
    pub(super) saving: bool,
    pub(super) save_error: Option<String>,
}

impl ResourceDataEditorState {
    fn new(values: BTreeMap<String, String>, resource_version: String) -> Self {
        Self {
            draft_values: values.clone(),
            server_values: values,
            resource_version,
            pending_external_values: None,
            pending_external_resource_version: None,
            revealed_secret_keys: HashSet::new(),
            saving: false,
            save_error: None,
        }
    }

    pub(super) fn is_modified(&self) -> bool {
        self.draft_values != self.server_values
    }

    pub(super) fn changed_values(&self) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
        let mut expected = BTreeMap::new();
        let mut updated = BTreeMap::new();
        for (key, value) in &self.draft_values {
            if self.server_values.get(key) != Some(value) {
                if let Some(expected_value) = self.server_values.get(key) {
                    expected.insert(key.clone(), expected_value.clone());
                    updated.insert(key.clone(), value.clone());
                }
            }
        }
        (expected, updated)
    }

    fn accept_watched_values(
        &mut self,
        values: BTreeMap<String, String>,
        resource_version: String,
    ) {
        if !self.is_modified() {
            self.server_values = values.clone();
            self.draft_values = values;
            self.resource_version = resource_version;
            self.pending_external_values = None;
            self.pending_external_resource_version = None;
            return;
        }
        if self.server_values != values {
            self.pending_external_values = Some(values);
            self.pending_external_resource_version = Some(resource_version);
        } else {
            self.resource_version = resource_version;
        }
    }

    pub(super) fn use_external_values(&mut self) {
        let Some(values) = self.pending_external_values.take() else {
            return;
        };
        self.server_values = values.clone();
        self.draft_values = values;
        self.resource_version = self
            .pending_external_resource_version
            .take()
            .unwrap_or_default();
        self.save_error = None;
    }

    pub(super) fn keep_local_edits(&mut self) {
        let Some(values) = self.pending_external_values.take() else {
            return;
        };
        let dirty_values = self
            .draft_values
            .iter()
            .filter(|(key, value)| self.server_values.get(*key) != Some(*value))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        self.server_values = values.clone();
        self.resource_version = self
            .pending_external_resource_version
            .take()
            .unwrap_or_default();
        self.draft_values = values;
        for (key, value) in dirty_values {
            if self.server_values.contains_key(&key) {
                self.draft_values.insert(key, value);
            } else {
                self.save_error = Some(format!(
                    "A changed data key was removed on the cluster and cannot be saved."
                ));
            }
        }
    }

    pub(super) fn mark_saved(&mut self) {
        let (expected, updated) = self.changed_values();
        for key in expected.keys() {
            if let Some(value) = updated.get(key) {
                self.server_values.insert(key.clone(), value.clone());
            }
        }
        self.saving = false;
        self.save_error = None;
    }
}

pub(super) enum ResourceAction {
    OpenDetails {
        name: String,
        namespace: Option<String>,
        uid: String,
    },
    EditYaml {
        name: String,
        namespace: Option<String>,
    },
    RequestDelete {
        name: String,
        namespace: Option<String>,
    },
    SaveData {
        expected_values: BTreeMap<String, String>,
        updated_values: BTreeMap<String, String>,
    },
    NavigateDetails {
        api_resource: ApiResource,
        name: String,
        namespace: Option<String>,
        uid: String,
    },
    NavigateBack,
    NavigateForward,
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
    pub(super) custom_resource_columns: BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    pub(super) selected_api_resource: Option<ApiResource>,
    pub(super) resource_cache: HashMap<ResourceWatchKey, ResourceWatchState>,
    pub(super) active_watchers: HashSet<ResourceWatchKey>,
    pub(super) resource_searches: HashMap<ApiResource, ResourceSearchState>,
    pub(super) yaml_panel: Option<YamlPanelState>,
    pub(super) resource_detail_panel: Option<ResourceDetailPanelState>,
    pub(super) next_detail_generation: u64,
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
        cluster.custom_resource_columns.clear();
        cluster.selected_namespaces.clear();
        cluster.selected_api_resource = None;
        cluster.resource_cache.clear();
        cluster.active_watchers.clear();
        cluster.resource_searches.clear();

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

        if cluster.resource_detail_panel.take().is_some() {
            commands_to_send.push(crate::worker::WorkerCommand::StopResourceDetailWatch {
                cluster_key: cluster.cluster_key,
            });
        }
        cluster.selected_api_resource = Some(api_resource);
        let api_resource = cluster
            .selected_api_resource
            .clone()
            .expect("selected API resource was just set");
        Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
    }

    pub(super) fn open_resource_detail(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        name: String,
        namespace: Option<String>,
        uid: String,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        cluster.next_detail_generation += 1;
        let selection_generation = cluster.next_detail_generation;
        cluster.resource_detail_panel = Some(ResourceDetailPanelState {
            api_resource: api_resource.clone(),
            namespace: namespace.clone(),
            resource_name: name.clone(),
            resource_uid: uid.clone(),
            history_entry_id: selection_generation,
            selection_generation,
            detail: None,
            events: Vec::new(),
            detail_error: None,
            events_error: None,
            managed_resources: Vec::new(),
            managed_resources_error: None,
            data_editor: None,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            transition: Some(ResourceDetailTransition::Opening),
            dismiss_on_outside_click: false,
        });
        commands_to_send.push(crate::worker::WorkerCommand::StartResourceDetailWatch {
            cluster_key: cluster.cluster_key,
            api_resource,
            namespace,
            resource_name: name,
            resource_uid: uid,
            selection_generation,
        });
    }

    pub(super) fn navigate_resource_detail(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        name: String,
        namespace: Option<String>,
        uid: String,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        let Some(panel) = cluster.resource_detail_panel.as_mut() else {
            return;
        };
        panel.back_stack.push(panel.history_entry());
        panel.forward_stack.clear();
        cluster.next_detail_generation += 1;
        let selection_generation = cluster.next_detail_generation;
        panel.replace_current(
            ResourceDetailHistoryEntry {
                history_entry_id: selection_generation,
                api_resource: api_resource.clone(),
                namespace: namespace.clone(),
                resource_name: name.clone(),
                resource_uid: uid.clone(),
                detail: None,
                events: Vec::new(),
                detail_error: None,
                events_error: None,
                managed_resources: Vec::new(),
                managed_resources_error: None,
                data_editor: None,
            },
            selection_generation,
        );
        panel.transition = Some(ResourceDetailTransition::Forward);
        commands_to_send.push(crate::worker::WorkerCommand::StartResourceDetailWatch {
            cluster_key: cluster.cluster_key,
            api_resource,
            namespace,
            resource_name: name,
            resource_uid: uid,
            selection_generation,
        });
    }

    pub(super) fn navigate_resource_detail_history(
        &mut self,
        cluster_key: i32,
        forward: bool,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        let Some(panel) = cluster.resource_detail_panel.as_mut() else {
            return;
        };
        let destination = if forward {
            panel.forward_stack.pop()
        } else {
            panel.back_stack.pop()
        };
        let Some(destination) = destination else {
            return;
        };
        let current = panel.history_entry();
        if forward {
            panel.back_stack.push(current);
        } else {
            panel.forward_stack.push(current);
        }
        cluster.next_detail_generation += 1;
        let selection_generation = cluster.next_detail_generation;
        let api_resource = destination.api_resource.clone();
        let namespace = destination.namespace.clone();
        let resource_name = destination.resource_name.clone();
        let resource_uid = destination.resource_uid.clone();
        panel.replace_current(destination, selection_generation);
        panel.transition = Some(if forward {
            ResourceDetailTransition::Forward
        } else {
            ResourceDetailTransition::Back
        });
        commands_to_send.push(crate::worker::WorkerCommand::StartResourceDetailWatch {
            cluster_key: cluster.cluster_key,
            api_resource,
            namespace,
            resource_name,
            resource_uid,
            selection_generation,
        });
    }

    pub(super) fn close_resource_detail(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        if cluster.resource_detail_panel.take().is_some() {
            commands_to_send.push(crate::worker::WorkerCommand::StopResourceDetailWatch {
                cluster_key: cluster.cluster_key,
            });
        }
    }

    pub(super) fn begin_close_resource_detail(&mut self, cluster_key: i32) -> bool {
        let Some(panel) = self
            .clusters
            .get_mut(&cluster_key)
            .and_then(|cluster| cluster.resource_detail_panel.as_mut())
        else {
            return false;
        };
        if matches!(panel.transition, Some(ResourceDetailTransition::Closing)) {
            return false;
        }
        panel.transition = Some(ResourceDetailTransition::Closing);
        true
    }

    pub(super) fn close_all_resource_details(
        &mut self,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        for cluster in self.clusters.values_mut() {
            if cluster.resource_detail_panel.take().is_some() {
                commands_to_send.push(crate::worker::WorkerCommand::StopResourceDetailWatch {
                    cluster_key: cluster.cluster_key,
                });
            }
        }
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
        Self::request_resource_watch(cluster, &api_resource, Some(namespace), commands_to_send);
    }

    /// Replace the visible namespace scope without cancelling existing watches.
    pub(super) fn replace_selected_namespaces<I>(
        &mut self,
        cluster_key: i32,
        namespaces: I,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) where
        I: IntoIterator<Item = String>,
    {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };

        cluster.selected_namespaces = namespaces.into_iter().collect();
        let Some(api_resource) = cluster.selected_api_resource.clone() else {
            return;
        };
        Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
    }

    /// Select every discovered namespace without cancelling existing watches.
    pub(super) fn select_all_namespaces(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<crate::worker::WorkerCommand>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };

        cluster.selected_namespaces = cluster
            .namespaces
            .values()
            .map(|namespace| namespace.name.clone())
            .collect();
        let Some(api_resource) = cluster.selected_api_resource.clone() else {
            return;
        };
        Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
    }

    /// Clear the visible namespace scope without cancelling existing watches.
    pub(super) fn clear_selected_namespaces(&mut self, cluster_key: i32) {
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            cluster.selected_namespaces.clear();
        }
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
        if api_resource.namespaced {
            let namespaces = cluster.selected_namespaces.clone();
            for namespace in namespaces {
                Self::request_resource_watch(
                    cluster,
                    api_resource,
                    Some(namespace),
                    commands_to_send,
                );
            }
        } else {
            Self::request_resource_watch(cluster, api_resource, None, commands_to_send);
        }
    }

    fn request_resource_watch(
        cluster: &mut ClusterState,
        api_resource: &ApiResource,
        namespace: Option<String>,
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
                        Some(crate::worker::WorkerCommand::StartResourceDetailWatch {
                            cluster_key,
                            selection_generation,
                            ..
                        }) => {
                            if let Some(panel) = self
                                .clusters
                                .get_mut(&cluster_key)
                                .and_then(|cluster| cluster.resource_detail_panel.as_mut())
                                .filter(|panel| panel.selection_generation == selection_generation)
                            {
                                panel.detail_error = Some(message);
                            }
                        }
                        Some(crate::worker::WorkerCommand::UpdateResourceData {
                            cluster_key,
                            resource_name,
                            ..
                        }) => {
                            if let Some(editor) = self
                                .clusters
                                .get_mut(&cluster_key)
                                .and_then(|cluster| cluster.resource_detail_panel.as_mut())
                                .filter(|panel| panel.resource_name == resource_name)
                                .and_then(|panel| panel.data_editor.as_mut())
                            {
                                editor.saving = false;
                                editor.save_error = Some(message);
                            }
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
                                custom_resource_columns: BTreeMap::new(),
                                resource_cache: HashMap::new(),
                                active_watchers: HashSet::new(),
                                resource_searches: HashMap::new(),
                                yaml_panel: None,
                                resource_detail_panel: None,
                                next_detail_generation: 0,
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
                WorkerResult::KubernetesCustomResourceColumnsLoaded {
                    cluster_key,
                    columns,
                } => {
                    if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                        cluster.custom_resource_columns.extend(columns);
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
                        if cluster
                            .resource_detail_panel
                            .as_ref()
                            .is_some_and(|panel| panel.resource_uid == resource_uid)
                        {
                            cluster.resource_detail_panel = None;
                            commands_to_send.push(
                                crate::worker::WorkerCommand::StopResourceDetailWatch {
                                    cluster_key,
                                },
                            );
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
                WorkerResult::ResourceDetailUpdated {
                    cluster_key,
                    selection_generation,
                    detail,
                } => {
                    if let Some(panel) = self
                        .clusters
                        .get_mut(&cluster_key)
                        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
                        .filter(|panel| panel.selection_generation == selection_generation)
                    {
                        sync_resource_data_editor(panel, &detail);
                        panel.detail = Some(detail);
                        panel.detail_error = None;
                    }
                }
                WorkerResult::ResourceEventsReplaced {
                    cluster_key,
                    selection_generation,
                    events,
                } => {
                    if let Some(panel) = self
                        .clusters
                        .get_mut(&cluster_key)
                        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
                        .filter(|panel| panel.selection_generation == selection_generation)
                    {
                        panel.events = events;
                        panel.events_error = None;
                    }
                }
                WorkerResult::ResourceDetailWatchFailed {
                    cluster_key,
                    selection_generation,
                    events,
                    error,
                } => {
                    if let Some(panel) = self
                        .clusters
                        .get_mut(&cluster_key)
                        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
                        .filter(|panel| panel.selection_generation == selection_generation)
                    {
                        if events {
                            panel.events_error = Some(error);
                        } else {
                            panel.detail_error = Some(error);
                        }
                    }
                }
                WorkerResult::ResourceDetailDeleted {
                    cluster_key,
                    selection_generation,
                } => {
                    if self
                        .clusters
                        .get(&cluster_key)
                        .and_then(|cluster| cluster.resource_detail_panel.as_ref())
                        .is_some_and(|panel| panel.selection_generation == selection_generation)
                    {
                        self.close_resource_detail(cluster_key, &mut commands_to_send);
                    }
                }
                WorkerResult::ManagedResourcesReplaced {
                    cluster_key,
                    selection_generation,
                    resources,
                } => {
                    if let Some(panel) = self
                        .clusters
                        .get_mut(&cluster_key)
                        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
                        .filter(|panel| panel.selection_generation == selection_generation)
                    {
                        panel.managed_resources = resources;
                        panel.managed_resources_error = None;
                    }
                }
                WorkerResult::ManagedResourcesWatchFailed {
                    cluster_key,
                    selection_generation,
                    error,
                } => {
                    if let Some(panel) = self
                        .clusters
                        .get_mut(&cluster_key)
                        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
                        .filter(|panel| panel.selection_generation == selection_generation)
                    {
                        panel.managed_resources_error = Some(error);
                    }
                }
                WorkerResult::ResourceDetailWatchStarted { .. }
                | WorkerResult::ResourceDetailWatchStopped { .. } => {}
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
                WorkerResult::ResourceDataUpdateCompleted {
                    cluster_key,
                    resource_name,
                } => {
                    if let Some(editor) = self
                        .clusters
                        .get_mut(&cluster_key)
                        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
                        .filter(|panel| panel.resource_name == resource_name)
                        .and_then(|panel| panel.data_editor.as_mut())
                    {
                        editor.mark_saved();
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
        namespace: Option<String>,
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

fn sync_resource_data_editor(panel: &mut ResourceDetailPanelState, detail: &ResourceDetail) {
    let values = match &detail.payload {
        ResourceDetailPayload::ConfigMap(config_map) => Some(config_map.data.clone()),
        ResourceDetailPayload::Secret(secret) => Some(
            secret
                .data
                .iter()
                .filter_map(|(key, value)| {
                    value.text.as_ref().map(|text| (key.clone(), text.clone()))
                })
                .collect(),
        ),
        ResourceDetailPayload::Generic | ResourceDetailPayload::Pod(_) => None,
    };
    match (panel.data_editor.as_mut(), values) {
        (Some(editor), Some(values)) => {
            editor.accept_watched_values(values, detail.resource_version.clone())
        }
        (None, Some(values)) => {
            panel.data_editor = Some(ResourceDataEditorState::new(
                values,
                detail.resource_version.clone(),
            ))
        }
        (_, None) => panel.data_editor = None,
    }
}
