use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    AvailableAksCluster, AvailableTailscaleCluster, Cluster, ClusterConnection,
    ClusterDiscoveryTools, ResourceDataUpdateRequest, ResourceDetailWatchRequest,
    ResourceYamlValidationRequest, add_aks_cluster, add_tailscale_cluster, apply_resource_yaml,
    delete_resource, discover_managed_clusters, force_delete_resource, get_resource_scale,
    get_resource_schema, get_resource_yaml, reload_kubeconfig, restart_deployment, run_cron_job,
    start_all_namespaces_resource_watcher, start_cluster_connection, start_resource_watcher,
    update_resource_data, update_resource_scale, validate_resource_yaml, watch_node_metrics,
    watch_pod_metrics_namespace, watch_resource_detail,
};
use crate::helm_release::HelmRelease;
use crate::helpers::ResultExt;
use crate::log_store::LogStoreAppender;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::{NodeUsage, PodUsage};
use crate::resource_detail::{ManagedResource, ResourceDetail, ResourceEvent};
use crate::resource_schema::ResourceSchema;
use crate::resource_table::CustomResourceColumn;
use anyhow::Error;
use async_trait::async_trait;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::info;

mod pod_logs;

#[allow(dead_code)]
pub(crate) trait AsAny: Any {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The concrete command API. It is deliberately not object-safe: each command
/// owns itself, receives worker state, and returns its normal Rust output.
#[async_trait]
pub(crate) trait WorkerCommand: Send + std::fmt::Debug + 'static {
    type Output: CommandOutput;

    async fn execute(self, state: &WorkerState) -> Self::Output;

    fn serializes_session_lifecycle(&self) -> bool {
        false
    }

    /// Commands carrying a cluster key are routed to that cluster's isolated
    /// runtime. Commands without one execute in the router runtime.
    fn cluster_key(&self) -> Option<i32> {
        None
    }
}

/// The channel-only, object-safe adapter for concrete commands.
#[async_trait]
pub(crate) trait ErasedWorkerCommand: AsAny + Send + std::fmt::Debug {
    async fn execute_boxed(self: Box<Self>, state: &WorkerState) -> Option<WorkerResultBox>;
    fn serializes_session_lifecycle(&self) -> bool;
    fn cluster_key(&self) -> Option<i32>;
}

#[async_trait]
impl<C: WorkerCommand> ErasedWorkerCommand for C {
    async fn execute_boxed(self: Box<Self>, state: &WorkerState) -> Option<WorkerResultBox> {
        (*self).execute(state).await.into_result_box()
    }

    fn serializes_session_lifecycle(&self) -> bool {
        WorkerCommand::serializes_session_lifecycle(self)
    }

    fn cluster_key(&self) -> Option<i32> {
        WorkerCommand::cluster_key(self)
    }
}

pub(crate) type WorkerCommandBox = Box<dyn ErasedWorkerCommand>;

/// Converts a concrete command output into an optional channel update.
/// Commands whose successful outcome is purely worker-local use `NoResult`,
/// while still forwarding a typed failure result.
pub(crate) trait CommandOutput: Send + std::fmt::Debug + 'static {
    fn into_result_box(self) -> Option<WorkerResultBox>;
}

impl<R: WorkerResult> CommandOutput for R {
    fn into_result_box(self) -> Option<WorkerResultBox> {
        Some(Box::new(self))
    }
}

#[derive(Debug)]
pub(crate) struct NoResult;

impl<E: WorkerResult> CommandOutput for Result<NoResult, E> {
    fn into_result_box(self) -> Option<WorkerResultBox> {
        self.err().map(|error| Box::new(error) as WorkerResultBox)
    }
}

/// A concrete UI update emitted by the worker.
pub(crate) trait WorkerResult: Send + std::fmt::Debug + 'static {
    fn apply(self, ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>);
}

impl<S: WorkerResult, E: WorkerResult> WorkerResult for Result<S, E> {
    fn apply(self, ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>) {
        match self {
            Ok(result) => result.apply(ui, commands),
            Err(result) => result.apply(ui, commands),
        }
    }
}

impl WorkerResult for () {
    fn apply(self, _ui: &mut crate::ui::state::UiState, _commands: &mut Vec<WorkerCommandBox>) {}
}

/// The channel-only, object-safe adapter for concrete UI updates.
pub(crate) trait ErasedWorkerResult: AsAny + Send + std::fmt::Debug {
    fn apply_boxed(
        self: Box<Self>,
        ui: &mut crate::ui::state::UiState,
        commands: &mut Vec<WorkerCommandBox>,
    );
}

impl<R: WorkerResult> ErasedWorkerResult for R {
    fn apply_boxed(
        self: Box<Self>,
        ui: &mut crate::ui::state::UiState,
        commands: &mut Vec<WorkerCommandBox>,
    ) {
        (*self).apply(ui, commands);
    }
}

pub(crate) type WorkerResultBox = Box<dyn ErasedWorkerResult>;

/// Trait abstracting the worker interface for testability
pub trait WorkerTrait: Default {
    fn with_repaint_context(_context: egui::Context) -> Self {
        Self::default()
    }

    fn start(&mut self);
    fn get_next_message(&mut self) -> Option<WorkerResultBox>;
    fn send_command(&mut self, command: WorkerCommandBox);
    fn set_log_store_appender(&mut self, _appender: LogStoreAppender) {}
}

#[derive(Default)]
pub struct Worker {
    inner: Option<WorkerInner>,
    pending_commands: VecDeque<WorkerCommandBox>,
    repaint_context: Option<egui::Context>,
    log_store_appender: Option<LogStoreAppender>,
}

impl Worker {
    pub(crate) fn with_repaint_context(context: egui::Context) -> Self {
        Self {
            inner: None,
            pending_commands: VecDeque::new(),
            repaint_context: Some(context),
            log_store_appender: None,
        }
    }

    pub fn start(&mut self) {
        if self.inner.is_none() {
            let (command_channel_sender, command_channel_receiver) =
                mpsc::channel::<WorkerCommandBox>(64);
            let (result_channel_sender, result_channel_receiver) = mpsc::channel(1024);
            let result_sender =
                WorkerResultSender::new(result_channel_sender, self.repaint_context.clone());

            command_channel_sender
                .try_send(Box::new(LoadClusters))
                .expect("Failed to send initial LoadClusters command");

            let state = Arc::new(WorkerState {
                results: result_sender,
                connections: Arc::new(Mutex::new(HashMap::new())),
                resource_watches: Arc::new(Mutex::new(ResourceWatchRegistry::default())),
                detail_watches: Arc::new(TaskRegistry::default()),
                pod_metrics_watches: Arc::new(TaskRegistry::default()),
                node_metrics_watches: Arc::new(TaskRegistry::default()),
                log_streams: Arc::new(TaskRegistry::default()),
                watch_initialization_slots: Arc::new(Mutex::new(HashMap::new())),
                log_store_appender: self.log_store_appender.clone(),
            });

            let worker = WorkerRuntime {
                receiver: command_channel_receiver,
                state,
            };

            let _ = std::thread::spawn(move || {
                worker.run();
            });

            self.inner = Some(WorkerInner {
                receiver: result_channel_receiver,
                sender: command_channel_sender,
            })
        }
    }

    pub(crate) fn get_next_message(&mut self) -> Option<WorkerResultBox> {
        self.flush_pending_commands();
        if let Some(inner) = &mut self.inner {
            inner.receiver.try_recv().ok()
        } else {
            None
        }
    }

    pub(crate) fn send_command(&mut self, command: WorkerCommandBox) {
        self.pending_commands.push_back(command);
        self.flush_pending_commands();
    }

    /// Move as many queued UI commands as the bounded worker channel currently accepts.
    /// This method deliberately never waits: a slow Kubernetes operation must not freeze the UI.
    fn flush_pending_commands(&mut self) {
        let Some(inner) = &self.inner else {
            return;
        };
        while let Some(command) = self.pending_commands.pop_front() {
            match inner.sender.try_send(command) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(command)) => {
                    self.pending_commands.push_front(command);
                    // Channel capacity changes do not themselves wake egui. Keep retrying at a
                    // modest cadence so queued stop commands are delivered even when the worker
                    // command produces no result and the user is otherwise idle.
                    if let Some(context) = &self.repaint_context {
                        context.request_repaint_after(Duration::from_millis(10));
                    }
                    break;
                }
                Err(mpsc::error::TrySendError::Closed(command)) => {
                    tracing::error!(?command, "Worker command channel closed");
                }
            }
        }
    }

    pub(crate) fn set_log_store_appender(&mut self, appender: LogStoreAppender) {
        assert!(
            self.inner.is_none(),
            "The log store appender must be configured before starting the worker"
        );
        self.log_store_appender = Some(appender);
    }
}

impl WorkerTrait for Worker {
    fn with_repaint_context(context: egui::Context) -> Self {
        Self::with_repaint_context(context)
    }

    fn start(&mut self) {
        Worker::start(self)
    }

    fn get_next_message(&mut self) -> Option<WorkerResultBox> {
        Worker::get_next_message(self)
    }

    fn send_command(&mut self, command: WorkerCommandBox) {
        Worker::send_command(self, command)
    }

    fn set_log_store_appender(&mut self, appender: LogStoreAppender) {
        Worker::set_log_store_appender(self, appender)
    }
}

/// Mock worker for testing - allows injecting predefined results
#[cfg(test)]
#[derive(Default)]
pub struct MockWorker {
    pub results: VecDeque<WorkerResultBox>,
    pub commands: Vec<WorkerCommandBox>,
}

#[cfg(test)]
impl MockWorker {
    /// Queue a typed worker update for delivery on the next UI frame.
    pub fn enqueue_result<R: WorkerResult>(&mut self, result: R) {
        self.results.push_back(Box::new(result));
    }

    /// Return the latest command when it has the requested concrete type.
    pub fn last_command<C: WorkerCommand>(&self) -> Option<&C> {
        self.commands
            .last()
            .and_then(|command| command.as_ref().as_any().downcast_ref())
    }

    /// Iterate over the commands of one concrete type in their dispatch order.
    pub fn commands_of<C: WorkerCommand>(&self) -> impl Iterator<Item = &C> {
        self.commands
            .iter()
            .filter_map(|command| command.as_ref().as_any().downcast_ref())
    }
}

#[cfg(test)]
impl WorkerTrait for MockWorker {
    fn start(&mut self) {
        // No-op for mock
    }

    fn get_next_message(&mut self) -> Option<WorkerResultBox> {
        self.results.pop_front()
    }

    fn send_command(&mut self, command: WorkerCommandBox) {
        self.commands.push(command);
    }
}

struct WorkerInner {
    sender: mpsc::Sender<WorkerCommandBox>,
    receiver: mpsc::Receiver<WorkerResultBox>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
/// Worker watch registry key for a resource scope.
struct ResourceScope {
    pub cluster_key: i32,
    pub api_resource: ApiResource,
    pub namespace: Option<String>,
}

#[derive(Debug)]
pub(crate) struct LoadClusters;
#[derive(Debug)]
pub(crate) struct LoadImportedClusters;
#[derive(Debug)]
pub(crate) struct LoadManagedClusterDiscovery;
#[derive(Debug)]
pub(crate) struct AddAksCluster {
    pub(crate) subscription_id: String,
    pub(crate) resource_group: String,
    pub(crate) cluster_name: String,
}
#[derive(Debug)]
pub(crate) struct AddTailscaleCluster {
    pub(crate) host_name: String,
}
#[derive(Debug)]
pub(crate) struct ConnectToCluster {
    pub(crate) cluster: String,
    pub(crate) cluster_key: i32,
}
/// Replace every source used for one resource type as a single worker-side
/// operation. This makes a namespace-scope change immediately supersede any
/// queued initializations from the previous scope.
#[derive(Debug)]
pub(crate) struct ReconcileResourceWatches {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) sources: Vec<ResourceWatchSource>,
}

#[derive(Debug, Clone)]
pub(crate) enum ResourceWatchSource {
    Namespace(String),
    AllNamespaces(BTreeSet<String>),
    Cluster,
}
#[derive(Debug)]
pub(crate) struct StartResourceDetailWatch {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) resource_uid: String,
    pub(crate) pod_metrics_api_available: bool,
    pub(crate) node_metrics_api_available: bool,
}
#[derive(Debug)]
pub(crate) struct StopResourceDetailWatch {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
}
#[derive(Debug)]
pub(crate) struct StartPodMetricsWatch {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
}
#[derive(Debug)]
pub(crate) struct StopPodMetricsWatch {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
}
#[derive(Debug)]
pub(crate) struct StartNodeMetricsWatch {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct StopNodeMetricsWatch {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct GetResourceYaml {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct DeleteResource {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) resource_uid: Option<String>,
    pub(crate) bulk_delete_id: Option<u64>,
}
#[derive(Debug)]
pub(crate) struct ForceDeleteResource {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) resource_uid: String,
}
#[derive(Debug)]
pub(crate) struct RestartDeployment {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct RunCronJob {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct GetResourceScale {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct UpdateResourceScale {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) replicas: i32,
}
pub(crate) struct ApplyResourceYaml {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) yaml: String,
}
#[derive(Debug)]
pub(crate) struct LoadResourceSchema {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
}
pub(crate) struct ValidateResourceYaml {
    pub(crate) editor_id: u64,
    pub(crate) revision: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) yaml: String,
}
#[derive(Debug)]
pub(crate) struct UpdateResourceData {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) request_id: u64,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: String,
    pub(crate) resource_name: String,
    pub(crate) update: ResourceDataUpdate,
}
#[derive(Debug)]
pub(crate) struct StartPodLogStream {
    pub(crate) cluster_key: i32,
    pub(crate) log_window_id: u64,
    pub(crate) namespace: String,
    pub(crate) pod_name: String,
    pub(crate) container: String,
}
#[derive(Debug)]
pub(crate) struct StopPodLogStream {
    pub(crate) cluster_key: i32,
    pub(crate) log_window_id: u64,
}

/// The values are intentionally omitted from Debug output because this command can
/// contain Secret plaintext. The worker logs failed commands at debug format.
pub struct ResourceDataUpdate {
    pub expected_resource_version: String,
    pub expected_values: BTreeMap<String, String>,
    pub updated_values: BTreeMap<String, String>,
}

impl std::fmt::Debug for ResourceDataUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceDataUpdate")
            .field("keys", &self.updated_values.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl std::fmt::Debug for ApplyResourceYaml {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApplyResourceYaml")
            .field("editor_id", &self.editor_id)
            .field("cluster_key", &self.cluster_key)
            .field("api_resource", &self.api_resource)
            .field("namespace", &self.namespace)
            .field("resource_name", &self.resource_name)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ValidateResourceYaml {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateResourceYaml")
            .field("editor_id", &self.editor_id)
            .field("revision", &self.revision)
            .field("cluster_key", &self.cluster_key)
            .field("api_resource", &self.api_resource)
            .field("namespace", &self.namespace)
            .field("resource_name", &self.resource_name)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct KubernetesClustersUpdated(pub(crate) Vec<Cluster>);
#[derive(Debug)]
pub(crate) struct ImportedKubernetesClusters(pub(crate) Vec<Cluster>);
#[derive(Debug)]
pub(crate) struct ManagedClusterImported;
#[derive(Debug)]
pub(crate) struct ManagedClusterDiscoveryUpdated {
    pub(crate) tools: ClusterDiscoveryTools,
    pub(crate) aks_clusters: Vec<AvailableAksCluster>,
    pub(crate) tailscale_clusters: Vec<AvailableTailscaleCluster>,
    pub(crate) azure_error: Option<String>,
    pub(crate) azure_warning: Option<String>,
    pub(crate) tailscale_error: Option<String>,
}
#[derive(Debug)]
pub(crate) struct ManagedClusterDiscoveryFailed {
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesNamespacesAdded {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: MinimalNamespace,
}
#[derive(Debug)]
pub(crate) struct KubernetesNamespacesDeleted {
    pub(crate) cluster_key: i32,
    pub(crate) namespace_name: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesNamespacesReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) namespaces: Vec<MinimalNamespace>,
}
#[derive(Debug)]
pub(crate) struct KubernetesNamespacesLoadFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesApisLoaded {
    pub(crate) cluster_key: i32,
    pub(crate) api_resources: Vec<ApiResource>,
    pub(crate) scalable_api_resources: std::collections::BTreeSet<ApiResource>,
    pub(crate) pod_metrics_api_available: bool,
    pub(crate) node_metrics_api_available: bool,
}
#[derive(Debug)]
pub(crate) struct KubernetesCustomResourceColumnsLoaded {
    pub(crate) cluster_key: i32,
    pub(crate) columns: std::collections::BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourceSchemasLoaded {
    pub(crate) cluster_key: i32,
    pub(crate) schemas: std::collections::BTreeMap<ApiResource, ResourceSchema>,
}
#[derive(Debug)]
pub(crate) struct KubernetesApisLoadFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesClusterConnectionCreated {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourceAdded {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource: MinimalResource,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourceDeleted {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_uid: String,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourcesReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resources: Vec<MinimalResource>,
}
#[derive(Debug)]
pub(crate) struct KubernetesResourceWatchStarted {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
}
#[derive(Debug, Clone)]
pub(crate) struct KubernetesResourceWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct HelmReleasesReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) releases: Vec<HelmRelease>,
}
#[derive(Debug)]
pub(crate) struct HelmReleaseBackendFailed {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) backend: &'static str,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) detail: Box<ResourceDetail>,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailDeleted {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
}
#[derive(Debug)]
pub(crate) struct ManagedResourcesReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) resources: Vec<ManagedResource>,
}
#[derive(Debug)]
pub(crate) struct ManagedResourcesWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceEventsReplaced {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) events: Vec<ResourceEvent>,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) events: bool,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct PodMetricsUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) usages: BTreeMap<String, PodUsage>,
}
#[derive(Debug)]
pub(crate) struct PodMetricsWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) namespace: String,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct PodMetricsApiUnavailable {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct NodeMetricsUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) usages: BTreeMap<String, NodeUsage>,
}
#[derive(Debug)]
pub(crate) struct NodeMetricsWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct NodeMetricsApiUnavailable {
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailPodUsageUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) usage: PodUsage,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailPodUsageFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailPodUsageMissing {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailNodeUsageUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) usage: NodeUsage,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailNodeUsageFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDetailNodeUsageMissing {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlFetched {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) yaml: String,
}
#[derive(Debug)]
pub(crate) struct ResourceSchemaLoaded {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) schema: ResourceSchema,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlValidated {
    pub(crate) editor_id: u64,
    pub(crate) revision: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlValidationFailed {
    pub(crate) editor_id: u64,
    pub(crate) revision: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) error: ResourceApiError,
}
#[derive(Debug)]
pub(crate) struct ResourceDeleteCompleted {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) bulk_delete_id: Option<u64>,
}
#[derive(Debug)]
pub(crate) struct ResourceForceDeleteCompleted {
    pub(crate) cluster_key: i32,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceApplyCompleted {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceApplyFailed {
    pub(crate) editor_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) error: ResourceApiError,
}
#[derive(Debug)]
pub(crate) struct DeploymentRestartCompleted {
    pub(crate) namespace: String,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceScaleFetched {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) replicas: i32,
}
#[derive(Debug)]
pub(crate) struct ResourceScaleUpdated {
    pub(crate) cluster_key: i32,
    pub(crate) resource_name: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDataUpdateCompleted {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) request_id: u64,
}
#[derive(Debug)]
pub(crate) struct PodLogStreamStarted {
    pub(crate) log_window_id: u64,
}
#[derive(Debug)]
pub(crate) struct PodLogStreamEnded {
    pub(crate) log_window_id: u64,
}
#[derive(Debug)]
pub(crate) struct PodLogStreamFailed {
    pub(crate) log_window_id: u64,
    pub(crate) error: String,
}

#[derive(Debug)]
pub(crate) struct WorkerError {
    pub(crate) error: Error,
}

#[derive(Debug)]
pub(crate) struct ClusterConnectionFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlFetchFailed {
    pub(crate) editor_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceSchemaLoadFailed {
    pub(crate) editor_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceDeleteFailed {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) bulk_delete_id: Option<u64>,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceForceDeleteFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct DeploymentRestartFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct CronJobRunCompleted {
    pub(crate) namespace: String,
    pub(crate) cron_job_name: String,
    pub(crate) job_name: String,
}
#[derive(Debug)]
pub(crate) struct CronJobRunFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceScaleFailed {
    pub(crate) cluster_key: i32,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlApplyCommandFailed {
    pub(crate) editor_id: u64,
    pub(crate) error: String,
}
#[derive(Debug)]
pub(crate) struct ResourceYamlValidationCommandFailed {
    pub(crate) editor_id: u64,
    pub(crate) revision: u64,
    pub(crate) error: String,
}

#[derive(Debug)]
pub(crate) enum ResourceYamlApplyFailure {
    Api(ResourceApplyFailed),
    Command(ResourceYamlApplyCommandFailed),
}

impl WorkerResult for ResourceYamlApplyFailure {
    fn apply(self, ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>) {
        match self {
            Self::Api(failure) => failure.apply(ui, commands),
            Self::Command(failure) => failure.apply(ui, commands),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ResourceYamlValidationFailure {
    Api(ResourceYamlValidationFailed),
    Command(ResourceYamlValidationCommandFailed),
}

impl WorkerResult for ResourceYamlValidationFailure {
    fn apply(self, ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>) {
        match self {
            Self::Api(failure) => failure.apply(ui, commands),
            Self::Command(failure) => failure.apply(ui, commands),
        }
    }
}
#[derive(Debug)]
pub(crate) struct ResourceDataUpdateFailed {
    pub(crate) cluster_key: i32,
    pub(crate) history_entry_id: u64,
    pub(crate) request_id: u64,
    pub(crate) error: String,
}

impl WorkerResult for WorkerError {
    fn apply(self, _ui: &mut crate::ui::state::UiState, _commands: &mut Vec<WorkerCommandBox>) {
        tracing::error!(error = ?self.error, "Worker command failed");
    }
}

#[async_trait]
impl WorkerCommand for LoadClusters {
    type Output = Result<KubernetesClustersUpdated, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state.stop_all_clusters().await;
        reload_kubeconfig()
            .await
            .map_err(|error| WorkerError { error })
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
    }
}

#[async_trait]
impl WorkerCommand for LoadImportedClusters {
    type Output = Result<ImportedKubernetesClusters, ManagedClusterDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        let KubernetesClustersUpdated(clusters) =
            reload_kubeconfig()
                .await
                .map_err(|error| ManagedClusterDiscoveryFailed {
                    error: format!(
                        "Could not reload kubeconfig after importing the cluster: {error:#}"
                    ),
                })?;
        Ok(ImportedKubernetesClusters(clusters))
    }
}

#[async_trait]
impl WorkerCommand for LoadManagedClusterDiscovery {
    type Output = Result<ManagedClusterDiscoveryUpdated, ManagedClusterDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        Ok(discover_managed_clusters().await?.into())
    }
}

#[async_trait]
impl WorkerCommand for AddAksCluster {
    type Output = Result<ManagedClusterImported, ManagedClusterDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        add_aks_cluster(
            &self.subscription_id,
            &self.resource_group,
            &self.cluster_name,
        )
        .await?;
        Ok(ManagedClusterImported)
    }
}

#[async_trait]
impl WorkerCommand for AddTailscaleCluster {
    type Output = Result<ManagedClusterImported, ManagedClusterDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        add_tailscale_cluster(&self.host_name).await?;
        Ok(ManagedClusterImported)
    }
}

impl From<crate::cluster_connection_manager::ClusterDiscovery> for ManagedClusterDiscoveryUpdated {
    fn from(discovery: crate::cluster_connection_manager::ClusterDiscovery) -> Self {
        Self {
            tools: discovery.tools,
            aks_clusters: discovery.aks_clusters,
            tailscale_clusters: discovery.tailscale_clusters,
            azure_error: discovery.azure_error,
            azure_warning: discovery.azure_warning,
            tailscale_error: discovery.tailscale_error,
        }
    }
}

impl From<anyhow::Error> for ManagedClusterDiscoveryFailed {
    fn from(error: anyhow::Error) -> Self {
        Self {
            error: format!("{error:#}"),
        }
    }
}

#[async_trait]
impl WorkerCommand for ConnectToCluster {
    type Output = Result<KubernetesClusterConnectionCreated, ClusterConnectionFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        state.stop_cluster(cluster_key).await;
        let result =
            start_cluster_connection(cluster_key, &self.cluster, state.results.clone()).await;
        match result {
            Ok(connection) => {
                state
                    .connections
                    .lock()
                    .await
                    .insert(cluster_key, connection);
                Ok(KubernetesClusterConnectionCreated { cluster_key })
            }
            Err(error) => Err(ClusterConnectionFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for ReconcileResourceWatches {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let generation = state
            .replace_resource_watch_sources(self.cluster_key, &self.api_resource)
            .await;
        let cluster_key = self.cluster_key;
        let session = state.resource_watch_session(cluster_key).await;
        for source in self.sources {
            let state = Arc::new(state.clone());
            let api_resource = self.api_resource.clone();
            tokio::spawn(async move {
                start_reconciled_resource_watch(
                    state,
                    cluster_key,
                    generation,
                    session,
                    api_resource,
                    source,
                )
                .await;
            });
        }
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

async fn start_reconciled_resource_watch(
    state: Arc<WorkerState>,
    cluster_key: i32,
    generation: u64,
    session: u64,
    api_resource: ApiResource,
    source: ResourceWatchSource,
) {
    let (namespace, watched_namespaces) = match source {
        ResourceWatchSource::Namespace(namespace) => (Some(namespace), None),
        ResourceWatchSource::AllNamespaces(namespaces) => (None, Some(namespaces)),
        ResourceWatchSource::Cluster => (None, None),
    };
    if !state
        .resource_watch_generation_is_current(cluster_key, &api_resource, generation, session)
        .await
    {
        return;
    }
    let failure = |error| KubernetesResourceWatchFailed {
        cluster_key,
        api_resource: api_resource.clone(),
        namespace: namespace.clone(),
        error,
    };
    let client = match state.client_for_cluster(cluster_key).await {
        Ok(client) => client,
        Err(error) => {
            let _ = state.results.send(failure(format!("{error:#?}"))).await;
            return;
        }
    };
    let initialization_slot = match state
        .watch_initialization_slot(cluster_key)
        .await
        .acquire_owned()
        .await
    {
        Ok(slot) => slot,
        Err(error) => {
            let _ = state
                .results
                .send(failure(format!(
                    "Unable to acquire watch initialization slot: {error}"
                )))
                .await;
            return;
        }
    };
    if !state
        .resource_watch_generation_is_current(cluster_key, &api_resource, generation, session)
        .await
    {
        return;
    }
    let (initialized_sender, initialized_receiver) = oneshot::channel();
    let started = if let Some(namespaces) = watched_namespaces {
        start_all_namespaces_resource_watcher(
            cluster_key,
            client,
            api_resource.clone(),
            namespaces,
            state.results.clone(),
            Some(initialized_sender),
        )
        .await
    } else {
        start_resource_watcher(
            cluster_key,
            client,
            api_resource.clone(),
            namespace.clone(),
            state.results.clone(),
            Some(initialized_sender),
        )
        .await
    };
    match started {
        Ok((result, task)) => {
            tokio::spawn(async move {
                let _ = initialized_receiver.await;
                drop(initialization_slot);
            });
            let key = ResourceScope {
                cluster_key,
                api_resource,
                namespace,
            };
            if state
                .install_resource_watch_if_current(key, generation, session, task)
                .await
            {
                state
                    .results
                    .send(result)
                    .await
                    .log_if_error("Failed to send resource watch start result");
            }
        }
        Err(error) => {
            let _ = state.results.send(failure(format!("{error:#?}"))).await;
        }
    }
}

#[async_trait]
impl WorkerCommand for StartResourceDetailWatch {
    type Output = Result<NoResult, ResourceDetailWatchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = |error| ResourceDetailWatchFailed {
            cluster_key: self.cluster_key,
            history_entry_id: self.history_entry_id,
            events: false,
            error: format!("{error:#?}"),
        };
        let client = state
            .client_for_cluster(self.cluster_key)
            .await
            .map_err(failure)?;
        let key = (self.cluster_key, self.history_entry_id);
        let event_sender = state.results.clone();
        state
            .detail_watches
            .replace_after_abort(key, move || {
                tokio::spawn(watch_resource_detail(ResourceDetailWatchRequest {
                    cluster_key: self.cluster_key,
                    client,
                    api_resource: self.api_resource,
                    namespace: self.namespace,
                    resource_name: self.resource_name,
                    resource_uid: self.resource_uid,
                    history_entry_id: self.history_entry_id,
                    pod_metrics_api_available: self.pod_metrics_api_available,
                    node_metrics_api_available: self.node_metrics_api_available,
                    event_sender,
                }))
            })
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StopResourceDetailWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .detail_watches
            .abort(&(self.cluster_key, self.history_entry_id))
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StartPodMetricsWatch {
    type Output = Result<NoResult, PodMetricsWatchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let client = state
            .client_for_cluster(self.cluster_key)
            .await
            .map_err(|error| PodMetricsWatchFailed {
                cluster_key: self.cluster_key,
                namespace: self.namespace.clone(),
                error: format!("{error:#?}"),
            })?;
        let key = (self.cluster_key, self.namespace.clone());
        let task = tokio::spawn(watch_pod_metrics_namespace(
            self.cluster_key,
            client,
            self.namespace,
            state.results.clone(),
        ));
        state.replace_pod_metrics_watch(key, task).await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StopPodMetricsWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .pod_metrics_watches
            .abort_matching(|key| key.0 == self.cluster_key && key.1 == self.namespace)
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StartNodeMetricsWatch {
    type Output = Result<NoResult, NodeMetricsWatchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let client = state
            .client_for_cluster(self.cluster_key)
            .await
            .map_err(|error| NodeMetricsWatchFailed {
                cluster_key: self.cluster_key,
                error: format!("{error:#?}"),
            })?;
        let task = tokio::spawn(watch_node_metrics(
            self.cluster_key,
            client,
            state.results.clone(),
        ));
        state
            .replace_node_metrics_watch(self.cluster_key, task)
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StopNodeMetricsWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .node_metrics_watches
            .abort_matching(|cluster_key| *cluster_key == self.cluster_key)
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for GetResourceYaml {
    type Output = Result<ResourceYamlFetched, ResourceYamlFetchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let editor_id = self.editor_id;
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => get_resource_yaml(
                editor_id,
                self.cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
            )
            .await
            .map_err(|error| ResourceYamlFetchFailed {
                editor_id,
                error: format!("{error:#?}"),
            }),
            Err(error) => Err(ResourceYamlFetchFailed {
                editor_id,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for LoadResourceSchema {
    type Output = Result<ResourceSchemaLoaded, ResourceSchemaLoadFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let editor_id = self.editor_id;
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => {
                get_resource_schema(editor_id, self.cluster_key, client, self.api_resource)
                    .await
                    .map_err(|error| ResourceSchemaLoadFailed {
                        editor_id,
                        error: format!("{error:#?}"),
                    })
            }
            Err(error) => Err(ResourceSchemaLoadFailed {
                editor_id,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for DeleteResource {
    type Output = Result<ResourceDeleteCompleted, ResourceDeleteFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = ResourceDeleteFailed {
            cluster_key: self.cluster_key,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            resource_name: self.resource_name.clone(),
            bulk_delete_id: self.bulk_delete_id,
            error: String::new(),
        };
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => delete_resource(
                self.cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
                self.resource_uid,
                self.bulk_delete_id,
            )
            .await
            .map_err(|error| ResourceDeleteFailed {
                error: format!("{error:#?}"),
                ..failure
            }),
            Err(error) => Err(ResourceDeleteFailed {
                error: format!("{error:#?}"),
                ..failure
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for ForceDeleteResource {
    type Output = Result<ResourceForceDeleteCompleted, ResourceForceDeleteFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => force_delete_resource(
                cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
                self.resource_uid,
            )
            .await
            .map_err(|error| ResourceForceDeleteFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
            Err(error) => Err(ResourceForceDeleteFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for RestartDeployment {
    type Output = Result<DeploymentRestartCompleted, DeploymentRestartFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => restart_deployment(client, self.namespace, self.resource_name)
                .await
                .map_err(|error| DeploymentRestartFailed {
                    cluster_key,
                    error: format!("{error:#?}"),
                }),
            Err(error) => Err(DeploymentRestartFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for RunCronJob {
    type Output = Result<CronJobRunCompleted, CronJobRunFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => run_cron_job(client, self.namespace, self.resource_name)
                .await
                .map_err(|error| CronJobRunFailed {
                    cluster_key,
                    error: format!("{error:#?}"),
                }),
            Err(error) => Err(CronJobRunFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for GetResourceScale {
    type Output = Result<ResourceScaleFetched, ResourceScaleFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => get_resource_scale(
                cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
            )
            .await
            .map_err(|error| ResourceScaleFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
            Err(error) => Err(ResourceScaleFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for UpdateResourceScale {
    type Output = Result<ResourceScaleUpdated, ResourceScaleFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        match state.client_for_cluster(cluster_key).await {
            Ok(client) => update_resource_scale(
                cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
                self.replicas,
            )
            .await
            .map_err(|error| ResourceScaleFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
            Err(error) => Err(ResourceScaleFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for ApplyResourceYaml {
    type Output = Result<ResourceApplyCompleted, ResourceYamlApplyFailure>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let editor_id = self.editor_id;
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => match apply_resource_yaml(
                editor_id,
                self.cluster_key,
                client,
                self.api_resource,
                self.namespace,
                self.resource_name,
                self.yaml,
            )
            .await
            {
                Ok(result) => result.map_err(ResourceYamlApplyFailure::Api),
                Err(error) => Err(ResourceYamlApplyFailure::Command(
                    ResourceYamlApplyCommandFailed {
                        editor_id,
                        error: format!("{error:#?}"),
                    },
                )),
            },
            Err(error) => Err(ResourceYamlApplyFailure::Command(
                ResourceYamlApplyCommandFailed {
                    editor_id,
                    error: format!("{error:#?}"),
                },
            )),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for ValidateResourceYaml {
    type Output = Result<ResourceYamlValidated, ResourceYamlValidationFailure>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let editor_id = self.editor_id;
        let revision = self.revision;
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => match validate_resource_yaml(ResourceYamlValidationRequest {
                editor_id,
                revision,
                cluster_key: self.cluster_key,
                client,
                api_resource: self.api_resource,
                namespace: self.namespace,
                resource_name: self.resource_name,
                yaml: self.yaml,
            })
            .await
            {
                Ok(result) => result.map_err(ResourceYamlValidationFailure::Api),
                Err(error) => Err(ResourceYamlValidationFailure::Command(
                    ResourceYamlValidationCommandFailed {
                        editor_id,
                        revision,
                        error: format!("{error:#?}"),
                    },
                )),
            },
            Err(error) => Err(ResourceYamlValidationFailure::Command(
                ResourceYamlValidationCommandFailed {
                    editor_id,
                    revision,
                    error: format!("{error:#?}"),
                },
            )),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for UpdateResourceData {
    type Output = Result<ResourceDataUpdateCompleted, ResourceDataUpdateFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = ResourceDataUpdateFailed {
            cluster_key: self.cluster_key,
            history_entry_id: self.history_entry_id,
            request_id: self.request_id,
            error: String::new(),
        };
        match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => update_resource_data(ResourceDataUpdateRequest {
                cluster_key: self.cluster_key,
                history_entry_id: self.history_entry_id,
                request_id: self.request_id,
                client,
                api_resource: self.api_resource,
                namespace: self.namespace,
                resource_name: self.resource_name,
                expected_values: &self.update.expected_values,
                updated_values: &self.update.updated_values,
                expected_resource_version: &self.update.expected_resource_version,
            })
            .await
            .map_err(|error| ResourceDataUpdateFailed {
                error: format!("{error:#?}"),
                ..failure
            }),
            Err(error) => Err(ResourceDataUpdateFailed {
                error: format!("{error:#?}"),
                ..failure
            }),
        }
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StartPodLogStream {
    type Output = Result<PodLogStreamStarted, PodLogStreamFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = |error| PodLogStreamFailed {
            log_window_id: self.log_window_id,
            error: format!("{error:#?}"),
        };
        let client = state
            .client_for_cluster(self.cluster_key)
            .await
            .map_err(failure)?;
        let log_store_appender =
            state
                .log_store_appender
                .clone()
                .ok_or_else(|| PodLogStreamFailed {
                    log_window_id: self.log_window_id,
                    error: format!(
                        "Pod log storage is not initialized for cluster_key {}",
                        self.cluster_key
                    ),
                })?;
        let key = (self.cluster_key, self.log_window_id);
        let event_sender = state.results.clone();
        state
            .log_streams
            .replace_after_abort(key, move || {
                tokio::spawn(pod_logs::stream(
                    self.log_window_id,
                    client,
                    self.namespace,
                    self.pod_name,
                    self.container,
                    log_store_appender,
                    event_sender,
                ))
            })
            .await;
        Ok(PodLogStreamStarted {
            log_window_id: self.log_window_id,
        })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StopPodLogStream {
    type Output = Result<PodLogStreamEnded, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .log_streams
            .abort(&(self.cluster_key, self.log_window_id))
            .await;
        Ok(PodLogStreamEnded {
            log_window_id: self.log_window_id,
        })
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[derive(Clone)]
pub struct WorkerResultSender {
    sender: mpsc::Sender<WorkerResultBox>,
    repaint_context: Option<egui::Context>,
}

impl WorkerResultSender {
    pub(crate) fn new(
        sender: mpsc::Sender<WorkerResultBox>,
        repaint_context: Option<egui::Context>,
    ) -> Self {
        Self {
            sender,
            repaint_context,
        }
    }

    /// Await queue capacity instead of blocking a Tokio worker thread. The await is cancellation
    /// safe, so tearing down a watcher always releases it even while the UI is busy.
    pub async fn send<R: WorkerResult + 'static>(
        &self,
        result: R,
    ) -> Result<(), mpsc::error::SendError<WorkerResultBox>> {
        self.send_box(Box::new(result)).await
    }

    pub async fn send_box(
        &self,
        result: WorkerResultBox,
    ) -> Result<(), mpsc::error::SendError<WorkerResultBox>> {
        self.sender.send(result).await?;
        if let Some(context) = &self.repaint_context {
            context.request_repaint();
        }
        Ok(())
    }
}

/// Kubernetes API status information retained for YAML editor feedback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceApiError {
    pub message: String,
    pub causes: Vec<ResourceApiErrorCause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceApiErrorCause {
    pub field: String,
    pub message: String,
    pub reason: String,
}

/// Shared state accessible from spawned async tasks
type SharedTaskRegistry<Key> = Arc<TaskRegistry<Key>>;

/// Owns one family of cancellable worker tasks.
///
/// Task removal happens while the registry is locked, but task abortion happens
/// after releasing that lock. This keeps lifecycle operations short and avoids
/// holding the registry lock across the bounded join in `abort_task`.
struct TaskRegistry<Key> {
    tasks: Mutex<HashMap<Key, JoinHandle<()>>>,
}

impl<Key> Default for TaskRegistry<Key> {
    fn default() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }
}

impl<Key> TaskRegistry<Key>
where
    Key: Eq + std::hash::Hash + Clone,
{
    async fn replace(&self, key: Key, task: JoinHandle<()>) {
        let previous = self.tasks.lock().await.insert(key, task);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
    }

    /// Replace a task whose creation must not overlap with the previous task.
    /// Detail watches and pod log streams use this because both can emit data
    /// immediately after they are spawned.
    async fn replace_after_abort(&self, key: Key, create: impl FnOnce() -> JoinHandle<()>) {
        self.abort(&key).await;
        self.replace(key, create()).await;
    }

    async fn abort(&self, key: &Key) {
        let task = self.tasks.lock().await.remove(key);
        if let Some(task) = task {
            abort_task(task).await;
        }
    }

    async fn abort_matching(&self, matches: impl Fn(&Key) -> bool) {
        let mut tasks = self.tasks.lock().await;
        let keys = tasks
            .keys()
            .filter(|key| matches(key))
            .cloned()
            .collect::<Vec<_>>();
        let removed = keys
            .into_iter()
            .filter_map(|key| tasks.remove(&key))
            .collect::<Vec<_>>();
        drop(tasks);
        for task in removed {
            abort_task(task).await;
        }
    }

    async fn abort_all(&self) {
        self.abort_matching(|_| true).await;
    }

    #[cfg(test)]
    async fn is_empty(&self) -> bool {
        self.tasks.lock().await.is_empty()
    }

    #[cfg(test)]
    async fn contains_key(&self, key: &Key) -> bool {
        self.tasks.lock().await.contains_key(key)
    }
}

#[derive(Clone)]
pub(crate) struct WorkerState {
    results: WorkerResultSender,
    /// Connected clusters and their root watcher tasks. This stays entirely on the
    /// worker side so UI state can never determine a Kubernetes task's lifetime.
    connections: Arc<Mutex<HashMap<i32, ClusterConnection>>>,
    /// Resource watches are keyed by their complete scope and are aborted before
    /// replacement and whenever their cluster session is torn down.
    resource_watches: Arc<Mutex<ResourceWatchRegistry>>,
    /// Detail watches remain active while their visit is retained in an
    /// inspector's history.
    detail_watches: SharedTaskRegistry<(i32, u64)>,
    /// Namespace-scoped Metrics API pollers used only while Pods are visible.
    pod_metrics_watches: SharedTaskRegistry<(i32, String)>,
    /// Cluster-scoped Metrics API pollers used only while Nodes are visible.
    node_metrics_watches: SharedTaskRegistry<i32>,
    /// Native log windows each own one cancellable follow stream.
    log_streams: SharedTaskRegistry<(i32, u64)>,
    /// Each connected cluster gets its own bounded pool for initial list/watch
    /// synchronization. A synchronized watch does not retain a permit.
    watch_initialization_slots: Arc<Mutex<HashMap<i32, Arc<Semaphore>>>>,
    /// The bounded, disk-backed ingress for pod logs. A Kubernetes stream
    /// awaits this directly rather than routing log data through the UI.
    log_store_appender: Option<LogStoreAppender>,
}

#[derive(Default)]
struct ResourceWatchRegistry {
    watches: HashMap<ResourceScope, JoinHandle<()>>,
    generations: HashMap<(i32, ApiResource), u64>,
    sessions: HashMap<i32, u64>,
}

impl WorkerState {
    async fn register_cluster_runtime(&self, cluster_key: i32) {
        self.resource_watches
            .lock()
            .await
            .sessions
            .entry(cluster_key)
            .or_insert(1);
        self.watch_initialization_slots
            .lock()
            .await
            .entry(cluster_key)
            .or_insert_with(|| Arc::new(Semaphore::new(16)));
    }

    async fn watch_initialization_slot(&self, cluster_key: i32) -> Arc<Semaphore> {
        self.watch_initialization_slots
            .lock()
            .await
            .entry(cluster_key)
            .or_insert_with(|| Arc::new(Semaphore::new(16)))
            .clone()
    }
    async fn client_for_cluster(&self, cluster_key: i32) -> anyhow::Result<kube::Client> {
        self.connections
            .lock()
            .await
            .get(&cluster_key)
            .map(ClusterConnection::client)
            .ok_or_else(|| anyhow::anyhow!("No client found for cluster_key {cluster_key}"))
    }

    async fn stop_cluster(&self, cluster_key: i32) {
        self.connections.lock().await.remove(&cluster_key);
        self.invalidate_cluster_resource_watches(cluster_key).await;
        self.detail_watches
            .abort_matching(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
        self.pod_metrics_watches
            .abort_matching(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
        self.node_metrics_watches
            .abort_matching(|watch_cluster_key| *watch_cluster_key == cluster_key)
            .await;
        self.log_streams
            .abort_matching(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
        self.watch_initialization_slots
            .lock()
            .await
            .remove(&cluster_key);
    }

    async fn stop_all_clusters(&self) {
        self.connections.lock().await.clear();
        self.invalidate_all_resource_watches().await;
        self.detail_watches.abort_all().await;
        self.pod_metrics_watches.abort_all().await;
        self.node_metrics_watches.abort_all().await;
        self.log_streams.abort_all().await;
        self.watch_initialization_slots.lock().await.clear();
    }

    #[cfg(test)]
    async fn replace_resource_watch(&self, key: ResourceScope, task: JoinHandle<()>) {
        let previous = self.resource_watches.lock().await.watches.insert(key, task);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
    }

    /// Advance a resource's generation and return all currently active tasks
    /// for it. Starts from an earlier generation check this value immediately
    /// before installing their watcher, so queued starts cannot resurrect an
    /// obsolete namespace scope.
    async fn replace_resource_watch_sources(
        &self,
        cluster_key: i32,
        api_resource: &ApiResource,
    ) -> u64 {
        let mut registry = self.resource_watches.lock().await;
        registry.sessions.entry(cluster_key).or_insert(1);
        let generation = {
            let generation = registry
                .generations
                .entry((cluster_key, api_resource.clone()))
                .and_modify(|generation| *generation += 1)
                .or_insert(1);
            *generation
        };
        let keys = registry
            .watches
            .keys()
            .filter(|scope| scope.cluster_key == cluster_key && scope.api_resource == *api_resource)
            .cloned()
            .collect::<Vec<_>>();
        let tasks = keys
            .into_iter()
            .filter_map(|key| registry.watches.remove(&key))
            .collect::<Vec<_>>();
        drop(registry);
        for task in tasks {
            abort_task(task).await;
        }
        generation
    }

    async fn install_resource_watch_if_current(
        &self,
        key: ResourceScope,
        generation: u64,
        session: u64,
        task: JoinHandle<()>,
    ) -> bool {
        let mut registry = self.resource_watches.lock().await;
        if registry.sessions.get(&key.cluster_key).copied() != Some(session)
            || registry
                .generations
                .get(&(key.cluster_key, key.api_resource.clone()))
                .copied()
                != Some(generation)
        {
            drop(registry);
            abort_task(task).await;
            return false;
        }
        let previous = registry.watches.insert(key, task);
        drop(registry);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
        true
    }

    async fn resource_watch_generation_is_current(
        &self,
        cluster_key: i32,
        api_resource: &ApiResource,
        generation: u64,
        session: u64,
    ) -> bool {
        let registry = self.resource_watches.lock().await;
        registry.sessions.get(&cluster_key).copied() == Some(session)
            && registry
                .generations
                .get(&(cluster_key, api_resource.clone()))
                .copied()
                == Some(generation)
    }

    async fn resource_watch_session(&self, cluster_key: i32) -> u64 {
        *self
            .resource_watches
            .lock()
            .await
            .sessions
            .entry(cluster_key)
            .or_insert(1)
    }

    async fn invalidate_cluster_resource_watches(&self, cluster_key: i32) {
        let mut registry = self.resource_watches.lock().await;
        *registry.sessions.entry(cluster_key).or_insert(1) += 1;
        registry
            .generations
            .retain(|(key, _), _| *key != cluster_key);
        let keys = registry
            .watches
            .keys()
            .filter(|scope| scope.cluster_key == cluster_key)
            .cloned()
            .collect::<Vec<_>>();
        let tasks = keys
            .into_iter()
            .filter_map(|key| registry.watches.remove(&key))
            .collect::<Vec<_>>();
        drop(registry);
        for task in tasks {
            abort_task(task).await;
        }
    }

    async fn invalidate_all_resource_watches(&self) {
        let cluster_keys = {
            let registry = self.resource_watches.lock().await;
            registry
                .sessions
                .keys()
                .chain(registry.watches.keys().map(|scope| &scope.cluster_key))
                .copied()
                .collect::<std::collections::HashSet<_>>()
        };
        for cluster_key in cluster_keys {
            self.invalidate_cluster_resource_watches(cluster_key).await;
        }
    }

    async fn replace_pod_metrics_watch(&self, key: (i32, String), task: JoinHandle<()>) {
        self.pod_metrics_watches.replace(key, task).await;
    }

    async fn replace_node_metrics_watch(&self, key: i32, task: JoinHandle<()>) {
        self.node_metrics_watches.replace(key, task).await;
    }
}

async fn abort_task(task: JoinHandle<()>) {
    task.abort();
    // A watcher may be in a synchronous result-channel send when cancellation
    // is requested. Bound the join so teardown cannot stall the command loop.
    let _ = tokio::time::timeout(Duration::from_millis(100), task).await;
}

struct WorkerRuntime {
    receiver: mpsc::Receiver<WorkerCommandBox>,
    state: Arc<WorkerState>,
}

enum ClusterRuntimeMessage {
    Command(WorkerCommandBox),
    Shutdown(oneshot::Sender<()>),
}

struct ClusterRuntimeHandle {
    sender: mpsc::UnboundedSender<ClusterRuntimeMessage>,
    thread: std::thread::JoinHandle<()>,
}

impl WorkerRuntime {
    fn run(mut self) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        info!("Worker thread running");
        let mut clusters = HashMap::<i32, ClusterRuntimeHandle>::new();
        while let Some(command) = self.receiver.blocking_recv() {
            if command.as_ref().as_any().is::<LoadClusters>() {
                shutdown_cluster_runtimes(&runtime, &self.state, &mut clusters);
                runtime.block_on(dispatch_command(command, self.state.clone()));
            } else if let Some(cluster_key) = command.cluster_key() {
                let handle = clusters.entry(cluster_key).or_insert_with(|| {
                    ClusterRuntimeHandle::start(cluster_key, self.state.clone())
                });
                if handle
                    .sender
                    .send(ClusterRuntimeMessage::Command(command))
                    .is_err()
                {
                    tracing::error!(cluster_key, "Cluster worker command channel closed");
                }
            } else {
                let state = self.state.clone();
                runtime.spawn(dispatch_command(command, state));
            }
        }
        shutdown_cluster_runtimes(&runtime, &self.state, &mut clusters);
    }
}

impl ClusterRuntimeHandle {
    fn start(cluster_key: i32, state: Arc<WorkerState>) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to build cluster worker runtime");
            runtime.block_on(async { state.register_cluster_runtime(cluster_key).await });
            ClusterWorkerRuntime {
                cluster_key,
                receiver,
                state,
            }
            .run(runtime);
        });
        Self { sender, thread }
    }
}

struct ClusterWorkerRuntime {
    cluster_key: i32,
    receiver: mpsc::UnboundedReceiver<ClusterRuntimeMessage>,
    state: Arc<WorkerState>,
}

impl ClusterWorkerRuntime {
    fn run(mut self, runtime: tokio::runtime::Runtime) {
        let mut in_flight = Vec::<JoinHandle<()>>::new();
        while let Some(message) = self.receiver.blocking_recv() {
            match message {
                ClusterRuntimeMessage::Command(command)
                    if command.serializes_session_lifecycle() =>
                {
                    drain_commands(&runtime, &mut in_flight);
                    runtime.block_on(dispatch_command(command, self.state.clone()));
                }
                ClusterRuntimeMessage::Command(command) => {
                    in_flight.retain(|task| !task.is_finished());
                    let state = self.state.clone();
                    in_flight.push(runtime.spawn(dispatch_command(command, state)));
                }
                ClusterRuntimeMessage::Shutdown(done) => {
                    drain_commands(&runtime, &mut in_flight);
                    runtime.block_on(self.state.stop_cluster(self.cluster_key));
                    let _ = done.send(());
                    return;
                }
            }
        }
        drain_commands(&runtime, &mut in_flight);
        runtime.block_on(self.state.stop_cluster(self.cluster_key));
    }
}

fn drain_commands(runtime: &tokio::runtime::Runtime, tasks: &mut Vec<JoinHandle<()>>) {
    let pending = std::mem::take(tasks);
    runtime.block_on(async {
        for task in pending {
            let _ = task.await;
        }
    });
}

fn shutdown_cluster_runtimes(
    runtime: &tokio::runtime::Runtime,
    state: &Arc<WorkerState>,
    clusters: &mut HashMap<i32, ClusterRuntimeHandle>,
) {
    let handles = std::mem::take(clusters);
    for (cluster_key, handle) in handles {
        let (done_sender, done_receiver) = oneshot::channel();
        if handle
            .sender
            .send(ClusterRuntimeMessage::Shutdown(done_sender))
            .is_ok()
        {
            let _ = runtime.block_on(done_receiver);
        } else {
            runtime.block_on(state.stop_cluster(cluster_key));
        }
        let _ = handle.thread.join();
    }
}

async fn dispatch_command(command: WorkerCommandBox, state: Arc<WorkerState>) {
    if let Some(result) = command.execute_boxed(&state).await {
        state
            .results
            .send_box(result)
            .await
            .log_if_error("Failed to send worker result");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    struct AbortProbe(Arc<AtomicUsize>);

    impl Future for AbortProbe {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }

    impl Drop for AbortProbe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn worker_state() -> WorkerState {
        let (sender, _receiver) = mpsc::channel(1);
        WorkerState {
            results: WorkerResultSender::new(sender, None),
            connections: Arc::new(Mutex::new(HashMap::new())),
            resource_watches: Arc::new(Mutex::new(ResourceWatchRegistry::default())),
            detail_watches: Arc::new(TaskRegistry::default()),
            pod_metrics_watches: Arc::new(TaskRegistry::default()),
            node_metrics_watches: Arc::new(TaskRegistry::default()),
            log_streams: Arc::new(TaskRegistry::default()),
            watch_initialization_slots: Arc::new(Mutex::new(HashMap::new())),
            log_store_appender: None,
        }
    }

    fn pod_resource() -> ApiResource {
        ApiResource {
            group: "core".to_owned(),
            version: "v1".to_owned(),
            kind: "Pod".to_owned(),
            name: "pods".to_owned(),
            namespaced: true,
        }
    }

    #[test]
    fn task_registry_replaces_and_selectively_aborts_tasks() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let registry = TaskRegistry::default();
            let first_aborted = Arc::new(AtomicUsize::new(0));
            let second_aborted = Arc::new(AtomicUsize::new(0));
            let retained_aborted = Arc::new(AtomicUsize::new(0));

            registry
                .replace(1, tokio::spawn(AbortProbe(first_aborted.clone())))
                .await;
            registry
                .replace_after_abort(1, || tokio::spawn(AbortProbe(second_aborted.clone())))
                .await;
            registry
                .replace(2, tokio::spawn(AbortProbe(retained_aborted.clone())))
                .await;
            tokio::task::yield_now().await;

            assert_eq!(first_aborted.load(Ordering::Relaxed), 1);
            registry.abort_matching(|key| *key == 1).await;
            tokio::task::yield_now().await;

            assert_eq!(second_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(retained_aborted.load(Ordering::Relaxed), 0);
            assert!(registry.contains_key(&2).await);
            registry.abort(&2).await;
            tokio::task::yield_now().await;
            assert_eq!(retained_aborted.load(Ordering::Relaxed), 1);
            assert!(registry.is_empty().await);
        });
    }

    #[test]
    fn worker_results_request_a_repaint_when_context_is_attached() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        let context = egui::Context::default();
        let repaint_count = Arc::new(AtomicUsize::new(0));
        let repaint_count_for_callback = repaint_count.clone();
        context.set_request_repaint_callback(move |_| {
            repaint_count_for_callback.fetch_add(1, Ordering::Relaxed);
        });
        let (sender, mut receiver) = mpsc::channel(1);
        let result_sender = WorkerResultSender::new(sender, Some(context));

        runtime.block_on(async {
            result_sender
                .send(PodLogStreamEnded { log_window_id: 2 })
                .await
                .expect("result receiver is open");
        });

        assert_eq!(
            receiver
                .try_recv()
                .expect("result is queued")
                .as_ref()
                .as_any()
                .downcast_ref::<PodLogStreamEnded>()
                .map(|result| result.log_window_id),
            Some(2)
        );
        assert_eq!(repaint_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn full_command_channel_queues_without_blocking_the_ui() {
        let (command_sender, _command_receiver) = mpsc::channel::<WorkerCommandBox>(1);
        command_sender
            .try_send(Box::new(LoadClusters))
            .expect("channel starts empty");
        let (_result_sender, result_receiver) = mpsc::channel(1);
        let mut worker = Worker {
            inner: Some(WorkerInner {
                sender: command_sender,
                receiver: result_receiver,
            }),
            pending_commands: VecDeque::new(),
            repaint_context: None,
            log_store_appender: None,
        };

        worker.send_command(Box::new(LoadClusters));

        assert_eq!(worker.pending_commands.len(), 1);
    }

    #[test]
    fn pending_commands_are_forwarded_in_order_after_capacity_returns() {
        let (command_sender, mut command_receiver) = mpsc::channel::<WorkerCommandBox>(1);
        command_sender
            .try_send(Box::new(LoadClusters))
            .expect("channel starts empty");
        let (_result_sender, result_receiver) = mpsc::channel(1);
        let mut worker = Worker {
            inner: Some(WorkerInner {
                sender: command_sender,
                receiver: result_receiver,
            }),
            pending_commands: VecDeque::new(),
            repaint_context: None,
            log_store_appender: None,
        };
        worker.send_command(Box::new(StopPodLogStream {
            cluster_key: 1,
            log_window_id: 2,
        }));
        worker.send_command(Box::new(StopResourceDetailWatch {
            cluster_key: 1,
            history_entry_id: 3,
        }));

        assert!(
            command_receiver
                .try_recv()
                .expect("queued command is available")
                .as_ref()
                .as_any()
                .downcast_ref::<LoadClusters>()
                .is_some()
        );
        let _ = worker.get_next_message();
        assert!(
            command_receiver
                .try_recv()
                .expect("queued command is available")
                .as_ref()
                .as_any()
                .downcast_ref::<StopPodLogStream>()
                .is_some_and(|command| command.cluster_key == 1 && command.log_window_id == 2)
        );
        let _ = worker.get_next_message();
        assert!(
            command_receiver
                .try_recv()
                .expect("queued command is available")
                .as_ref()
                .as_any()
                .downcast_ref::<StopResourceDetailWatch>()
                .is_some_and(|command| command.cluster_key == 1 && command.history_entry_id == 3)
        );
    }

    #[test]
    fn lifecycle_command_waits_for_result_capacity_to_preserve_delivery_order() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let (result_channel_sender, mut result_receiver) = mpsc::channel(1);
            result_channel_sender
                .try_send(Box::new(PodLogStreamEnded { log_window_id: 1 }) as WorkerResultBox)
                .expect("channel starts empty");
            let state = Arc::new(WorkerState {
                results: WorkerResultSender::new(result_channel_sender, None),
                connections: Arc::new(Mutex::new(HashMap::new())),
                resource_watches: Arc::new(Mutex::new(ResourceWatchRegistry::default())),
                detail_watches: Arc::new(TaskRegistry::default()),
                pod_metrics_watches: Arc::new(TaskRegistry::default()),
                node_metrics_watches: Arc::new(TaskRegistry::default()),
                log_streams: Arc::new(TaskRegistry::default()),
                watch_initialization_slots: Arc::new(Mutex::new(HashMap::new())),
                log_store_appender: None,
            });

            let dispatch = tokio::spawn(dispatch_command(
                Box::new(StopPodLogStream {
                    cluster_key: 1,
                    log_window_id: 2,
                }),
                state,
            ));
            assert!(
                tokio::time::timeout(Duration::from_millis(25), dispatch)
                    .await
                    .is_err(),
                "the lifecycle command must not overtake the queued result"
            );

            assert_eq!(
                result_receiver
                    .try_recv()
                    .expect("initial result is queued")
                    .as_ref()
                    .as_any()
                    .downcast_ref::<PodLogStreamEnded>()
                    .map(|result| result.log_window_id),
                Some(1)
            );
            assert_eq!(
                tokio::time::timeout(Duration::from_millis(100), result_receiver.recv())
                    .await
                    .expect("dispatch should finish after capacity returns")
                    .expect("result notification")
                    .as_ref()
                    .as_any()
                    .downcast_ref::<<StopPodLogStream as WorkerCommand>::Output>()
                    .and_then(|result| result.as_ref().ok())
                    .map(|result| result.log_window_id),
                Some(2)
            );
        });
    }

    #[test]
    fn successful_worker_local_commands_do_not_emit_channel_results() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        let result = runtime.block_on(
            Box::new(StopResourceDetailWatch {
                cluster_key: 1,
                history_entry_id: 2,
            })
            .execute_boxed(&worker_state()),
        );

        assert!(result.is_none());
    }

    #[test]
    fn failed_worker_local_commands_emit_their_typed_failure() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        let result = runtime.block_on(
            Box::new(StartResourceDetailWatch {
                cluster_key: 7,
                history_entry_id: 8,
                api_resource: pod_resource(),
                namespace: Some("default".to_owned()),
                resource_name: "missing".to_owned(),
                resource_uid: "uid".to_owned(),
                pod_metrics_api_available: true,
                node_metrics_api_available: true,
            })
            .execute_boxed(&worker_state()),
        );

        assert_eq!(
            result
                .expect("failure is forwarded to the UI")
                .as_ref()
                .as_any()
                .downcast_ref::<ResourceDetailWatchFailed>()
                .map(|failure| (
                    failure.cluster_key,
                    failure.history_entry_id,
                    failure.events
                )),
            Some((7, 8, false))
        );
    }

    #[derive(Debug)]
    struct QueuesLoadClusters;

    impl WorkerResult for QueuesLoadClusters {
        fn apply(self, _ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>) {
            commands.push(Box::new(LoadClusters));
        }
    }

    #[derive(Debug)]
    struct QueuesStopLogs;

    impl WorkerResult for QueuesStopLogs {
        fn apply(self, _ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>) {
            commands.push(Box::new(StopPodLogStream {
                cluster_key: 1,
                log_window_id: 2,
            }));
        }
    }

    #[test]
    fn erased_result_adapter_applies_both_result_branches() {
        let mut ui = crate::ui::state::UiState::default();
        let mut commands = Vec::new();
        let success: WorkerResultBox =
            Box::new(Ok::<QueuesLoadClusters, QueuesStopLogs>(QueuesLoadClusters));
        success.apply_boxed(&mut ui, &mut commands);
        assert!(
            commands[0]
                .as_ref()
                .as_any()
                .downcast_ref::<LoadClusters>()
                .is_some()
        );

        let failure: WorkerResultBox =
            Box::new(Err::<QueuesLoadClusters, QueuesStopLogs>(QueuesStopLogs));
        failure.apply_boxed(&mut ui, &mut commands);
        assert!(
            commands[1]
                .as_ref()
                .as_any()
                .downcast_ref::<StopPodLogStream>()
                .is_some()
        );
    }

    #[test]
    fn waiting_for_result_capacity_is_cancellable() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let (sender, mut receiver) = mpsc::channel(1);
            sender
                .try_send(Box::new(PodLogStreamEnded { log_window_id: 1 }) as WorkerResultBox)
                .expect("channel starts empty");
            let result_sender = WorkerResultSender::new(sender, None);
            let task = tokio::spawn(async move {
                result_sender
                    .send(PodLogStreamEnded { log_window_id: 2 })
                    .await
            });
            tokio::task::yield_now().await;
            assert!(!task.is_finished());

            task.abort();
            assert!(task.await.expect_err("task was aborted").is_cancelled());
            assert_eq!(
                receiver
                    .try_recv()
                    .expect("initial result is queued")
                    .as_ref()
                    .as_any()
                    .downcast_ref::<PodLogStreamEnded>()
                    .map(|result| result.log_window_id),
                Some(1)
            );
        });
    }

    #[test]
    fn replacing_a_resource_watch_aborts_the_previous_task() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = worker_state();
            let aborted = Arc::new(AtomicUsize::new(0));
            let key = ResourceScope {
                cluster_key: 1,
                api_resource: pod_resource(),
                namespace: Some("default".to_owned()),
            };

            state
                .replace_resource_watch(key.clone(), tokio::spawn(AbortProbe(aborted.clone())))
                .await;
            state
                .replace_resource_watch(key, tokio::spawn(std::future::pending()))
                .await;
            tokio::task::yield_now().await;

            assert_eq!(aborted.load(Ordering::Relaxed), 1);
        });
    }

    #[test]
    fn reconciliation_or_cluster_teardown_prevents_a_queued_start_from_installing() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = worker_state();
            let resource = pod_resource();
            let obsolete_generation = state.replace_resource_watch_sources(1, &resource).await;
            let current_generation = state.replace_resource_watch_sources(1, &resource).await;
            let session = state.resource_watch_session(1).await;
            let aborted = Arc::new(AtomicUsize::new(0));

            assert!(
                !state
                    .install_resource_watch_if_current(
                        ResourceScope {
                            cluster_key: 1,
                            api_resource: resource,
                            namespace: Some("default".to_owned()),
                        },
                        obsolete_generation,
                        session,
                        tokio::spawn(AbortProbe(aborted.clone())),
                    )
                    .await
            );
            tokio::task::yield_now().await;

            assert_eq!(current_generation, obsolete_generation + 1);
            assert_eq!(aborted.load(Ordering::Relaxed), 1);
            assert!(state.resource_watches.lock().await.watches.is_empty());

            state.invalidate_cluster_resource_watches(1).await;
            assert!(
                !state
                    .resource_watch_generation_is_current(
                        1,
                        &pod_resource(),
                        current_generation,
                        session
                    )
                    .await
            );
        });
    }

    #[test]
    fn replacing_a_pod_metrics_watch_aborts_the_previous_task() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = worker_state();
            let aborted = Arc::new(AtomicUsize::new(0));
            let key = (1, "default".to_owned());

            state
                .replace_pod_metrics_watch(key.clone(), tokio::spawn(AbortProbe(aborted.clone())))
                .await;
            state
                .replace_pod_metrics_watch(key, tokio::spawn(std::future::pending()))
                .await;
            tokio::task::yield_now().await;

            assert_eq!(aborted.load(Ordering::Relaxed), 1);
        });
    }

    #[test]
    fn replacing_a_node_metrics_watch_aborts_the_previous_task() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = worker_state();
            let aborted = Arc::new(AtomicUsize::new(0));

            state
                .replace_node_metrics_watch(1, tokio::spawn(AbortProbe(aborted.clone())))
                .await;
            state
                .replace_node_metrics_watch(1, tokio::spawn(std::future::pending()))
                .await;
            tokio::task::yield_now().await;

            assert_eq!(aborted.load(Ordering::Relaxed), 1);
        });
    }

    #[test]
    fn stopping_all_clusters_aborts_supervised_tasks() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = worker_state();
            let resource_aborted = Arc::new(AtomicUsize::new(0));
            let detail_aborted = Arc::new(AtomicUsize::new(0));
            let metrics_aborted = Arc::new(AtomicUsize::new(0));
            let node_metrics_aborted = Arc::new(AtomicUsize::new(0));
            let log_aborted = Arc::new(AtomicUsize::new(0));

            state
                .replace_resource_watch(
                    ResourceScope {
                        cluster_key: 1,
                        api_resource: pod_resource(),
                        namespace: Some("default".to_owned()),
                    },
                    tokio::spawn(AbortProbe(resource_aborted.clone())),
                )
                .await;
            state
                .detail_watches
                .replace((1, 3), tokio::spawn(AbortProbe(detail_aborted.clone())))
                .await;
            state
                .pod_metrics_watches
                .replace(
                    (1, "default".to_owned()),
                    tokio::spawn(AbortProbe(metrics_aborted.clone())),
                )
                .await;
            state
                .node_metrics_watches
                .replace(1, tokio::spawn(AbortProbe(node_metrics_aborted.clone())))
                .await;
            state
                .log_streams
                .replace((1, 4), tokio::spawn(AbortProbe(log_aborted.clone())))
                .await;

            state.stop_all_clusters().await;
            tokio::task::yield_now().await;

            assert_eq!(resource_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(detail_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(metrics_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(node_metrics_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(log_aborted.load(Ordering::Relaxed), 1);
            assert!(state.resource_watches.lock().await.watches.is_empty());
            assert!(state.detail_watches.is_empty().await);
            assert!(state.pod_metrics_watches.is_empty().await);
            assert!(state.node_metrics_watches.is_empty().await);
            assert!(state.log_streams.is_empty().await);
        });
    }

    #[test]
    fn stopping_one_cluster_preserves_other_cluster_tasks() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = worker_state();
            let first_aborted = Arc::new(AtomicUsize::new(0));
            let second_aborted = Arc::new(AtomicUsize::new(0));
            let first_metrics_aborted = Arc::new(AtomicUsize::new(0));
            let second_metrics_aborted = Arc::new(AtomicUsize::new(0));
            let first_node_metrics_aborted = Arc::new(AtomicUsize::new(0));
            let second_node_metrics_aborted = Arc::new(AtomicUsize::new(0));
            let first_detail_aborted = Arc::new(AtomicUsize::new(0));
            let second_detail_aborted = Arc::new(AtomicUsize::new(0));
            let first_log_aborted = Arc::new(AtomicUsize::new(0));
            let second_log_aborted = Arc::new(AtomicUsize::new(0));
            state
                .replace_resource_watch(
                    ResourceScope {
                        cluster_key: 1,
                        api_resource: pod_resource(),
                        namespace: Some("default".to_owned()),
                    },
                    tokio::spawn(AbortProbe(first_aborted.clone())),
                )
                .await;
            state
                .replace_resource_watch(
                    ResourceScope {
                        cluster_key: 2,
                        api_resource: pod_resource(),
                        namespace: Some("default".to_owned()),
                    },
                    tokio::spawn(AbortProbe(second_aborted.clone())),
                )
                .await;

            state
                .replace_pod_metrics_watch(
                    (1, "default".to_owned()),
                    tokio::spawn(AbortProbe(first_metrics_aborted.clone())),
                )
                .await;
            state
                .replace_node_metrics_watch(
                    1,
                    tokio::spawn(AbortProbe(first_node_metrics_aborted.clone())),
                )
                .await;
            state
                .replace_node_metrics_watch(
                    2,
                    tokio::spawn(AbortProbe(second_node_metrics_aborted.clone())),
                )
                .await;
            state
                .replace_pod_metrics_watch(
                    (2, "default".to_owned()),
                    tokio::spawn(AbortProbe(second_metrics_aborted.clone())),
                )
                .await;

            state
                .detail_watches
                .replace(
                    (1, 10),
                    tokio::spawn(AbortProbe(first_detail_aborted.clone())),
                )
                .await;
            state
                .detail_watches
                .replace(
                    (2, 10),
                    tokio::spawn(AbortProbe(second_detail_aborted.clone())),
                )
                .await;
            state
                .log_streams
                .replace((1, 11), tokio::spawn(AbortProbe(first_log_aborted.clone())))
                .await;
            state
                .log_streams
                .replace(
                    (2, 11),
                    tokio::spawn(AbortProbe(second_log_aborted.clone())),
                )
                .await;

            state.stop_cluster(1).await;

            assert_eq!(first_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(second_aborted.load(Ordering::Relaxed), 0);
            assert_eq!(first_metrics_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(second_metrics_aborted.load(Ordering::Relaxed), 0);
            assert_eq!(first_node_metrics_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(second_node_metrics_aborted.load(Ordering::Relaxed), 0);
            assert_eq!(first_detail_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(second_detail_aborted.load(Ordering::Relaxed), 0);
            assert_eq!(first_log_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(second_log_aborted.load(Ordering::Relaxed), 0);
            assert!(
                state
                    .resource_watches
                    .lock()
                    .await
                    .watches
                    .contains_key(&ResourceScope {
                        cluster_key: 2,
                        api_resource: pod_resource(),
                        namespace: Some("default".to_owned()),
                    })
            );
            assert!(
                state
                    .pod_metrics_watches
                    .contains_key(&(2, "default".to_owned()))
                    .await
            );
            assert!(state.node_metrics_watches.contains_key(&2).await);
            assert!(state.detail_watches.contains_key(&(2, 10)).await);
            assert!(state.log_streams.contains_key(&(2, 11)).await);
        });
    }

    #[test]
    fn sensitive_commands_omit_resource_data_values_from_debug_output() {
        let command = UpdateResourceData {
            cluster_key: 7,
            history_entry_id: 12,
            request_id: 34,
            api_resource: pod_resource(),
            namespace: "default".to_owned(),
            resource_name: "credentials".to_owned(),
            update: ResourceDataUpdate {
                expected_resource_version: "42".to_owned(),
                expected_values: BTreeMap::from([("token".to_owned(), "old-secret".to_owned())]),
                updated_values: BTreeMap::from([("token".to_owned(), "new-secret".to_owned())]),
            },
        };
        assert!(!format!("{command:?}").contains("secret"));
    }

    #[test]
    fn yaml_commands_omit_document_text_from_debug_output() {
        let secret_yaml = "data:\n  token: definitely-secret".to_owned();
        let apply = ApplyResourceYaml {
            editor_id: 9,
            cluster_key: 7,
            api_resource: pod_resource(),
            namespace: Some("default".to_owned()),
            resource_name: "credentials".to_owned(),
            yaml: secret_yaml.clone(),
        };
        let validation = ValidateResourceYaml {
            editor_id: 9,
            revision: 4,
            cluster_key: 7,
            api_resource: pod_resource(),
            namespace: Some("default".to_owned()),
            resource_name: "credentials".to_owned(),
            yaml: secret_yaml,
        };
        assert!(!format!("{apply:?}{validation:?}").contains("definitely-secret"));
    }

    #[test]
    fn cluster_lifecycle_commands_are_serialized_and_watch_reconciliation_is_scoped() {
        let load_clusters: WorkerCommandBox = Box::new(LoadClusters);
        assert!(load_clusters.serializes_session_lifecycle());
        assert_eq!(load_clusters.cluster_key(), None);
        let stop_logs = StopPodLogStream {
            cluster_key: 1,
            log_window_id: 1,
        };
        let stop_logs: WorkerCommandBox = Box::new(stop_logs);
        assert!(!stop_logs.serializes_session_lifecycle());
        assert_eq!(stop_logs.cluster_key(), Some(1));
        let reconcile: WorkerCommandBox = Box::new(ReconcileResourceWatches {
            cluster_key: 1,
            api_resource: pod_resource(),
            sources: vec![ResourceWatchSource::Namespace("default".to_owned())],
        });
        assert!(!reconcile.serializes_session_lifecycle());
        assert_eq!(reconcile.cluster_key(), Some(1));
        let get_yaml = GetResourceYaml {
            editor_id: 1,
            cluster_key: 1,
            api_resource: pod_resource(),
            namespace: Some("default".to_owned()),
            resource_name: "pod".to_owned(),
        };
        let get_yaml: WorkerCommandBox = Box::new(get_yaml);
        assert!(!get_yaml.serializes_session_lifecycle());
        assert_eq!(get_yaml.cluster_key(), Some(1));
    }

    #[test]
    fn watch_initialization_slots_are_limited_per_cluster() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = worker_state();
            let first = state.watch_initialization_slot(1).await;
            let second = state.watch_initialization_slot(2).await;
            let permits = (0..16)
                .map(|_| {
                    first
                        .clone()
                        .try_acquire_owned()
                        .expect("slot is available")
                })
                .collect::<Vec<_>>();
            assert!(first.try_acquire().is_err());
            assert!(second.try_acquire().is_ok());
            drop(permits);
            assert!(first.try_acquire().is_ok());
        });
    }
}
