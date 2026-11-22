use super::*;

#[derive(Debug)]

pub(crate) struct ClusterState {
    pub(crate) name: String,
    pub(crate) presentation: crate::context_presentation::ContextPresentation,
    pub(crate) cluster_key: i32,
    pub(crate) namespaces: BTreeMap<SortedName, MinimalNamespace>,
    pub(crate) connection: ClusterConnectionState,
    pub(crate) namespaces_load: ClusterLoadState,
    /// Set while the namespace watcher has been retrying past the grace
    /// period; the last known namespaces stay usable meanwhile.
    pub(crate) namespaces_reconnecting: Option<String>,
    pub(crate) api_resources_load: ClusterLoadState,
    pub(crate) selected_namespaces: SelectedNamespaces,
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
    /// Every discovered API resource, in discovery order, so recovered API groups can be
    /// merged in and the navigation rebuilt without disturbing the selection.
    pub(crate) discovered_api_resources: Vec<ApiResource>,
    pub(crate) resource_navigation: ResourceNavigation,
    pub(crate) custom_resource_columns: BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    pub(crate) scalable_api_resources: BTreeSet<ApiResource>,
    pub(crate) selected_api_resource: Option<ApiResource>,
    pub(crate) resource_cache: HashMap<ResourceWatchKey, ResourceWatchState>,
    pub(in crate::ui) resource_table_cache: super::super::resource_table_cache::ResourceTableCache,
    pub(crate) helm_release_cache: HashMap<String, HelmReleaseWatchState>,
    pub(crate) active_watchers: HashSet<ResourceWatchKey>,
    /// The namespaces each active all-namespaces watcher was started for. It
    /// filters events by this set, so it is restarted only when the set changes.
    pub(crate) all_namespaces_watch_scopes: HashMap<ApiResource, BTreeSet<String>>,
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
    pub(crate) pending_force_delete: Option<PendingForceDelete>,
    pub(crate) pending_deployment_restart: Option<PendingDeploymentRestart>,
    pub(crate) pending_cron_job_run: Option<PendingCronJobRun>,
    pub(crate) cron_job_runs: InFlightOperations,
    pub(crate) pending_helm_rollback: Option<PendingHelmRollback>,
    /// The release (namespace, name) each submitted rollback targets.
    pub(crate) helm_rollbacks: InFlightOperations<HelmReleaseKey>,
    pub(crate) pending_scale: Option<PendingScale>,
    /// Outcomes are intentionally session-scoped: rebuilding this resource
    /// state on reconnect drops entries from the previous cluster session.
    pub(crate) operation_history: Vec<OperationHistoryEntry>,
    /// Brief feedback for newly recorded outcomes. This is reset with the
    /// resource session so a previous connection cannot surface a stale toast.
    pub(crate) transient_operation_toasts: Vec<TransientOperationToast>,
}

impl ClusterState {
    pub(crate) fn new(cluster_key: i32, name: String) -> Self {
        Self {
            name,
            presentation: Default::default(),
            cluster_key,
            namespaces: BTreeMap::new(),
            connection: ClusterConnectionState::Disconnected,
            namespaces_load: ClusterLoadState::Loading,
            namespaces_reconnecting: None,
            api_resources_load: ClusterLoadState::Loading,
            selected_namespaces: SelectedNamespaces::default(),
            resources: ClusterResourceState::new(false),
        }
    }

    /// The selected namespaces resolved against the discovered ones.
    pub(crate) fn namespace_scope(&self) -> NamespaceScope<'_> {
        NamespaceScope::new(&self.selected_namespaces, &self.namespaces)
    }

    /// Whether a running watcher covers `namespace`, either on its own or as
    /// part of an all-namespaces watcher.
    pub(crate) fn is_namespace_watched(&self, api_resource: &ApiResource, namespace: &str) -> bool {
        self.active_watchers
            .contains(&(api_resource.clone(), Some(namespace.to_owned())))
            || (self.active_watchers.contains(&(api_resource.clone(), None))
                && self
                    .all_namespaces_watch_scopes
                    .get(api_resource)
                    .is_some_and(|scope| scope.contains(namespace)))
    }

    /// Clear state derived from a previous connection before opening a new
    /// session. The worker cancels the old session's watchers, so no workspace,
    /// inspector, or pending resource action may survive into the new session.
    pub(crate) fn reset_for_connection(&mut self) {
        self.connection = ClusterConnectionState::Connecting;
        self.namespaces_load = ClusterLoadState::Loading;
        self.namespaces_reconnecting = None;
        self.api_resources_load = ClusterLoadState::Loading;
        self.namespaces.clear();
        self.selected_namespaces = SelectedNamespaces::default();
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
    /// Replaces the discovered resources and rebuilds the navigation from them. The
    /// selection is kept as is.
    pub(crate) fn set_discovered_api_resources(&mut self, api_resources: Vec<ApiResource>) {
        self.resource_navigation = build_resource_navigation(api_resources.clone());
        self.discovered_api_resources = api_resources;
    }

    fn new(metrics_api_available: bool) -> Self {
        Self {
            discovered_api_resources: Vec::new(),
            resource_navigation: ResourceNavigation::default(),
            custom_resource_columns: BTreeMap::new(),
            scalable_api_resources: BTreeSet::new(),
            selected_api_resource: None,
            resource_cache: HashMap::new(),
            resource_table_cache: Default::default(),
            helm_release_cache: HashMap::new(),
            active_watchers: HashSet::new(),
            all_namespaces_watch_scopes: HashMap::new(),
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
            pending_force_delete: None,
            pending_deployment_restart: None,
            pending_cron_job_run: None,
            cron_job_runs: InFlightOperations::default(),
            pending_helm_rollback: None,
            helm_rollbacks: InFlightOperations::default(),
            pending_scale: None,
            operation_history: Vec::new(),
            transient_operation_toasts: Vec::new(),
        }
    }

    fn reset_for_connection(&mut self) {
        let cron_job_runs = self.cron_job_runs.restarted();
        let helm_rollbacks = self.helm_rollbacks.restarted();
        *self = Self {
            cron_job_runs,
            helm_rollbacks,
            ..Self::new(false)
        };
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
    pub(crate) revision: u64,
}

#[derive(Debug, Default)]
pub(crate) struct NodeMetricsState {
    pub(crate) usages: BTreeMap<String, NodeUsage>,
    pub(crate) error: Option<String>,
    pub(crate) revision: u64,
}

#[derive(Debug)]
pub(crate) enum ClusterConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Failed(String),
}
