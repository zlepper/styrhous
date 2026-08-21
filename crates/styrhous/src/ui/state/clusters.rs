use super::*;

#[derive(Debug)]

pub(crate) struct ClusterState {
    pub(crate) name: String,
    pub(crate) cluster_key: i32,
    pub(crate) namespaces: BTreeMap<SortedName, MinimalNamespace>,
    pub(crate) connection: ClusterConnectionState,
    pub(crate) namespaces_load: ClusterLoadState,
    pub(crate) api_resources_load: ClusterLoadState,
    pub(crate) selected_namespaces: HashSet<String>,
    pub(crate) resources: ClusterResourceState,
}

/// UI state owned by a connected cluster's resource workspace.
///
/// Keeping this separate from connection identity and namespace discovery makes
/// it explicit which fields are replaced when a cluster session is re-opened.
/// `ClusterState` dereferences to this state for a gradual migration of feature
/// modules; new code should prefer `cluster.resources` when it needs to make
/// the ownership boundary visible.
#[derive(Debug)]
pub(crate) struct ClusterResourceState {
    pub(crate) resource_navigation: ResourceNavigation,
    pub(crate) custom_resource_columns: BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    pub(crate) scalable_api_resources: BTreeSet<ApiResource>,
    pub(crate) selected_api_resource: Option<ApiResource>,
    pub(crate) resource_cache: HashMap<ResourceWatchKey, ResourceWatchState>,
    pub(crate) helm_release_cache: HashMap<String, HelmReleaseWatchState>,
    pub(crate) active_watchers: HashSet<ResourceWatchKey>,
    pub(crate) pod_metrics_api_available: bool,
    pub(crate) pod_metrics: HashMap<String, PodMetricsNamespaceState>,
    pub(crate) active_pod_metrics: HashSet<String>,
    pub(crate) node_metrics_api_available: bool,
    pub(crate) node_metrics: NodeMetricsState,
    pub(crate) node_metrics_active: bool,
    pub(crate) resource_searches: HashMap<ApiResource, ResourceSearchState>,
    pub(crate) resource_selections: HashMap<ApiResource, HashSet<String>>,
    pub(crate) next_bulk_delete_id: u64,
    pub(crate) resource_detail_panel: Option<ResourceDetailPanelState>,
    pub(crate) next_detail_generation: u64,
    pub(crate) next_data_save_request_id: u64,
    pub(crate) pending_delete: Option<PendingDelete>,
    pub(crate) pending_bulk_delete: Option<PendingBulkDelete>,
    pub(crate) bulk_delete_progress: Option<BulkDeleteProgress>,
    pub(crate) bulk_delete_error: Option<String>,
    pub(crate) pending_force_delete: Option<PendingForceDelete>,
    pub(crate) force_delete_error: Option<String>,
    pub(crate) pending_deployment_restart: Option<PendingDeploymentRestart>,
    pub(crate) deployment_restart_error: Option<String>,
    pub(crate) pending_cron_job_run: Option<PendingCronJobRun>,
    pub(crate) cron_job_run_error: Option<String>,
    pub(crate) pending_scale: Option<PendingScale>,
    pub(crate) scale_error: Option<String>,
}

impl ClusterState {
    pub(crate) fn new(cluster_key: i32, name: String) -> Self {
        Self {
            name,
            cluster_key,
            namespaces: BTreeMap::new(),
            connection: ClusterConnectionState::Disconnected,
            namespaces_load: ClusterLoadState::Loading,
            api_resources_load: ClusterLoadState::Loading,
            selected_namespaces: HashSet::new(),
            resources: ClusterResourceState::new(false),
        }
    }

    /// Clear state derived from a previous connection before opening a new
    /// session. The worker cancels the old session's watchers, so no workspace,
    /// inspector, or pending resource action may survive into the new session.
    pub(crate) fn reset_for_connection(&mut self) {
        self.connection = ClusterConnectionState::Connecting;
        self.namespaces_load = ClusterLoadState::Loading;
        self.api_resources_load = ClusterLoadState::Loading;
        self.namespaces.clear();
        self.selected_namespaces.clear();
        self.resources.reset_for_connection();
    }

    #[cfg(test)]
    /// Construct an inert cluster whose optional UI state is empty.
    ///
    /// UI tests should layer only their scenario-specific state on top of this
    /// fixture so additions to `ClusterState` have one test default to update.
    pub(crate) fn for_test(cluster_key: i32, name: impl Into<String>) -> Self {
        let mut state = Self::new(cluster_key, name.into());
        state.namespaces_load = ClusterLoadState::Ready;
        state.api_resources_load = ClusterLoadState::Ready;
        state.resources.pod_metrics_api_available = true;
        state.resources.node_metrics_api_available = true;
        state
    }
}

impl ClusterResourceState {
    fn new(metrics_api_available: bool) -> Self {
        Self {
            resource_navigation: ResourceNavigation::default(),
            custom_resource_columns: BTreeMap::new(),
            scalable_api_resources: BTreeSet::new(),
            selected_api_resource: None,
            resource_cache: HashMap::new(),
            helm_release_cache: HashMap::new(),
            active_watchers: HashSet::new(),
            pod_metrics_api_available: metrics_api_available,
            pod_metrics: HashMap::new(),
            active_pod_metrics: HashSet::new(),
            node_metrics_api_available: metrics_api_available,
            node_metrics: NodeMetricsState::default(),
            node_metrics_active: false,
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
            pending_cron_job_run: None,
            cron_job_run_error: None,
            pending_scale: None,
            scale_error: None,
        }
    }

    fn reset_for_connection(&mut self) {
        *self = Self::new(false);
    }
}

impl std::ops::Deref for ClusterState {
    type Target = ClusterResourceState;

    fn deref(&self) -> &Self::Target {
        &self.resources
    }
}

impl std::ops::DerefMut for ClusterState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.resources
    }
}

#[derive(Debug, Default)]
pub(crate) struct PodMetricsNamespaceState {
    pub(crate) usages: BTreeMap<String, PodUsage>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct NodeMetricsState {
    pub(crate) usages: BTreeMap<String, NodeUsage>,
    pub(crate) error: Option<String>,
}

#[derive(Debug)]
pub(crate) enum ClusterConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Failed(String),
}
