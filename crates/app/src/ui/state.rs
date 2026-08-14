use super::global_blade::{GlobalBladeContent, GlobalBladeCoordinator};
use crate::api_resource::ApiResource;
use crate::log_store::LogStoreResult;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::{MinimalResource, PodLogContainer};
use crate::resource_catalog::{ResourceNavigation, build_resource_navigation};
use crate::resource_detail::{
    ManagedResource, ResourceDetail, ResourceDetailPayload, ResourceEvent,
};
use crate::resource_schema::{
    CompletionContext, CompletionSuggestion, ResourceSchema, SourceRange, YamlDiagnostic,
    kubernetes_field_path_to_json_pointer,
};
use crate::resource_table::CustomResourceColumn;
use crate::sorted_name::SortedName;
use crate::terminal_launcher::{DebugImagePreset, ShellRequest, TerminalLaunchSettings};
use crate::worker::{ResourceApiError, WorkerCommandBox, WorkerResult, WorkerTrait};
#[cfg(test)]
use components::BladeNavigator;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

pub(super) use super::log_state::{
    LogDisplayOptions, LogPageKey, LogTextPosition, LogTextSelection, PendingLogCaret,
    PodLogStatus, PodLogWindowState,
};

pub(super) use super::persistence::{
    PersistedApiResource, PersistedClusterSelection, PersistedClusterSelections,
    ResourceNavigationExpansion,
};

const DELETE_CONFIRMATION_DELAY: Duration = Duration::from_secs(3);

#[derive(Default)]
pub(crate) struct UiState {
    pub(super) clusters: HashMap<i32, ClusterState>,
    pub(super) next_cluster_key: i32,
    pub(super) selected_cluster: Option<i32>,
    pub(super) log_windows: BTreeMap<u64, PodLogWindowState>,
    pub(super) next_log_window_id: u64,
    pub(super) yaml_editors: BTreeMap<u64, YamlEditorWindowState>,
    pub(super) next_yaml_editor_id: u64,
    pub(super) resource_schemas: HashMap<(i32, ApiResource), ResourceSchema>,
    pub(super) log_display_options: LogDisplayOptions,
    pub(super) global_blades: GlobalBladeCoordinator,
    pub(super) terminal_launch_error: Option<String>,
    pub(super) cluster_selections: PersistedClusterSelections,
    pub(super) resource_navigation_expansion: ResourceNavigationExpansion,
}

/// Key for identifying a resource watcher (API resource + optional namespace).
pub(super) type ResourceWatchKey = (ApiResource, Option<String>);

pub(super) fn stop_resource_detail_watches(
    cluster_key: i32,
    history_entry_ids: impl IntoIterator<Item = u64>,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    commands_to_send.extend(history_entry_ids.into_iter().map(|history_entry_id| {
        Box::new(crate::worker::StopResourceDetailWatch {
            cluster_key,
            history_entry_id,
        }) as WorkerCommandBox
    }));
}

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
pub(super) struct YamlEditorWindowState {
    pub(super) id: u64,
    pub(super) cluster_key: i32,
    pub(super) api_resource: ApiResource,
    pub(super) namespace: Option<String>,
    pub(super) resource_name: String,
    pub(super) original_yaml: Option<String>,
    pub(super) edited_yaml: String,
    pub(super) loading: bool,
    pub(super) saving: bool,
    pub(super) error: Option<String>,
    pub(super) close_requested: bool,
    pub(super) confirm_discard: bool,
    pub(super) focus_requested: bool,
    pub(super) schema: Option<ResourceSchema>,
    pub(super) schema_loading: bool,
    pub(super) diagnostics: Vec<YamlDiagnostic>,
    /// The last diagnostics shown in the pane while a newer document is being validated.
    /// These are intentionally separate from `diagnostics`, whose ranges must always match
    /// the current editor buffer before they are used for line markers or squiggles.
    pub(super) retained_diagnostics: Vec<YamlDiagnostic>,
    pub(super) scroll_to_diagnostic: Option<SourceRange>,
    pub(super) server_validation: ValidationState,
    pub(super) validation_revision: u64,
    pub(super) validation_due: Option<Instant>,
    pub(super) suggestions: Vec<CompletionSuggestion>,
    pub(super) completion_context: Option<CompletionContext>,
    pub(super) completion_cursor: Option<usize>,
    pub(super) suggestions_visible: bool,
    pub(super) suggestion_selection: usize,
    pub(super) search: YamlEditorSearchState,
}

#[derive(Debug, Clone, Default)]
pub(super) struct YamlEditorSearchState {
    pub(super) query: String,
    pub(super) regex_mode: bool,
    pub(super) input_focused: bool,
    pub(super) active_match: Option<usize>,
    /// The next rendered editor frame scrolls this match into view, then clears it.
    pub(super) scroll_to_match: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) enum ValidationState {
    #[default]
    Idle,
    Pending,
    Valid,
    Failed(String),
}

pub(super) fn diagnostics_from_api_error(
    error: &ResourceApiError,
    yaml: &str,
) -> Vec<YamlDiagnostic> {
    error
        .causes
        .iter()
        .filter_map(|cause| {
            let detail = if cause.message.is_empty() {
                cause.reason.as_str()
            } else {
                cause.message.as_str()
            };
            if detail.is_empty() {
                return None;
            }
            let message = if cause.field.is_empty() {
                detail.to_owned()
            } else {
                format!("{}: {detail}", cause.field)
            };
            let path = kubernetes_field_path_to_json_pointer(&cause.field).unwrap_or_default();
            Some(YamlDiagnostic::at_path(path, message).locate_in(yaml))
        })
        .collect()
}

pub(super) fn api_error_message(error: &ResourceApiError) -> String {
    if !error.message.is_empty() {
        error.message.clone()
    } else {
        "The Kubernetes API rejected this resource".into()
    }
}

pub(super) fn set_editor_diagnostics(
    editor: &mut YamlEditorWindowState,
    diagnostics: Vec<YamlDiagnostic>,
) {
    editor.diagnostics = diagnostics;
    if !editor.diagnostics.is_empty() {
        editor.retained_diagnostics = editor.diagnostics.clone();
    }
}

impl YamlEditorWindowState {
    pub(super) fn is_modified(&self) -> bool {
        self.original_yaml
            .as_ref()
            .is_some_and(|original_yaml| original_yaml != &self.edited_yaml)
    }

    pub(super) fn resource_matches(
        &self,
        cluster_key: i32,
        api_resource: &ApiResource,
        namespace: &Option<String>,
        resource_name: &str,
    ) -> bool {
        self.cluster_key == cluster_key
            && self.api_resource == *api_resource
            && self.namespace == *namespace
            && self.resource_name == resource_name
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingDelete {
    pub(super) api_resource: ApiResource,
    pub(super) resource_name: String,
    pub(super) namespace: Option<String>,
    pub(super) confirmation_available_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct BulkDeleteTarget {
    pub(super) uid: String,
    pub(super) name: String,
    pub(super) namespace: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct PendingBulkDelete {
    pub(super) api_resource: ApiResource,
    pub(super) targets: Vec<BulkDeleteTarget>,
    pub(super) confirmation_available_at: Instant,
}

impl PendingBulkDelete {
    pub(super) fn new(api_resource: ApiResource, targets: Vec<BulkDeleteTarget>) -> Self {
        Self {
            api_resource,
            targets,
            confirmation_available_at: Instant::now() + DELETE_CONFIRMATION_DELAY,
        }
    }
}

#[derive(Debug)]
pub(super) struct BulkDeleteProgress {
    pub(super) id: u64,
    pub(super) api_resource: ApiResource,
    pub(super) remaining_targets: HashSet<BulkDeleteTarget>,
    pub(super) failures: Vec<(BulkDeleteTarget, String)>,
}

impl BulkDeleteProgress {
    pub(super) fn new(id: u64, api_resource: ApiResource, targets: Vec<BulkDeleteTarget>) -> Self {
        Self {
            id,
            api_resource,
            remaining_targets: targets.into_iter().collect(),
            failures: Vec::new(),
        }
    }

    fn target_for(
        &self,
        api_resource: &ApiResource,
        name: &str,
        namespace: &Option<String>,
    ) -> Option<BulkDeleteTarget> {
        if self.api_resource != *api_resource {
            return None;
        }
        self.remaining_targets
            .iter()
            .find(|target| target.name == name && target.namespace == *namespace)
            .cloned()
    }
}

impl BulkDeleteTarget {
    pub(super) fn display_name(&self) -> String {
        self.namespace.as_deref().map_or_else(
            || self.name.clone(),
            |namespace| format!("{namespace}/{}", self.name),
        )
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingForceDelete {
    pub(super) api_resource: ApiResource,
    pub(super) resource_name: String,
    pub(super) resource_uid: String,
    pub(super) namespace: Option<String>,
    pub(super) finalizers: Vec<String>,
    pub(super) acknowledgement: String,
    pub(super) confirmation_available_at: Instant,
}

impl PendingForceDelete {
    pub(super) fn new(
        api_resource: ApiResource,
        resource_name: String,
        resource_uid: String,
        namespace: Option<String>,
        finalizers: Vec<String>,
    ) -> Self {
        Self {
            api_resource,
            resource_name,
            resource_uid,
            namespace,
            finalizers,
            acknowledgement: String::new(),
            confirmation_available_at: Instant::now() + DELETE_CONFIRMATION_DELAY,
        }
    }
}

impl PendingDelete {
    pub(super) fn new(
        api_resource: ApiResource,
        resource_name: String,
        namespace: Option<String>,
    ) -> Self {
        Self {
            api_resource,
            resource_name,
            namespace,
            confirmation_available_at: Instant::now() + DELETE_CONFIRMATION_DELAY,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingDeploymentRestart {
    pub(super) resource_name: String,
    pub(super) namespace: String,
}

#[derive(Debug, Clone)]
pub(super) struct PendingScale {
    pub(super) api_resource: ApiResource,
    pub(super) resource_name: String,
    pub(super) namespace: Option<String>,
    pub(super) current_replicas: i32,
    pub(super) desired_replicas: String,
}

#[derive(Debug)]
pub(super) struct ResourceDetailPanelState {
    /// Avoid treating the row click which opened the overlay as a scrim dismissal.
    pub(super) dismiss_on_outside_click: bool,
}

#[derive(Debug)]
pub(super) struct ResourceDetailHistoryEntry {
    /// Distinguishes repeated visits to the same Kubernetes resource.
    pub(super) history_entry_id: u64,
    pub(super) cluster_key: i32,
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
    /// UI interactions are recorded while rendering, then consumed by the
    /// global blade coordinator after the navigator borrow ends.
    pub(super) pending_action: Option<ResourceAction>,
}

impl UiState {
    /// Replace the sole global blade root and perform the lifecycle cleanup
    /// that every root replacement requires. Feature modules must use this
    /// instead of manipulating the coordinator directly.
    pub(super) fn replace_global_blade(
        &mut self,
        content: Box<dyn GlobalBladeContent>,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let discarded = self.global_blades.open(content);
        Self::stop_discarded_blades(discarded, commands_to_send);
        for cluster in self.clusters.values_mut() {
            cluster.resource_detail_panel = None;
        }
    }

    #[cfg(test)]
    pub(super) fn terminal_settings_blade(
        &self,
    ) -> Option<&super::settings::TerminalSettingsBlade> {
        self.global_blades
            .navigator()?
            .current()
            .terminal_settings()
    }
    pub(super) fn resource_detail_entry_mut(
        &mut self,
        history_entry_id: u64,
    ) -> Option<&mut ResourceDetailHistoryEntry> {
        self.global_blades
            .navigator_mut()?
            .entries_mut()
            .filter_map(|entry| entry.resource_detail_mut())
            .find(|entry| entry.history_entry_id == history_entry_id)
    }

    pub(super) fn stop_discarded_blades(
        discarded: impl IntoIterator<Item = Box<dyn GlobalBladeContent>>,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let mut entries_by_cluster = HashMap::<i32, Vec<u64>>::new();
        for content in discarded {
            if let Some(entry) = content.resource_detail() {
                entries_by_cluster
                    .entry(entry.cluster_key)
                    .or_default()
                    .push(entry.history_entry_id);
            }
        }
        for (cluster_key, history_entry_ids) in entries_by_cluster {
            stop_resource_detail_watches(cluster_key, history_entry_ids, commands_to_send);
        }
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
    pub(super) pending_save_request_id: Option<u64>,
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
            pending_save_request_id: None,
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
            if self.server_values.get(key) != Some(value)
                && let Some(expected_value) = self.server_values.get(key)
            {
                expected.insert(key.clone(), expected_value.clone());
                updated.insert(key.clone(), value.clone());
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
                self.save_error = Some(
                    "A changed data key was removed on the cluster and cannot be saved.".to_owned(),
                );
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
        self.pending_save_request_id = None;
        self.save_error = None;
    }
}

#[derive(Debug)]
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
    RequestForceDelete {
        name: String,
        uid: String,
        namespace: Option<String>,
        finalizers: Vec<String>,
    },
    RequestDeploymentRestart {
        name: String,
        namespace: String,
    },
    RequestScale {
        name: String,
        namespace: Option<String>,
    },
    SaveData {
        expected_values: BTreeMap<String, String>,
        updated_values: BTreeMap<String, String>,
    },
    ViewLogs {
        name: String,
        namespace: Option<String>,
        container: PodLogContainer,
    },
    Shell {
        name: String,
        namespace: Option<String>,
        container: PodLogContainer,
    },
    PodDebugShell {
        name: String,
        namespace: Option<String>,
        target_container: String,
        preset: DebugImagePreset,
    },
    NodeShell {
        name: String,
        preset: DebugImagePreset,
    },
    NavigateDetails {
        api_resource: ApiResource,
        name: String,
        namespace: Option<String>,
        uid: String,
    },
}

impl ResourceAction {
    pub(super) fn shell_request(&self, kube_context: &str) -> Option<ShellRequest> {
        match self {
            Self::Shell {
                name,
                namespace: Some(namespace),
                container,
            } => Some(ShellRequest::Pod {
                kube_context: kube_context.to_owned(),
                namespace: namespace.clone(),
                pod_name: name.clone(),
                container: container.name.clone(),
            }),
            Self::NodeShell { name, preset } => Some(ShellRequest::Node {
                kube_context: kube_context.to_owned(),
                node_name: name.clone(),
                preset: preset.clone(),
            }),
            Self::PodDebugShell {
                name,
                namespace: Some(namespace),
                target_container,
                preset,
            } => Some(ShellRequest::PodDebug {
                kube_context: kube_context.to_owned(),
                namespace: namespace.clone(),
                pod_name: name.clone(),
                target_container: target_container.clone(),
                preset: preset.clone(),
            }),
            Self::Shell {
                namespace: None, ..
            }
            | Self::PodDebugShell {
                namespace: None, ..
            }
            | Self::OpenDetails { .. }
            | Self::EditYaml { .. }
            | Self::RequestDelete { .. }
            | Self::RequestForceDelete { .. }
            | Self::RequestDeploymentRestart { .. }
            | Self::RequestScale { .. }
            | Self::SaveData { .. }
            | Self::ViewLogs { .. }
            | Self::NavigateDetails { .. } => None,
        }
    }
}

#[derive(Debug)]
pub(super) struct ClusterState {
    pub(super) name: String,
    pub(super) cluster_key: i32,
    pub(super) namespaces: BTreeMap<SortedName, MinimalNamespace>,
    pub(super) connection: ClusterConnectionState,
    pub(super) namespaces_load: ClusterLoadState,
    pub(super) api_resources_load: ClusterLoadState,
    pub(super) selected_namespaces: HashSet<String>,
    pub(super) resource_navigation: ResourceNavigation,
    pub(super) custom_resource_columns: BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    pub(super) scalable_api_resources: BTreeSet<ApiResource>,
    pub(super) selected_api_resource: Option<ApiResource>,
    pub(super) resource_cache: HashMap<ResourceWatchKey, ResourceWatchState>,
    pub(super) active_watchers: HashSet<ResourceWatchKey>,
    pub(super) resource_searches: HashMap<ApiResource, ResourceSearchState>,
    pub(super) resource_selections: HashMap<ApiResource, HashSet<String>>,
    pub(super) next_bulk_delete_id: u64,
    pub(super) resource_detail_panel: Option<ResourceDetailPanelState>,
    pub(super) next_detail_generation: u64,
    pub(super) next_data_save_request_id: u64,
    pub(super) pending_delete: Option<PendingDelete>,
    pub(super) pending_bulk_delete: Option<PendingBulkDelete>,
    pub(super) bulk_delete_progress: Option<BulkDeleteProgress>,
    pub(super) bulk_delete_error: Option<String>,
    pub(super) pending_force_delete: Option<PendingForceDelete>,
    pub(super) force_delete_error: Option<String>,
    pub(super) pending_deployment_restart: Option<PendingDeploymentRestart>,
    pub(super) deployment_restart_error: Option<String>,
    pub(super) pending_scale: Option<PendingScale>,
    pub(super) scale_error: Option<String>,
}

#[derive(Debug)]
pub(super) enum ClusterConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Failed(String),
}

impl UiState {
    pub(super) fn resource_navigation_node_is_expanded(&self, node_id: &str) -> bool {
        self.resource_navigation_expansion
            .expanded_nodes
            .contains(node_id)
    }

    pub(super) fn set_resource_navigation_node_expanded(
        &mut self,
        node_id: impl Into<String>,
        is_expanded: bool,
    ) {
        let node_id = node_id.into();
        if is_expanded {
            self.resource_navigation_expansion
                .expanded_nodes
                .insert(node_id);
        } else {
            self.resource_navigation_expansion
                .expanded_nodes
                .remove(&node_id);
        }
    }

    fn remember_selected_namespaces(&mut self, cluster_key: i32) {
        let Some(cluster) = self.clusters.get(&cluster_key) else {
            return;
        };
        let context_name = cluster.name.clone();
        let namespaces = cluster.selected_namespaces.iter().cloned().collect();
        self.cluster_selections
            .selections
            .entry(context_name)
            .or_default()
            .selected_namespaces = namespaces;
        self.prune_empty_cluster_selection(cluster_key);
    }

    fn remember_selected_api_resource(&mut self, cluster_key: i32, api_resource: &ApiResource) {
        let Some(context_name) = self
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.name.clone())
        else {
            return;
        };
        self.cluster_selections
            .selections
            .entry(context_name)
            .or_default()
            .selected_api_resource = Some(PersistedApiResource::from_api_resource(api_resource));
    }

    fn prune_empty_cluster_selection(&mut self, cluster_key: i32) {
        let Some(context_name) = self
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.name.clone())
        else {
            return;
        };
        if self
            .cluster_selections
            .selections
            .get(&context_name)
            .is_some_and(|selection| selection == &PersistedClusterSelection::default())
        {
            self.cluster_selections.selections.remove(&context_name);
        }
    }

    fn restore_selected_namespaces(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(cluster) = self.clusters.get(&cluster_key) else {
            return;
        };
        let context_name = cluster.name.clone();
        let available_namespaces = cluster
            .namespaces
            .values()
            .map(|namespace| namespace.name.clone())
            .collect::<BTreeSet<_>>();

        let restored_namespaces =
            if let Some(selection) = self.cluster_selections.selections.get_mut(&context_name) {
                selection
                    .selected_namespaces
                    .retain(|namespace| available_namespaces.contains(namespace));
                selection.selected_namespaces.clone()
            } else {
                BTreeSet::new()
            };

        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            cluster.selected_namespaces = restored_namespaces.into_iter().collect();
            if let Some(api_resource) = cluster.selected_api_resource.clone() {
                Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
            }
        }
        self.prune_empty_cluster_selection(cluster_key);
    }

    fn restored_api_resource(
        &mut self,
        cluster_key: i32,
        api_resources: &[ApiResource],
    ) -> Option<ApiResource> {
        let context_name = self
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.name.clone())?;
        let saved_resource = self
            .cluster_selections
            .selections
            .get(&context_name)
            .and_then(|selection| selection.selected_api_resource.as_ref());
        let api_resource = saved_resource.and_then(|saved_resource| {
            api_resources
                .iter()
                .find(|api_resource| saved_resource.matches(api_resource))
                .cloned()
        });

        if saved_resource.is_some() && api_resource.is_none() {
            if let Some(selection) = self.cluster_selections.selections.get_mut(&context_name) {
                selection.selected_api_resource = None;
            }
            self.prune_empty_cluster_selection(cluster_key);
            return None;
        }

        api_resource
    }

    pub(super) fn open_terminal_settings(
        &mut self,
        settings: &TerminalLaunchSettings,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        self.replace_global_blade(
            Box::new(super::settings::TerminalSettingsBlade::new(
                settings.clone(),
            )),
            commands_to_send,
        );
    }

    pub(super) fn open_pod_log_window(
        &mut self,
        cluster_key: i32,
        pod_name: String,
        namespace: Option<String>,
        container: PodLogContainer,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(namespace) = namespace else {
            return;
        };
        self.next_log_window_id += 1;
        let log_window_id = self.next_log_window_id;
        self.log_windows.insert(
            log_window_id,
            PodLogWindowState::new(
                log_window_id,
                cluster_key,
                namespace.clone(),
                pod_name.clone(),
                container.clone(),
            ),
        );
        commands_to_send.push(Box::new(crate::worker::StartPodLogStream {
            cluster_key,
            log_window_id,
            namespace,
            pod_name,
            container: container.name,
        }));
    }

    pub(super) fn open_yaml_editor(
        &mut self,
        ctx: &egui::Context,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        if let Some(editor) = self.yaml_editors.values_mut().find(|editor| {
            editor.resource_matches(cluster_key, &api_resource, &namespace, &resource_name)
        }) {
            editor.focus_requested = true;
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of(("yaml-editor-window", editor.id)),
                egui::ViewportCommand::Focus,
            );
            return;
        }

        self.next_yaml_editor_id += 1;
        let editor_id = self.next_yaml_editor_id;
        self.yaml_editors.insert(
            editor_id,
            YamlEditorWindowState {
                id: editor_id,
                cluster_key,
                api_resource: api_resource.clone(),
                namespace: namespace.clone(),
                resource_name: resource_name.clone(),
                original_yaml: None,
                edited_yaml: String::new(),
                loading: true,
                saving: false,
                error: None,
                close_requested: false,
                confirm_discard: false,
                focus_requested: false,
                schema: self
                    .resource_schemas
                    .get(&(cluster_key, api_resource.clone()))
                    .cloned(),
                schema_loading: !self
                    .resource_schemas
                    .contains_key(&(cluster_key, api_resource.clone())),
                diagnostics: Vec::new(),
                retained_diagnostics: Vec::new(),
                scroll_to_diagnostic: None,
                server_validation: ValidationState::Idle,
                validation_revision: 0,
                validation_due: None,
                suggestions: Vec::new(),
                completion_context: None,
                completion_cursor: None,
                suggestions_visible: false,
                suggestion_selection: 0,
                search: YamlEditorSearchState::default(),
            },
        );
        commands_to_send.push(Box::new(crate::worker::GetResourceYaml {
            editor_id,
            cluster_key,
            api_resource: api_resource.clone(),
            namespace: namespace.clone(),
            resource_name: resource_name.clone(),
        }));
        if !self
            .resource_schemas
            .contains_key(&(cluster_key, api_resource.clone()))
        {
            commands_to_send.push(Box::new(crate::worker::LoadResourceSchema {
                editor_id,
                cluster_key,
                api_resource,
            }));
        }
    }

    pub(super) fn select_cluster(&mut self, cluster_key: i32) -> Option<WorkerCommandBox> {
        self.selected_cluster = Some(cluster_key);

        let cluster = self.clusters.get_mut(&cluster_key)?;
        if matches!(
            &cluster.connection,
            ClusterConnectionState::Connected | ClusterConnectionState::Connecting
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

        Some(Box::new(crate::worker::ConnectToCluster {
            cluster: cluster.name.clone(),
            cluster_key,
        }))
    }

    pub(super) fn select_api_resource(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let (api_resource, closed_resource_detail) = {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };

            let closed_resource_detail = cluster.resource_detail_panel.take().is_some();
            cluster.selected_api_resource = Some(api_resource);
            (
                cluster
                    .selected_api_resource
                    .clone()
                    .expect("selected API resource was just set"),
                closed_resource_detail,
            )
        };
        if closed_resource_detail {
            Self::stop_discarded_blades(self.global_blades.clear(), commands_to_send);
        }
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
        }
        self.remember_selected_api_resource(cluster_key, &api_resource);
    }

    pub(super) fn open_resource_detail(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        name: String,
        namespace: Option<String>,
        uid: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let selection_generation = {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };
            cluster.next_detail_generation += 1;
            cluster.next_detail_generation
        };
        self.replace_global_blade(
            Box::new(ResourceDetailHistoryEntry {
                history_entry_id: selection_generation,
                cluster_key,
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
                pending_action: None,
            }),
            commands_to_send,
        );
        let cluster = self
            .clusters
            .get_mut(&cluster_key)
            .expect("cluster was checked before opening its blade");
        cluster.resource_detail_panel = Some(ResourceDetailPanelState {
            dismiss_on_outside_click: false,
        });
        commands_to_send.push(Box::new(crate::worker::StartResourceDetailWatch {
            cluster_key: cluster.cluster_key,
            history_entry_id: selection_generation,
            api_resource,
            namespace,
            resource_name: name,
            resource_uid: uid,
        }));
    }

    #[cfg(test)]
    pub(super) fn navigate_resource_detail(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        name: String,
        namespace: Option<String>,
        uid: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        cluster.next_detail_generation += 1;
        let selection_generation = cluster.next_detail_generation;
        if cluster.resource_detail_panel.is_none() {
            return;
        }
        let entry = ResourceDetailHistoryEntry {
            history_entry_id: selection_generation,
            cluster_key,
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
            pending_action: None,
        };
        let discarded = self.global_blades.push(Box::new(entry));
        Self::stop_discarded_blades(discarded, commands_to_send);
        commands_to_send.push(Box::new(crate::worker::StartResourceDetailWatch {
            cluster_key: cluster.cluster_key,
            history_entry_id: selection_generation,
            api_resource,
            namespace,
            resource_name: name,
            resource_uid: uid,
        }));
    }

    #[cfg(test)]
    pub(super) fn navigate_resource_detail_history(
        &mut self,
        cluster_key: i32,
        forward: bool,
        _commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        let Some(_panel) = cluster.resource_detail_panel.as_mut() else {
            return;
        };
        if forward {
            let _ = self
                .global_blades
                .navigator_mut()
                .is_some_and(BladeNavigator::go_forward);
        } else {
            let _ = self
                .global_blades
                .navigator_mut()
                .is_some_and(BladeNavigator::go_back);
        }
    }

    pub(super) fn close_all_resource_details(
        &mut self,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        for cluster in self.clusters.values_mut() {
            cluster.resource_detail_panel = None;
        }
        if self.global_blades.navigator().is_some_and(|navigator| {
            navigator
                .entries()
                .any(|entry| entry.resource_detail().is_some())
        }) {
            Self::stop_discarded_blades(self.global_blades.clear(), commands_to_send);
        }
    }

    pub(super) fn toggle_namespace(
        &mut self,
        cluster_key: i32,
        namespace: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let (was_selected, api_resource) = {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };
            let was_selected = !cluster.selected_namespaces.insert(namespace.clone());
            if was_selected {
                cluster.selected_namespaces.remove(&namespace);
            }
            (was_selected, cluster.selected_api_resource.clone())
        };
        self.remember_selected_namespaces(cluster_key);
        if was_selected {
            return;
        }
        let Some(api_resource) = api_resource else {
            return;
        };
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            Self::request_resource_watch(cluster, &api_resource, Some(namespace), commands_to_send);
        }
    }

    /// Replace the visible namespace scope without cancelling existing watches.
    pub(super) fn replace_selected_namespaces<I>(
        &mut self,
        cluster_key: i32,
        namespaces: I,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) where
        I: IntoIterator<Item = String>,
    {
        {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };
            cluster.selected_namespaces = namespaces.into_iter().collect();
        }
        self.remember_selected_namespaces(cluster_key);
        let Some(api_resource) = self
            .clusters
            .get(&cluster_key)
            .and_then(|cluster| cluster.selected_api_resource.clone())
        else {
            return;
        };
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
        }
    }

    /// Select every discovered namespace without cancelling existing watches.
    pub(super) fn select_all_namespaces(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };
            cluster.selected_namespaces = cluster
                .namespaces
                .values()
                .map(|namespace| namespace.name.clone())
                .collect();
        }
        self.remember_selected_namespaces(cluster_key);
        let Some(api_resource) = self
            .clusters
            .get(&cluster_key)
            .and_then(|cluster| cluster.selected_api_resource.clone())
        else {
            return;
        };
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
        }
    }

    /// Clear the visible namespace scope without cancelling existing watches.
    pub(super) fn clear_selected_namespaces(&mut self, cluster_key: i32) {
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            cluster.selected_namespaces.clear();
        }
        self.remember_selected_namespaces(cluster_key);
    }

    pub(super) fn retry_selected_load(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<WorkerCommandBox>,
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
        commands_to_send: &mut Vec<WorkerCommandBox>,
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
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let key = (api_resource.clone(), namespace.clone());
        let watch = cluster.resource_cache.entry(key.clone()).or_default();
        if watch.is_synced || cluster.active_watchers.contains(&key) {
            return;
        }
        watch.error = None;
        cluster.active_watchers.insert(key);
        commands_to_send.push(Box::new(crate::worker::StartResourceWatch {
            cluster_key: cluster.cluster_key,
            api_resource: api_resource.clone(),
            namespace,
        }));
    }

    pub(super) fn settle_bulk_delete_target(
        &mut self,
        cluster_key: i32,
        bulk_delete_id: Option<u64>,
        api_resource: &ApiResource,
        resource_name: &str,
        namespace: &Option<String>,
        failure: Option<String>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        let Some(progress) = cluster.bulk_delete_progress.as_mut() else {
            return;
        };
        if bulk_delete_id != Some(progress.id) {
            return;
        }
        let Some(target) = progress.target_for(api_resource, resource_name, namespace) else {
            return;
        };

        let api_resource = progress.api_resource.clone();
        progress.remaining_targets.remove(&target);
        if let Some(failure) = failure {
            progress.failures.push((target, failure));
        } else if let Some(selection) = cluster.resource_selections.get_mut(&api_resource) {
            selection.remove(&target.uid);
        }

        if !progress.remaining_targets.is_empty() {
            return;
        }

        let failures = std::mem::take(&mut progress.failures);
        cluster.bulk_delete_progress = None;
        if failures.is_empty() {
            return;
        }
        let details = failures
            .iter()
            .map(|(target, error)| format!("{}: {error}", target.display_name()))
            .collect::<Vec<_>>()
            .join("\n");
        cluster.bulk_delete_error = Some(details);
    }

    pub(super) fn update<W: WorkerTrait>(&mut self, worker: &mut W) -> Vec<WorkerCommandBox> {
        let mut commands_to_send = Vec::new();
        while let Some(result) = worker.get_next_message() {
            result.apply_boxed(self, &mut commands_to_send);
        }
        commands_to_send
    }

    pub(super) fn apply_log_store_result(&mut self, result: LogStoreResult) {
        super::log_state::apply_store_result(&mut self.log_windows, result);
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

impl WorkerResult for crate::worker::ClusterConnectionFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.connection = ClusterConnectionState::Failed(self.error);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesClustersUpdated {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesClustersUpdated(clusters) = self;
        if ui.global_blades.navigator().is_some_and(|navigator| {
            navigator
                .entries()
                .any(|content| content.resource_detail().is_some())
        }) {
            UiState::stop_discarded_blades(ui.global_blades.clear(), commands);
        }
        ui.clusters.clear();
        ui.selected_cluster = None;
        let mut current_cluster_key = None;
        for cluster in clusters {
            ui.next_cluster_key += 1;
            let cluster_key = ui.next_cluster_key;
            if cluster.is_current {
                current_cluster_key = Some(cluster_key);
            }
            ui.clusters.insert(
                cluster_key,
                ClusterState {
                    cluster_key,
                    name: cluster.name,
                    namespaces: BTreeMap::new(),
                    connection: ClusterConnectionState::Disconnected,
                    namespaces_load: ClusterLoadState::Loading,
                    api_resources_load: ClusterLoadState::Loading,
                    selected_namespaces: HashSet::new(),
                    selected_api_resource: None,
                    resource_navigation: ResourceNavigation::default(),
                    custom_resource_columns: BTreeMap::new(),
                    scalable_api_resources: BTreeSet::new(),
                    resource_cache: HashMap::new(),
                    active_watchers: HashSet::new(),
                    resource_searches: HashMap::new(),
                    resource_selections: HashMap::new(),
                    next_bulk_delete_id: 0,
                    resource_detail_panel: None,
                    next_detail_generation: 0,
                    next_data_save_request_id: 0,
                    pending_delete: None,
                    pending_bulk_delete: None,
                    bulk_delete_progress: None,
                    bulk_delete_error: None,
                    pending_force_delete: None,
                    force_delete_error: None,
                    pending_deployment_restart: None,
                    deployment_restart_error: None,
                    pending_scale: None,
                    scale_error: None,
                },
            );
        }
        if let Some(cluster_key) = current_cluster_key
            && let Some(command) = ui.select_cluster(cluster_key)
        {
            commands.push(command);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesAdded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesNamespacesAdded {
            cluster_key,
            namespace,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster
                .namespaces
                .insert(SortedName::new(&namespace.name), namespace);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesDeleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesNamespacesDeleted {
            cluster_key,
            namespace_name,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.namespaces.remove(&SortedName::new(&namespace_name));
            cluster.selected_namespaces.remove(&namespace_name);
        }
        if let Some(context_name) = ui
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.name.clone())
            && let Some(selection) = ui.cluster_selections.selections.get_mut(&context_name)
        {
            selection.selected_namespaces.remove(&namespace_name);
            ui.prune_empty_cluster_selection(cluster_key);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesReplaced {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesNamespacesReplaced {
            cluster_key,
            namespaces,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.namespaces = namespaces
                .into_iter()
                .map(|namespace| (SortedName::new(&namespace.name), namespace))
                .collect();
            cluster.namespaces_load = ClusterLoadState::Ready;
        }
        ui.restore_selected_namespaces(cluster_key, _commands);
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesLoadFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesNamespacesLoadFailed { cluster_key, error } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.namespaces_load = ClusterLoadState::Failed(error);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesApisLoaded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesApisLoaded {
            cluster_key,
            api_resources,
            scalable_api_resources,
        } = self;
        let restored_api_resource = ui.restored_api_resource(cluster_key, &api_resources);
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.resource_navigation = build_resource_navigation(api_resources);
            cluster.scalable_api_resources = scalable_api_resources;
            cluster.api_resources_load = ClusterLoadState::Ready;
        }
        if let Some(api_resource) = restored_api_resource {
            ui.select_api_resource(cluster_key, api_resource, _commands);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesCustomResourceColumnsLoaded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesCustomResourceColumnsLoaded {
            cluster_key,
            columns,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.custom_resource_columns.extend(columns);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesResourceSchemasLoaded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceSchemasLoaded {
            cluster_key,
            schemas,
        } = self;
        for (api_resource, schema) in schemas {
            ui.resource_schemas
                .insert((cluster_key, api_resource.clone()), schema.clone());
            for editor in ui.yaml_editors.values_mut().filter(|editor| {
                editor.cluster_key == cluster_key && editor.api_resource == api_resource
            }) {
                editor.schema = Some(schema.clone());
                editor.schema_loading = false;
                editor.validation_revision = 0;
            }
        }
    }
}

impl WorkerResult for crate::worker::KubernetesApisLoadFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesApisLoadFailed { cluster_key, error } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.api_resources_load = ClusterLoadState::Failed(error);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesClusterConnectionCreated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesClusterConnectionCreated { cluster_key } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.connection = ClusterConnectionState::Connected;
        }
    }
}
impl WorkerResult for crate::worker::KubernetesResourceAdded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceAdded {
            cluster_key,
            api_resource,
            namespace,
            resource,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster
                .resource_cache
                .entry((api_resource, namespace))
                .or_default()
                .resources
                .insert(resource.uid.clone(), resource);
        }
    }
}
impl WorkerResult for crate::worker::KubernetesResourceDeleted {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceDeleted {
            cluster_key,
            api_resource,
            namespace,
            resource_uid,
        } = self;
        let deleted_history_entry_id = ui.global_blades.navigator().and_then(|navigator| {
            navigator
                .entries()
                .filter_map(|content| content.resource_detail())
                .find(|entry| {
                    entry.cluster_key == cluster_key && entry.resource_uid == resource_uid
                })
                .map(|entry| entry.history_entry_id)
        });
        let closes_active_blade = deleted_history_entry_id.is_some_and(|history_entry_id| {
            ui.global_blades.navigator().is_some_and(|navigator| {
                navigator
                    .current()
                    .resource_detail()
                    .is_some_and(|entry| entry.history_entry_id == history_entry_id)
                    || navigator
                        .current()
                        .is_owned_by_resource_detail(history_entry_id)
            })
        });
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            if let Some(watch) = cluster
                .resource_cache
                .get_mut(&(api_resource.clone(), namespace.clone()))
            {
                watch.resources.remove(&resource_uid);
            }
            if closes_active_blade {
                cluster.resource_detail_panel = None;
            }
        }
        if closes_active_blade {
            UiState::stop_discarded_blades(ui.global_blades.clear(), commands);
        }
    }
}
impl WorkerResult for crate::worker::KubernetesResourcesReplaced {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourcesReplaced {
            cluster_key,
            api_resource,
            namespace,
            resources,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            let watch = cluster
                .resource_cache
                .entry((api_resource.clone(), namespace))
                .or_default();
            watch.resources = resources
                .into_iter()
                .map(|resource| (resource.uid.clone(), resource))
                .collect();
            watch.is_synced = true;
            watch.error = None;
            let visible_uids = cluster
                .resource_cache
                .iter()
                .filter(|((cached_resource, cached_namespace), _)| {
                    cached_resource == &api_resource
                        && (!api_resource.namespaced
                            || cached_namespace.as_ref().is_some_and(|namespace| {
                                cluster.selected_namespaces.contains(namespace)
                            }))
                })
                .flat_map(|(_, watch)| watch.resources.keys().cloned())
                .collect::<HashSet<_>>();
            if let Some(selection) = cluster.resource_selections.get_mut(&api_resource) {
                selection.retain(|uid| visible_uids.contains(uid));
            }
        }
    }
}
impl WorkerResult for crate::worker::KubernetesResourceWatchStarted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceWatchStarted {
            cluster_key,
            api_resource,
            namespace,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.active_watchers.insert((api_resource, namespace));
        }
    }
}
impl WorkerResult for crate::worker::KubernetesResourceWatchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceWatchFailed {
            cluster_key,
            api_resource,
            namespace,
            error,
        } = self;
        ui.resource_watch_failed(cluster_key, api_resource, namespace, error);
    }
}
impl WorkerResult for crate::worker::ResourceDetailUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ResourceDetailUpdated {
            cluster_key,
            history_entry_id,
            detail,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            sync_resource_data_editor(&mut entry.data_editor, &detail);
            entry.detail = Some(*detail);
            entry.detail_error = None;
        }
    }
}
impl WorkerResult for crate::worker::ResourceEventsReplaced {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ResourceEventsReplaced {
            cluster_key,
            history_entry_id,
            events,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            entry.events = events;
            entry.events_error = None;
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailWatchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ResourceDetailWatchFailed {
            cluster_key,
            history_entry_id,
            events,
            error,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            if events {
                entry.events_error = Some(error);
            } else {
                entry.detail_error = Some(error);
            }
        }
    }
}
impl WorkerResult for crate::worker::ResourceDetailDeleted {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ResourceDetailDeleted {
            cluster_key,
            history_entry_id,
        } = self;
        let closes_active_blade = ui.global_blades.navigator().is_some_and(|navigator| {
            navigator.current().resource_detail().is_some_and(|entry| {
                entry.cluster_key == cluster_key && entry.history_entry_id == history_entry_id
            }) || navigator
                .current()
                .is_owned_by_resource_detail(history_entry_id)
        });
        if closes_active_blade {
            if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
                cluster.resource_detail_panel = None;
            }
            UiState::stop_discarded_blades(ui.global_blades.clear(), commands);
        } else if let Some(navigator) = ui.global_blades.navigator_mut() {
            navigator.back_stack_mut().retain(|entry| {
                entry.resource_detail().is_none_or(|entry| {
                    entry.cluster_key != cluster_key || entry.history_entry_id != history_entry_id
                }) && !entry.is_owned_by_resource_detail(history_entry_id)
            });
            navigator.forward_stack_mut().retain(|entry| {
                entry.resource_detail().is_none_or(|entry| {
                    entry.cluster_key != cluster_key || entry.history_entry_id != history_entry_id
                }) && !entry.is_owned_by_resource_detail(history_entry_id)
            });
            stop_resource_detail_watches(cluster_key, [history_entry_id], commands);
        }
    }
}
impl WorkerResult for crate::worker::ManagedResourcesReplaced {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ManagedResourcesReplaced {
            cluster_key,
            history_entry_id,
            resources,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            entry.managed_resources = resources;
            entry.managed_resources_error = None;
        }
    }
}
impl WorkerResult for crate::worker::ManagedResourcesWatchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ManagedResourcesWatchFailed {
            cluster_key,
            history_entry_id,
            error,
        } = self;
        if let Some(entry) = ui
            .resource_detail_entry_mut(history_entry_id)
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            entry.managed_resources_error = Some(error);
        }
    }
}
fn sync_resource_data_editor(
    data_editor: &mut Option<ResourceDataEditorState>,
    detail: &ResourceDetail,
) {
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
        ResourceDetailPayload::Generic
        | ResourceDetailPayload::Pod(_)
        | ResourceDetailPayload::Node(_) => None,
    };
    match (data_editor.as_mut(), values) {
        (Some(editor), Some(values)) => {
            editor.accept_watched_values(values, detail.resource_version.clone())
        }
        (None, Some(values)) => {
            *data_editor = Some(ResourceDataEditorState::new(
                values,
                detail.resource_version.clone(),
            ))
        }
        (_, None) => *data_editor = None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::log_state::rebase_display_row;
    use super::*;
    use crate::api_resource::ApiResource;
    use crate::cluster_connection_manager::Cluster;
    use crate::log_store::LogPageRow;
    use crate::minimal_resource::MinimalResource;
    use crate::resource_table::ContainerKind;
    use crate::worker::*;
    use std::collections::VecDeque;

    #[test]
    fn pod_log_windows_route_each_stream_by_its_window_id() {
        let mut state = UiState::default();
        let mut commands = Vec::new();
        state.open_pod_log_window(
            7,
            "api-pod".into(),
            Some("default".into()),
            PodLogContainer {
                name: "api".into(),
                kind: ContainerKind::App,
                image: None,
            },
            &mut commands,
        );
        state.open_pod_log_window(
            7,
            "api-pod".into(),
            Some("default".into()),
            PodLogContainer {
                name: "sidecar".into(),
                kind: ContainerKind::App,
                image: None,
            },
            &mut commands,
        );

        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0]
                .as_ref()
                .as_any()
                .downcast_ref::<StartPodLogStream>()
                .map(|command| command.log_window_id),
            Some(1)
        );
        assert_eq!(
            commands[1]
                .as_ref()
                .as_any()
                .downcast_ref::<StartPodLogStream>()
                .map(|command| command.log_window_id),
            Some(2)
        );

        let mut worker = MockWorker {
            results: VecDeque::from([
                Box::new(PodLogStreamStarted { log_window_id: 1 }) as WorkerResultBox,
                Box::new(PodLogStreamStarted { log_window_id: 2 }) as WorkerResultBox,
                Box::new(PodLogStreamEnded { log_window_id: 1 }) as WorkerResultBox,
            ]),
            commands: Vec::new(),
        };
        let _ = state.update(&mut worker);
        state.apply_log_store_result(LogStoreResult::Updated {
            window_id: 2,
            total_lines: 1,
            completed_search: None,
            appended_rows: Vec::new(),
            backfill_lines: None,
        });
        state.apply_log_store_result(LogStoreResult::Updated {
            window_id: 1,
            total_lines: 2,
            completed_search: None,
            appended_rows: Vec::new(),
            backfill_lines: None,
        });

        assert_eq!(state.log_windows[&1].total_lines, 2);
        assert_eq!(state.log_windows[&1].status, PodLogStatus::Finished);
        assert_eq!(state.log_windows[&2].total_lines, 1);
        assert_eq!(state.log_windows[&2].status, PodLogStatus::Following);
    }

    #[test]
    fn cluster_reload_ignores_resource_events_from_the_retired_cluster_key() {
        let api_resource = ApiResource {
            group: "core".into(),
            version: "v1".into(),
            kind: "Pod".into(),
            name: "pods".into(),
            namespaced: true,
        };
        let stale_resource = MinimalResource {
            uid: "stale".into(),
            name: "stale-pod".into(),
            namespace: Some("default".into()),
            creation_timestamp: None,
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        };
        let mut state = UiState::default();
        let mut worker = MockWorker {
            results: VecDeque::from([
                Box::new(KubernetesClustersUpdated(vec![Cluster {
                    name: "old".into(),
                    is_current: true,
                }])) as WorkerResultBox,
                Box::new(KubernetesClustersUpdated(vec![Cluster {
                    name: "new".into(),
                    is_current: true,
                }])) as WorkerResultBox,
                Box::new(KubernetesResourceAdded {
                    cluster_key: 1,
                    api_resource,
                    namespace: Some("default".into()),
                    resource: stale_resource,
                }) as WorkerResultBox,
            ]),
            commands: Vec::new(),
        };

        let _ = state.update(&mut worker);

        assert_eq!(state.selected_cluster, Some(2));
        assert_eq!(state.clusters.len(), 1);
        assert_eq!(state.clusters[&2].name, "new");
        assert!(state.clusters[&2].resource_cache.is_empty());
    }

    #[test]
    fn yaml_editors_are_deduplicated_and_route_results_by_editor_id() {
        let ctx = egui::Context::default();
        let api_resource = ApiResource {
            group: "core".into(),
            version: "v1".into(),
            kind: "ConfigMap".into(),
            name: "configmaps".into(),
            namespaced: true,
        };
        let mut state = UiState::default();
        let mut commands = Vec::new();

        state.open_yaml_editor(
            &ctx,
            7,
            api_resource.clone(),
            Some("default".into()),
            "settings".into(),
            &mut commands,
        );
        state.open_yaml_editor(
            &ctx,
            7,
            api_resource.clone(),
            Some("default".into()),
            "settings".into(),
            &mut commands,
        );
        state.open_yaml_editor(
            &ctx,
            7,
            api_resource.clone(),
            Some("default".into()),
            "other-settings".into(),
            &mut commands,
        );

        assert_eq!(commands.len(), 4);
        assert_eq!(
            commands[0]
                .as_ref()
                .as_any()
                .downcast_ref::<GetResourceYaml>()
                .map(|command| command.editor_id),
            Some(1)
        );
        assert_eq!(
            commands[1]
                .as_ref()
                .as_any()
                .downcast_ref::<LoadResourceSchema>()
                .map(|command| command.editor_id),
            Some(1)
        );
        assert_eq!(
            commands[2]
                .as_ref()
                .as_any()
                .downcast_ref::<GetResourceYaml>()
                .map(|command| command.editor_id),
            Some(2)
        );
        assert_eq!(
            commands[3]
                .as_ref()
                .as_any()
                .downcast_ref::<LoadResourceSchema>()
                .map(|command| command.editor_id),
            Some(2)
        );
        assert!(state.yaml_editors[&1].focus_requested);

        let mut worker = MockWorker {
            results: VecDeque::from([
                Box::new(ResourceYamlFetched {
                    editor_id: 2,
                    cluster_key: 7,
                    api_resource: api_resource.clone(),
                    namespace: Some("default".into()),
                    resource_name: "other-settings".into(),
                    yaml: "kind: ConfigMap\nmetadata:\n  name: other-settings".into(),
                }) as WorkerResultBox,
                Box::new(ResourceYamlFetched {
                    editor_id: 1,
                    cluster_key: 7,
                    api_resource: api_resource.clone(),
                    namespace: Some("default".into()),
                    resource_name: "settings".into(),
                    yaml: "kind: ConfigMap\nmetadata:\n  name: settings".into(),
                }) as WorkerResultBox,
            ]),
            commands: Vec::new(),
        };
        state.update(&mut worker);

        assert_eq!(state.yaml_editors[&1].resource_name, "settings");
        assert_eq!(state.yaml_editors[&2].resource_name, "other-settings");
        assert!(
            state
                .yaml_editors
                .values()
                .all(|editor| !editor.loading && editor.original_yaml.is_some())
        );
    }

    #[test]
    fn resource_data_completion_updates_only_the_initiating_history_entry() {
        let config_maps = ApiResource {
            group: "core".into(),
            version: "v1".into(),
            kind: "ConfigMap".into(),
            name: "configmaps".into(),
            namespaced: true,
        };
        let secrets = ApiResource {
            group: "core".into(),
            version: "v1".into(),
            kind: "Secret".into(),
            name: "secrets".into(),
            namespaced: true,
        };
        let mut state = UiState::default();
        let mut commands = Vec::new();
        let mut setup_worker = MockWorker {
            results: VecDeque::from([Box::new(KubernetesClustersUpdated(vec![Cluster {
                name: "kind".into(),
                is_current: true,
            }])) as WorkerResultBox]),
            commands: Vec::new(),
        };
        state.update(&mut setup_worker);
        state.open_resource_detail(
            1,
            config_maps.clone(),
            "settings".into(),
            Some("default".into()),
            "config-map-uid".into(),
            &mut commands,
        );
        state.navigate_resource_detail(
            1,
            secrets,
            "settings".into(),
            Some("default".into()),
            "secret-uid".into(),
            &mut commands,
        );
        let navigator = state
            .global_blades
            .navigator_mut()
            .expect("detail panel is open");
        let config_map_history_entry_id = navigator
            .entries()
            .filter_map(|entry| entry.resource_detail())
            .find(|entry| entry.api_resource == config_maps)
            .expect("ConfigMap history entry exists")
            .history_entry_id;
        for entry in navigator.entries_mut() {
            let entry = entry
                .resource_detail_mut()
                .expect("the test only creates resource detail content");
            entry.data_editor = Some(ResourceDataEditorState {
                saving: true,
                pending_save_request_id: Some(2),
                ..ResourceDataEditorState::new(BTreeMap::new(), "1".into())
            });
        }

        let mut worker = MockWorker {
            results: VecDeque::from([
                Box::new(ResourceDataUpdateCompleted {
                    cluster_key: 1,
                    history_entry_id: config_map_history_entry_id,
                    request_id: 999,
                }) as WorkerResultBox,
                Box::new(ResourceDataUpdateCompleted {
                    cluster_key: 1,
                    history_entry_id: config_map_history_entry_id,
                    request_id: 2,
                }) as WorkerResultBox,
            ]),
            commands: Vec::new(),
        };
        state.update(&mut worker);

        let navigator = state
            .global_blades
            .navigator()
            .expect("detail panel is open");
        let config_map_editor = navigator
            .entries()
            .filter_map(|entry| entry.resource_detail())
            .find(|entry| entry.history_entry_id == config_map_history_entry_id)
            .and_then(|entry| entry.data_editor.as_ref())
            .expect("config map editor exists");
        assert!(!config_map_editor.saving);
        assert!(
            navigator
                .current()
                .resource_detail()
                .and_then(|entry| entry.data_editor.as_ref())
                .expect("secret editor exists")
                .saving
        );

        for entry in state
            .global_blades
            .navigator_mut()
            .expect("detail panel is open")
            .entries_mut()
        {
            let editor = entry
                .resource_detail_mut()
                .expect("the test only creates resource detail content")
                .data_editor
                .as_mut()
                .expect("editor exists");
            editor.saving = true;
            editor.pending_save_request_id = Some(3);
            editor.save_error = None;
        }
        let mut worker = MockWorker {
            results: VecDeque::from([Box::new(ResourceDataUpdateFailed {
                cluster_key: 1,
                history_entry_id: config_map_history_entry_id,
                request_id: 3,
                error: "stale update failed".into(),
            }) as WorkerResultBox]),
            commands: Vec::new(),
        };
        state.update(&mut worker);

        assert_eq!(
            state
                .global_blades
                .navigator()
                .expect("detail panel is open")
                .entries()
                .filter_map(|entry| entry.resource_detail())
                .find(|entry| entry.history_entry_id == config_map_history_entry_id)
                .and_then(|entry| entry.data_editor.as_ref())
                .and_then(|editor| editor.save_error.as_deref()),
            Some("stale update failed")
        );
        assert_eq!(
            state
                .global_blades
                .navigator()
                .expect("detail panel is open")
                .current()
                .resource_detail()
                .and_then(|entry| entry.data_editor.as_ref())
                .expect("secret editor exists")
                .save_error,
            None
        );
    }

    #[test]
    fn api_status_causes_become_editor_diagnostics_for_validation_and_apply() {
        let ctx = egui::Context::default();
        let api_resource = ApiResource {
            group: "apps".into(),
            version: "v1".into(),
            kind: "Deployment".into(),
            name: "deployments".into(),
            namespaced: true,
        };
        let yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: api\nspec:\n  template:\n    spec:\n      containers:\n        - name: api\n          image: invalid";
        let api_error = ResourceApiError {
            message: "Deployment.apps \"api\" is invalid".into(),
            causes: vec![crate::worker::ResourceApiErrorCause {
                field: "spec.template.spec.containers[0].image".into(),
                message: "Invalid value: \"invalid\"".into(),
                reason: "FieldValueInvalid".into(),
            }],
        };
        let mut state = UiState::default();
        let mut commands = Vec::new();
        state.open_yaml_editor(
            &ctx,
            7,
            api_resource.clone(),
            Some("default".into()),
            "api".into(),
            &mut commands,
        );
        let mut worker = MockWorker {
            results: VecDeque::from([
                Box::new(ResourceYamlFetched {
                    editor_id: 1,
                    cluster_key: 7,
                    api_resource: api_resource.clone(),
                    namespace: Some("default".into()),
                    resource_name: "api".into(),
                    yaml: yaml.into(),
                }) as WorkerResultBox,
                Box::new(ResourceYamlValidationFailed {
                    editor_id: 1,
                    revision: 0,
                    cluster_key: 7,
                    api_resource: api_resource.clone(),
                    namespace: Some("default".into()),
                    resource_name: "api".into(),
                    error: api_error.clone(),
                }) as WorkerResultBox,
                Box::new(ResourceApplyFailed {
                    editor_id: 1,
                    cluster_key: 7,
                    api_resource,
                    namespace: Some("default".into()),
                    resource_name: "api".into(),
                    error: api_error,
                }) as WorkerResultBox,
            ]),
            commands: Vec::new(),
        };

        state.update(&mut worker);

        let editor = &state.yaml_editors[&1];
        assert_eq!(
            editor.server_validation,
            ValidationState::Failed("Deployment.apps \"api\" is invalid".into())
        );
        assert_eq!(
            editor.error.as_deref(),
            Some("Deployment.apps \"api\" is invalid")
        );
        assert_eq!(editor.diagnostics.len(), 1);
        assert_eq!(editor.diagnostics[0].line, Some(10));
        assert!(editor.diagnostics[0].range.is_some());
        assert_eq!(
            editor.diagnostics[0].message,
            "spec.template.spec.containers[0].image: Invalid value: \"invalid\""
        );
    }

    #[test]
    fn ignores_stale_pages_and_evicts_pages_using_the_injected_cache_limit() {
        let mut state = UiState::default();
        let mut commands = Vec::new();
        state.open_pod_log_window(
            7,
            "api-pod".into(),
            Some("default".into()),
            PodLogContainer {
                name: "api".into(),
                kind: ContainerKind::App,
                image: None,
            },
            &mut commands,
        );
        let window = state.log_windows.get_mut(&1).expect("log window exists");
        window.page_cache_limit = 64;

        state.apply_log_store_result(LogStoreResult::PageLoaded {
            window_id: 1,
            generation: 1,
            filter_matches: false,
            page_start: 0,
            total_rows: 1,
            rows: vec![test_log_row(0, "stale page must be ignored")],
        });
        assert!(state.log_windows[&1].pages.is_empty());

        for page_start in [0, 1] {
            state.apply_log_store_result(LogStoreResult::PageLoaded {
                window_id: 1,
                generation: 0,
                filter_matches: false,
                page_start,
                total_rows: 2,
                rows: vec![test_log_row(page_start, &"x".repeat(64))],
            });
        }

        let window = &state.log_windows[&1];
        assert!(!window.pages.contains_key(&LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: 0,
        }));
        assert!(window.pages.contains_key(&LogPageKey {
            generation: 0,
            filter_matches: false,
            page_start: 1,
        }));
    }

    #[test]
    fn live_tail_rows_bridge_disk_pages_only_while_following_bottom() {
        let mut state = UiState::default();
        let mut commands = Vec::new();
        state.open_pod_log_window(
            7,
            "api-pod".into(),
            Some("default".into()),
            PodLogContainer {
                name: "api".into(),
                kind: ContainerKind::App,
                image: None,
            },
            &mut commands,
        );

        let tail_row = |display_row, text: &str| LogPageRow {
            display_row,
            line_index: display_row,
            timestamp: None,
            text: text.to_owned(),
            style_spans: Vec::new(),
            match_ranges: Vec::new(),
        };
        state.apply_log_store_result(LogStoreResult::Updated {
            window_id: 1,
            total_lines: 1,
            completed_search: None,
            appended_rows: vec![tail_row(0, "live now")],
            backfill_lines: Some(12_345),
        });
        let window = &state.log_windows[&1];
        assert_eq!(window.backfill_lines, Some(12_345));
        assert_eq!(window.live_rows[&0].text, "live now");

        state.log_windows.get_mut(&1).unwrap().following_bottom = false;
        state.apply_log_store_result(LogStoreResult::Updated {
            window_id: 1,
            total_lines: 2,
            completed_search: None,
            appended_rows: vec![tail_row(1, "wait for disk")],
            backfill_lines: None,
        });
        let window = &state.log_windows[&1];
        assert_eq!(window.total_lines, 2);
        assert!(!window.live_rows.contains_key(&1));

        state.apply_log_store_result(LogStoreResult::PageLoaded {
            window_id: 1,
            generation: 0,
            filter_matches: false,
            page_start: 0,
            total_rows: 2,
            rows: vec![tail_row(0, "live now"), tail_row(1, "wait for disk")],
        });
        assert!(state.log_windows[&1].live_rows.is_empty());
    }

    #[test]
    fn log_store_reducer_applies_only_current_async_results() {
        let mut state = UiState::default();
        let mut commands = Vec::new();
        state.open_pod_log_window(
            7,
            "api-pod".into(),
            Some("default".into()),
            PodLogContainer {
                name: "api".into(),
                kind: ContainerKind::App,
                image: None,
            },
            &mut commands,
        );
        let window = state.log_windows.get_mut(&1).expect("log window exists");
        window.search.generation = 3;
        window.selection_generation = 2;

        state.apply_log_store_result(LogStoreResult::SearchProgress {
            window_id: 1,
            generation: 2,
            scanned_lines: 10,
            total_lines: 20,
            match_count: 4,
        });
        assert_eq!(state.log_windows[&1].total_lines, 0);

        state.apply_log_store_result(LogStoreResult::SearchProgress {
            window_id: 1,
            generation: 3,
            scanned_lines: 10,
            total_lines: 20,
            match_count: 4,
        });
        state.apply_log_store_result(LogStoreResult::SearchCompleted {
            window_id: 1,
            generation: 3,
            match_count: 5,
        });
        state.apply_log_store_result(LogStoreResult::Copied {
            window_id: 1,
            selection_generation: 3,
            text: "stale copy".into(),
        });
        state.apply_log_store_result(LogStoreResult::Copied {
            window_id: 1,
            selection_generation: 4,
            text: "current copy".into(),
        });

        let window = &state.log_windows[&1];
        assert_eq!(window.total_lines, 20);
        assert_eq!(window.search.scanned_lines, 20);
        assert_eq!(window.search.match_count, 5);
        assert!(window.search.search_complete);
        assert_eq!(window.copied_text.as_deref(), Some("current copy"));

        state.apply_log_store_result(LogStoreResult::Failed {
            window_id: 1,
            error: "disk full".into(),
        });
        assert_eq!(
            state.log_windows[&1].status,
            PodLogStatus::Failed("Log storage failed: disk full".into())
        );
    }

    #[test]
    fn changing_a_log_selection_rejects_an_in_flight_copy_for_its_old_range() {
        let mut state = UiState::default();
        let mut commands = Vec::new();
        state.open_pod_log_window(
            7,
            "api-pod".into(),
            Some("default".into()),
            PodLogContainer {
                name: "api".into(),
                kind: ContainerKind::App,
                image: None,
            },
            &mut commands,
        );
        let window = state.log_windows.get_mut(&1).expect("log window exists");
        let selection_start = LogTextPosition {
            display_row: 0,
            byte_offset: 0,
        };
        window.set_selection(Some(LogTextSelection {
            anchor: selection_start,
            focus: selection_start,
        }));
        let old_generation = window.selection_generation;
        window.set_selection(Some(LogTextSelection {
            anchor: selection_start,
            focus: LogTextPosition {
                display_row: 0,
                byte_offset: 8,
            },
        }));
        let current_generation = window.selection_generation;

        state.apply_log_store_result(LogStoreResult::Copied {
            window_id: 1,
            selection_generation: old_generation,
            text: "old range".into(),
        });
        assert!(state.log_windows[&1].copied_text.is_none());

        state.apply_log_store_result(LogStoreResult::Copied {
            window_id: 1,
            selection_generation: current_generation,
            text: "current range".into(),
        });
        assert_eq!(
            state.log_windows[&1].copied_text.as_deref(),
            Some("current range")
        );
    }

    #[test]
    fn rebasing_maps_an_overlapping_tail_row_to_its_history_position() {
        assert_eq!(rebase_display_row(40, 200, 100), 140);
    }

    #[test]
    fn rebasing_maps_live_records_after_the_overlap_without_an_extra_shift() {
        assert_eq!(rebase_display_row(120, 200, 100), 220);
    }

    #[test]
    fn rebasing_without_overlap_places_the_live_segment_after_all_history() {
        assert_eq!(rebase_display_row(40, 100, 0), 140);
    }

    #[test]
    fn resolved_matches_scroll_in_source_or_filtered_display_row_space() {
        let mut state = UiState::default();
        let mut commands = Vec::new();
        state.open_pod_log_window(
            7,
            "api-pod".into(),
            Some("default".into()),
            PodLogContainer {
                name: "api".into(),
                kind: ContainerKind::App,
                image: None,
            },
            &mut commands,
        );
        let window = state.log_windows.get_mut(&1).expect("log window exists");
        window.search.generation = 3;
        window.search.active_match = Some(4);

        state.apply_log_store_result(LogStoreResult::MatchResolved {
            window_id: 1,
            generation: 3,
            match_row: 4,
            line_index: 400,
        });
        assert_eq!(state.log_windows[&1].search.active_display_row, Some(400));
        assert_eq!(
            state.log_windows[&1].search.scroll_to_display_row,
            Some(400)
        );

        let window = state.log_windows.get_mut(&1).expect("log window exists");
        window.search.filter_matches = true;
        window.search.active_match = Some(5);
        state.apply_log_store_result(LogStoreResult::MatchResolved {
            window_id: 1,
            generation: 3,
            match_row: 5,
            line_index: 400,
        });
        assert_eq!(state.log_windows[&1].search.active_display_row, Some(5));
        assert_eq!(state.log_windows[&1].search.scroll_to_display_row, Some(5));
    }

    fn test_log_row(display_row: usize, text: &str) -> LogPageRow {
        LogPageRow {
            display_row,
            line_index: display_row,
            timestamp: None,
            text: text.to_owned(),
            style_spans: Vec::new(),
            match_ranges: Vec::new(),
        }
    }
}
