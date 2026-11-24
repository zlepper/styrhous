use super::*;

#[derive(Debug)]

pub(crate) struct ClusterState {
    pub(crate) name: String,
    pub(crate) presentation: crate::context_presentation::ContextPresentation,
    /// The API server URL from the kubeconfig, which says where the cluster runs.
    pub(crate) server: Option<String>,
    pub(crate) cluster_key: i32,
    pub(crate) namespaces: BTreeMap<SortedName, MinimalNamespace>,
    pub(crate) connection: ClusterConnectionState,
    pub(crate) namespaces_load: ClusterLoadState,
    /// Set while the namespace watcher has been retrying past the grace
    /// period; the last known namespaces stay usable meanwhile.
    pub(crate) namespaces_reconnecting: Option<String>,
    /// When the current connection attempt passed each step; see `reachable`.
    pub(crate) connection_progress: ConnectionProgress,
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
    /// What discovery reported and what was assumed before it answered.
    pub(crate) api_discovery: ApiDiscoveryState,
    /// The API resources shown and which of them scale, derived from `api_discovery` by
    /// `refresh_api_resources`, in discovery order.
    pub(crate) displayed_api_resources: ApiResourceSet,
    pub(crate) resource_navigation: ResourceNavigation,
    pub(crate) custom_resource_columns: BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    pub(crate) selected_api_resource: Option<ApiResource>,
    pub(crate) resource_cache: HashMap<ResourceWatchKey, ResourceWatchState>,
    pub(in crate::ui) resource_table_cache: super::super::resource_table_cache::ResourceTableCache,
    pub(crate) helm_release_cache: HashMap<String, HelmReleaseWatchState>,
    pub(crate) active_watchers: HashSet<ResourceWatchKey>,
    /// The namespaces each active all-namespaces watcher was started for. It
    /// filters events by this set, so it is restarted only when the set changes.
    pub(crate) all_namespaces_watch_scopes: HashMap<ApiResource, crate::worker::WatchedNamespaces>,
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
    pub(crate) pending_deployment_rollback: Option<PendingDeploymentRollback>,
    pub(crate) next_deployment_revisions_request_id: u64,
    pub(crate) deployment_rollbacks: InFlightOperations<DeploymentKey>,
    pub(crate) pending_scale: Option<PendingScale>,
    /// Outcomes are intentionally session-scoped: rebuilding this resource
    /// state on reconnect drops entries from the previous cluster session.
    pub(crate) operation_history: Vec<OperationHistoryEntry>,
    /// Brief feedback for newly recorded outcomes. This is reset with the
    /// resource session so a previous connection cannot surface a stale toast.
    pub(crate) transient_operation_toasts: Vec<TransientOperationToast>,
    // Node maintenance and the Nodes table's Pod counts.
    pub(crate) pending_node_drain: Option<PendingNodeDrain>,
    /// The Node each submitted drain targets.
    pub(crate) node_drains: InFlightOperations<String>,
    /// The Node each submitted cordon or uncordon targets.
    pub(crate) node_cordons: InFlightOperations<String>,
    pub(crate) node_pod_counts: NodePodCountsState,
    pub(crate) node_pod_counts_active: bool,
}

impl ClusterState {
    pub(crate) fn new(cluster_key: i32, name: String) -> Self {
        Self {
            name,
            presentation: Default::default(),
            server: None,
            cluster_key,
            namespaces: BTreeMap::new(),
            connection: ClusterConnectionState::Disconnected,
            namespaces_load: ClusterLoadState::Loading,
            namespaces_reconnecting: None,
            connection_progress: ConnectionProgress::default(),
            selected_namespaces: SelectedNamespaces::default(),
            resources: ClusterResourceState::new(false),
        }
    }

    /// The selected namespaces resolved against the discovered ones.
    pub(crate) fn namespace_scope(&self) -> NamespaceScope<'_> {
        NamespaceScope::new(&self.selected_namespaces, &self.namespaces)
    }

    /// Whether "All namespaces" is still resolved against a namespace list that has not
    /// arrived, or that failed. An explicit selection never waits for the list.
    pub(crate) fn awaited_namespace_list(&self) -> NamespaceListWait<'_> {
        match (&self.selected_namespaces, &self.namespaces_load) {
            (SelectedNamespaces::All, ClusterLoadState::Loading) => NamespaceListWait::Loading,
            (SelectedNamespaces::All, ClusterLoadState::Failed(error)) => {
                NamespaceListWait::Failed(error)
            }
            _ => NamespaceListWait::Ready,
        }
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
        self.connection_progress = ConnectionProgress::default();
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
        state.mark_loaded_for_test();
        state.resources.pod_metrics_api_available = true;
        state.resources.node_metrics_api_available = true;
        state
    }
}

impl ClusterState {
    /// Whether the API server has answered an authenticated request this session. Until
    /// then a connection that was set up may still fail its first request.
    pub(crate) fn reachable(&self) -> bool {
        self.connection_progress.reachable.is_some()
    }

    #[cfg(any(test, feature = "benchmarks"))]
    /// Marks the namespaces listed, the API server reachable and discovery complete, as
    /// for a session that has finished loading. The connection state is left alone.
    pub(crate) fn mark_loaded_for_test(&mut self) {
        self.namespaces_load = ClusterLoadState::Ready;
        self.connection_progress.reachable = Some(self.connection_progress.started);
        self.resources.api_discovery.complete = true;
    }

    /// The connection was set up: the kubeconfig and credentials were loaded.
    pub(crate) fn mark_authenticated(&mut self, now: f64) {
        if self.connection_progress.authenticated.is_none() {
            self.connection_progress.authenticated = Some(now);
            self.log_connect_timing("authenticated", now);
        }
    }

    /// The API server answered an authenticated request; see `reachable`.
    pub(crate) fn mark_reachable(&mut self, now: f64) {
        // A failed session's late answers do not revive it.
        if !self.reachable() && !matches!(self.connection, ClusterConnectionState::Failed(_)) {
            self.connection_progress.reachable = Some(now);
            self.log_connect_timing("API server answered", now);
        }
    }

    /// Rows of `api_resource` arrived; the first ones of the selected resource are timed.
    pub(crate) fn mark_rows_received(&mut self, api_resource: &ApiResource, now: f64) {
        if self.connection_progress.first_rows.is_none()
            && self.selected_api_resource.as_ref() == Some(api_resource)
        {
            self.connection_progress.first_rows = Some(now);
            self.log_connect_timing("first table rows received", now);
        }
    }

    fn log_connect_timing(&self, milestone: &str, now: f64) {
        tracing::info!(
            target: "styrhous::connect_timing",
            cluster = %self.name,
            "{milestone} {:.0} ms after connecting",
            (now - self.connection_progress.started) * 1000.0
        );
    }
}

/// What a namespace scope still waits for; see `ClusterState::awaited_namespace_list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamespaceListWait<'a> {
    Ready,
    Loading,
    Failed(&'a str),
}

/// When a connection attempt started and passed each step, in seconds of egui input time.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct ConnectionProgress {
    pub(crate) started: f64,
    pub(crate) authenticated: Option<f64>,
    pub(crate) reachable: Option<f64>,
    pub(crate) first_rows: Option<f64>,
}

impl ConnectionProgress {
    pub(crate) fn started_at(now: f64) -> Self {
        Self {
            started: now,
            ..Self::default()
        }
    }
}

impl ClusterResourceState {
    /// Forgets every watched row of `api_resource`, and the table prepared from them.
    pub(crate) fn drop_rows_of(&mut self, api_resource: &ApiResource) {
        self.resource_cache
            .retain(|(resource, _), _| resource != api_resource);
        self.resource_table_cache.clear();
    }

    /// Whether the Pod and Node usage columns show the Metrics API as available. Until
    /// discovery completes it is not known, so the columns wait for data instead.
    pub(crate) fn metrics_shown_as_available(&self) -> (bool, bool) {
        let undecided = !self.api_discovery.metrics_decided();
        (
            self.pod_metrics_api_available || undecided,
            self.node_metrics_api_available || undecided,
        )
    }

    /// Derives the shown resources, their scale support and the navigation from the
    /// discovery state. The selection is kept as is.
    pub(crate) fn refresh_api_resources(&mut self) {
        self.displayed_api_resources = self.api_discovery.displayed();
        self.resource_navigation =
            build_resource_navigation(self.displayed_api_resources.api_resources.clone());
    }

    fn new(metrics_api_available: bool) -> Self {
        Self {
            api_discovery: ApiDiscoveryState::default(),
            displayed_api_resources: ApiResourceSet::default(),
            resource_navigation: ResourceNavigation::default(),
            custom_resource_columns: BTreeMap::new(),
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
            pending_deployment_rollback: None,
            next_deployment_revisions_request_id: 0,
            deployment_rollbacks: InFlightOperations::default(),
            pending_scale: None,
            operation_history: Vec::new(),
            transient_operation_toasts: Vec::new(),
            pending_node_drain: None,
            node_drains: InFlightOperations::default(),
            node_cordons: InFlightOperations::default(),
            node_pod_counts: NodePodCountsState::default(),
            node_pod_counts_active: false,
        }
    }

    fn reset_for_connection(&mut self) {
        let cron_job_runs = self.cron_job_runs.restarted();
        let helm_rollbacks = self.helm_rollbacks.restarted();
        let deployment_rollbacks = self.deployment_rollbacks.restarted();
        let node_drains = self.node_drains.restarted();
        let node_pod_counts = self.node_pod_counts.restarted();
        let node_cordons = self.node_cordons.restarted();
        // Like operation ids, request ids continue so a reply that outlives its
        // session cannot fill a dialog opened in the next one.
        let next_deployment_revisions_request_id = self.next_deployment_revisions_request_id;
        *self = Self {
            cron_job_runs,
            helm_rollbacks,
            deployment_rollbacks,
            next_deployment_revisions_request_id,
            node_drains,
            node_cordons,
            node_pod_counts,
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
