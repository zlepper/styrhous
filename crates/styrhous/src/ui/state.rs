use super::global_blade::{GlobalBladeContent, GlobalBladeCoordinator};
use crate::api_resource::{ApiResource, ApiResourceSet};
use crate::cluster_connection_manager::{
    AvailableAksCluster, AvailableEksCluster, AvailableTailscaleCluster, AwsAccount, AwsProfile,
    Cluster, ClusterDiscoveryTools,
};
use crate::helm_release::{HelmRelease, HelmReleaseKey};
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
    PendingLogSources, PodLogStatus, PodLogWindowState,
};

pub(super) use super::persistence::{
    PersistedApiDiscovery, PersistedApiResource, PersistedClusterSelection,
    PersistedClusterSelections, PinnedKinds, ResourceNavigationExpansion,
};

const DELETE_CONFIRMATION_DELAY: Duration = Duration::from_secs(3);

#[derive(Default)]
pub(crate) struct UiState {
    pub(super) clusters: HashMap<i32, ClusterState>,
    pub(super) next_cluster_key: i32,
    pub(super) selected_cluster: Option<i32>,
    pub(super) log_windows: BTreeMap<u64, PodLogWindowState>,
    pub(super) next_log_window_id: u64,
    /// Exports still being written for windows that have closed, keyed by
    /// (window id, export id), so their outcome is still recorded.
    pub(super) orphaned_log_exports: HashMap<(u64, u64), i32>,
    pub(super) pending_log_sources: Option<PendingLogSources>,
    pub(super) yaml_editors: BTreeMap<u64, YamlEditorWindowState>,
    pub(super) next_yaml_editor_id: u64,
    pub(super) resource_schemas: HashMap<(i32, ApiResource), ResourceSchema>,
    pub(super) log_display_options: LogDisplayOptions,
    pub(super) global_blades: GlobalBladeCoordinator,
    pub(super) terminal_launch_error: Option<String>,
    pub(super) cluster_selections: PersistedClusterSelections,
    pub(super) api_discovery_cache: PersistedApiDiscovery,
    pub(super) resource_navigation_expansion: ResourceNavigationExpansion,
    pub(super) next_operation_toast_sequence: u64,
    /// Toasts about the application rather than one cluster, such as an
    /// invalid theme file.
    pub(super) application_toasts: Vec<TransientOperationToast>,
    pub(super) next_scale_request_id: u64,
    pub(super) awaited_scale_lookup: Option<AwaitedScaleLookup>,
    pub(super) managed_cluster_discovery: ManagedClusterDiscoveryState,
    pub(super) context_presentation_saves: HashMap<String, ContextPresentationSave>,
    /// The time of the frame being processed, in seconds of egui input time.
    pub(super) now: f64,
    /// The wall clock ages and relative times are measured against.
    pub(crate) clock: super::clock::WallClock,
    /// The kinds pinned to the top of the sidebar.
    pub(super) pinned_kinds: PinnedKinds,
    /// What the sidebar's kind filter holds.
    pub(super) kind_filter: String,
    /// The pinned and recently visited clusters and the node counts last seen.
    /// Where the kubeconfig contexts come from ("~/.kube/config"); empty until known.
    pub(super) kubeconfig_source: String,
    /// The cluster being added from the switcher is switched to once it is in.
    pub(super) switch_to_imported: bool,
    pub(super) cluster_roster: super::cluster_roster::ClusterRoster,
    /// The open cluster switcher (A16), if any.
    pub(super) cluster_switcher: Option<super::cluster_switcher::SwitcherSession>,
    /// The licence warning the banner was dismissed for this session; it shows
    /// again when the warning changes.
    pub(super) dismissed_license_warning: Option<String>,
}

#[derive(Debug)]
pub(super) struct ContextPresentationSave {
    pub(super) request_id: uuid::Uuid,
    pub(super) presentation: crate::context_presentation::ContextPresentation,
    pub(super) result: Option<Result<(), String>>,
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
    pub(super) aws: AwsDiscoveryState,
}

#[derive(Debug, Default)]
pub(super) struct AwsDiscoveryState {
    pub(super) request_id: u64,
    pub(super) cli_installed: bool,
    pub(super) accounts: Vec<AwsAccount>,
    pub(super) account_loading: bool,
    pub(super) account_warning: Option<String>,
    pub(super) account_error: Option<String>,
    pub(super) selected_account_id: Option<String>,
    pub(super) selected_profile: Option<AwsProfile>,
    pub(super) scan_loading: bool,
    pub(super) clusters: Vec<AvailableEksCluster>,
    pub(super) scan_warning: Option<String>,
    pub(super) scan_error: Option<String>,
    pub(super) import_error: Option<String>,
}

impl AwsDiscoveryState {
    pub(super) fn clear_scan(&mut self) {
        self.request_id += 1;
        self.selected_account_id = None;
        self.selected_profile = None;
        self.scan_loading = false;
        self.clusters.clear();
        self.scan_warning = None;
        self.scan_error = None;
        self.import_error = None;
    }

    pub(super) fn begin_scan(&mut self, account_id: String, profile: AwsProfile) {
        self.clear_scan();
        self.selected_account_id = Some(account_id);
        self.selected_profile = Some(profile);
        self.scan_loading = true;
    }
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
    Eks {
        account_id: String,
        region: String,
        name: String,
        profile: AwsProfile,
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
    pub(super) revision: u64,
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
    /// Only the rows that are doing badly show (the "Problems only" chip).
    pub(super) problems_only: bool,
    /// The groups of a grouped table the user collapsed.
    pub(super) collapsed_groups: std::collections::HashSet<String>,
    /// The problem row `[` and `]` last stepped to (its uid), which the
    /// next step goes on from.
    pub(super) problem_cursor: Option<String>,
    /// A row (its uid) to scroll into view the next time the table shows.
    pub(super) scroll_to: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) enum ClusterLoadState {
    #[default]
    Loading,
    Ready,
    Failed(String),
}

mod actions;
mod api_discovery;
mod blades;
mod clusters;
mod editors;
mod mutations;
mod notifications;
mod operations;
mod resource_data;
mod selected_namespaces;

pub(super) use actions::*;
pub(crate) use api_discovery::ApiDiscoveryState;
pub(super) use blades::*;
pub(super) use clusters::*;
pub(super) use editors::*;
pub(super) use mutations::*;
pub(super) use notifications::*;
pub(crate) use operations::InFlightOperations;
pub(super) use resource_data::*;
pub(super) use scale_lookup::AwaitedScaleLookup;
pub(crate) use selected_namespaces::{NamespaceScope, SelectedNamespaces};

mod cluster_operations;
mod cluster_results;
#[cfg(test)]
pub(crate) use cluster_results::FAILED_API_GROUPS_TITLE_SUFFIX;
mod deployment_rollback;
mod detail_results;
mod details;
mod metrics_results;
mod namespaces;
mod node_maintenance;
pub(super) use node_maintenance::*;
mod openings;
mod preferences;
mod resource_results;
mod runtime;
mod scale_lookup;

#[cfg(test)]
mod tests;
