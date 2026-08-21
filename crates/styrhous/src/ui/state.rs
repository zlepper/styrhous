use super::global_blade::{GlobalBladeContent, GlobalBladeCoordinator};
use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    AvailableAksCluster, AvailableTailscaleCluster, Cluster, ClusterDiscoveryTools,
};
use crate::helm_release::HelmRelease;
use crate::log_store::LogStoreResult;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::{MinimalResource, PodLogContainer};
use crate::pod_metrics::{NodeUsage, POD_USAGE_HISTORY_WINDOW, PodUsage};
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
use std::ops::Range;
use std::sync::Arc;
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
    pub(super) managed_cluster_discovery: ManagedClusterDiscoveryState,
}

#[derive(Debug, Default)]
pub(super) struct ManagedClusterDiscoveryState {
    pub(super) tools: ClusterDiscoveryTools,
    pub(super) aks_clusters: Vec<AvailableAksCluster>,
    pub(super) tailscale_clusters: Vec<AvailableTailscaleCluster>,
    pub(super) loading: bool,
    pub(super) importing: Option<ManagedClusterImport>,
    pub(super) error: Option<String>,
    pub(super) azure_error: Option<String>,
    pub(super) azure_warning: Option<String>,
    pub(super) tailscale_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ManagedClusterImport {
    Aks {
        subscription_id: String,
        resource_group: String,
        cluster_name: String,
    },
    Tailscale {
        host_name: String,
    },
}

/// Key for identifying a resource watcher (API resource + optional namespace).
pub(crate) type ResourceWatchKey = (ApiResource, Option<String>);

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
pub(crate) struct ResourceWatchState {
    pub(super) resources: BTreeMap<String, MinimalResource>,
    pub(super) is_synced: bool,
    pub(super) error: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct HelmReleaseWatchState {
    pub(super) releases: Vec<HelmRelease>,
    pub(super) is_synced: bool,
    pub(super) backend_errors: BTreeMap<&'static str, String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ResourceSearchState {
    pub(super) query: String,
    pub(super) regex_mode: bool,
}

#[derive(Debug, Default)]
pub(crate) enum ClusterLoadState {
    #[default]
    Loading,
    Ready,
    Failed(String),
}

mod actions;
mod blades;
mod clusters;
mod editors;
mod mutations;
mod resource_data;

pub(super) use actions::*;
pub(super) use blades::*;
pub(super) use clusters::*;
pub(super) use editors::*;
pub(super) use mutations::*;
pub(super) use resource_data::*;

mod cluster_operations;
mod cluster_results;
mod detail_results;
mod details;
mod metrics_results;
mod namespaces;
mod openings;
mod preferences;
mod resource_results;
mod runtime;

#[cfg(test)]
mod tests;
