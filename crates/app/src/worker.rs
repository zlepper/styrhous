use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    Cluster, ClusterConnection, ResourceDataUpdateRequest, ResourceDetailWatchRequest,
    ResourceYamlValidationRequest, apply_resource_yaml, delete_resource, force_delete_resource,
    get_resource_scale, get_resource_schema, get_resource_yaml, reload_kubeconfig,
    restart_deployment, start_cluster_connection, start_resource_watcher, update_resource_data,
    update_resource_scale, validate_resource_yaml, watch_node_metrics, watch_pod_metrics_namespace,
    watch_resource_detail,
};
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
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
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
}

/// The channel-only, object-safe adapter for concrete commands.
#[async_trait]
pub(crate) trait ErasedWorkerCommand: AsAny + Send + std::fmt::Debug {
    async fn execute_boxed(self: Box<Self>, state: &WorkerState) -> Option<WorkerResultBox>;
    fn serializes_session_lifecycle(&self) -> bool;
}

#[async_trait]
impl<C: WorkerCommand> ErasedWorkerCommand for C {
    async fn execute_boxed(self: Box<Self>, state: &WorkerState) -> Option<WorkerResultBox> {
        (*self).execute(state).await.into_result_box()
    }

    fn serializes_session_lifecycle(&self) -> bool {
        WorkerCommand::serializes_session_lifecycle(self)
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
                connections: Mutex::new(HashMap::new()),
                resource_watches: Mutex::new(HashMap::new()),
                detail_watches: Mutex::new(HashMap::new()),
                pod_metrics_watches: Mutex::new(HashMap::new()),
                node_metrics_watches: Mutex::new(HashMap::new()),
                log_streams: Mutex::new(HashMap::new()),
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
pub(crate) struct ConnectToCluster {
    pub(crate) cluster: String,
    pub(crate) cluster_key: i32,
}
#[derive(Debug)]
pub(crate) struct StartResourceWatch {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
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
#[derive(Debug)]
pub(crate) struct KubernetesResourceWatchFailed {
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
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
}

#[async_trait]
impl WorkerCommand for StartResourceWatch {
    type Output = Result<KubernetesResourceWatchStarted, KubernetesResourceWatchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = KubernetesResourceWatchFailed {
            cluster_key: self.cluster_key,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
            error: String::new(),
        };
        let client = match state.client_for_cluster(self.cluster_key).await {
            Ok(client) => client,
            Err(error) => {
                return Err(KubernetesResourceWatchFailed {
                    error: format!("{error:#?}"),
                    ..failure
                });
            }
        };
        let key = ResourceScope {
            cluster_key: self.cluster_key,
            api_resource: self.api_resource.clone(),
            namespace: self.namespace.clone(),
        };
        match start_resource_watcher(
            self.cluster_key,
            client,
            self.api_resource,
            self.namespace,
            state.results.clone(),
        )
        .await
        {
            Ok((result, task)) => {
                state.replace_resource_watch(key, task).await;
                Ok(result)
            }
            Err(error) => Err(KubernetesResourceWatchFailed {
                error: format!("{error:#?}"),
                ..failure
            }),
        }
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
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
        let previous = state.detail_watches.lock().await.remove(&key);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
        let handle = tokio::spawn(watch_resource_detail(ResourceDetailWatchRequest {
            cluster_key: self.cluster_key,
            client,
            api_resource: self.api_resource,
            namespace: self.namespace,
            resource_name: self.resource_name,
            resource_uid: self.resource_uid,
            history_entry_id: self.history_entry_id,
            pod_metrics_api_available: self.pod_metrics_api_available,
            node_metrics_api_available: self.node_metrics_api_available,
            event_sender: state.results.clone(),
        }));
        state.detail_watches.lock().await.insert(key, handle);
        Ok(NoResult)
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
    }
}

#[async_trait]
impl WorkerCommand for StopResourceDetailWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let handle = state
            .detail_watches
            .lock()
            .await
            .remove(&(self.cluster_key, self.history_entry_id));
        if let Some(handle) = handle {
            abort_task(handle).await;
        }
        Ok(NoResult)
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
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

    fn serializes_session_lifecycle(&self) -> bool {
        true
    }
}

#[async_trait]
impl WorkerCommand for StopPodMetricsWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .abort_pod_metrics_watches(|key| key.0 == self.cluster_key && key.1 == self.namespace)
            .await;
        Ok(NoResult)
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
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

    fn serializes_session_lifecycle(&self) -> bool {
        true
    }
}

#[async_trait]
impl WorkerCommand for StopNodeMetricsWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .abort_node_metrics_watches(|cluster_key| *cluster_key == self.cluster_key)
            .await;
        Ok(NoResult)
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
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
        let previous = state.log_streams.lock().await.remove(&key);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
        let task = tokio::spawn(pod_logs::stream(
            self.log_window_id,
            client,
            self.namespace,
            self.pod_name,
            self.container,
            log_store_appender,
            state.results.clone(),
        ));
        state.log_streams.lock().await.insert(key, task);
        Ok(PodLogStreamStarted {
            log_window_id: self.log_window_id,
        })
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
    }
}

#[async_trait]
impl WorkerCommand for StopPodLogStream {
    type Output = Result<PodLogStreamEnded, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let task = state
            .log_streams
            .lock()
            .await
            .remove(&(self.cluster_key, self.log_window_id));
        if let Some(task) = task {
            abort_task(task).await;
        }
        Ok(PodLogStreamEnded {
            log_window_id: self.log_window_id,
        })
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
    }
}

#[derive(Clone)]
pub struct WorkerResultSender {
    sender: mpsc::Sender<WorkerResultBox>,
    repaint_context: Option<egui::Context>,
}

impl WorkerResultSender {
    fn new(sender: mpsc::Sender<WorkerResultBox>, repaint_context: Option<egui::Context>) -> Self {
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
pub(crate) struct WorkerState {
    results: WorkerResultSender,
    /// Connected clusters and their root watcher tasks. This stays entirely on the
    /// worker side so UI state can never determine a Kubernetes task's lifetime.
    connections: Mutex<HashMap<i32, ClusterConnection>>,
    /// Resource watches are keyed by their complete scope and are aborted before
    /// replacement and whenever their cluster session is torn down.
    resource_watches: Mutex<HashMap<ResourceScope, JoinHandle<()>>>,
    /// Detail watches remain active while their visit is retained in an
    /// inspector's history.
    detail_watches: Mutex<HashMap<(i32, u64), JoinHandle<()>>>,
    /// Namespace-scoped Metrics API pollers used only while Pods are visible.
    pod_metrics_watches: Mutex<HashMap<(i32, String), JoinHandle<()>>>,
    /// Cluster-scoped Metrics API pollers used only while Nodes are visible.
    node_metrics_watches: Mutex<HashMap<i32, JoinHandle<()>>>,
    /// Native log windows each own one cancellable follow stream.
    log_streams: Mutex<HashMap<(i32, u64), JoinHandle<()>>>,
    /// The bounded, disk-backed ingress for pod logs. A Kubernetes stream
    /// awaits this directly rather than routing log data through the UI.
    log_store_appender: Option<LogStoreAppender>,
}

impl WorkerState {
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
        self.abort_resource_watches(|scope| scope.cluster_key == cluster_key)
            .await;
        self.abort_detail_watches(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
        self.abort_pod_metrics_watches(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
        self.abort_node_metrics_watches(|watch_cluster_key| *watch_cluster_key == cluster_key)
            .await;
        self.abort_log_streams(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
    }

    async fn stop_all_clusters(&self) {
        self.connections.lock().await.clear();
        self.abort_resource_watches(|_| true).await;
        self.abort_detail_watches(|_| true).await;
        self.abort_pod_metrics_watches(|_| true).await;
        self.abort_node_metrics_watches(|_| true).await;
        self.abort_log_streams(|_| true).await;
    }

    async fn replace_resource_watch(&self, key: ResourceScope, task: JoinHandle<()>) {
        let previous = self.resource_watches.lock().await.insert(key, task);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
    }

    async fn abort_resource_watches(&self, matches: impl Fn(&ResourceScope) -> bool) {
        let mut watches = self.resource_watches.lock().await;
        let keys = watches
            .keys()
            .filter(|key| matches(key))
            .cloned()
            .collect::<Vec<_>>();
        let tasks = keys
            .into_iter()
            .filter_map(|key| watches.remove(&key))
            .collect::<Vec<_>>();
        drop(watches);
        for task in tasks {
            abort_task(task).await;
        }
    }

    async fn abort_detail_watches(&self, matches: impl Fn(&(i32, u64)) -> bool) {
        let mut watches = self.detail_watches.lock().await;
        let keys = watches
            .keys()
            .filter(|key| matches(key))
            .copied()
            .collect::<Vec<_>>();
        let tasks = keys
            .into_iter()
            .filter_map(|key| watches.remove(&key))
            .collect::<Vec<_>>();
        drop(watches);
        for task in tasks {
            abort_task(task).await;
        }
    }

    async fn replace_pod_metrics_watch(&self, key: (i32, String), task: JoinHandle<()>) {
        let previous = self.pod_metrics_watches.lock().await.insert(key, task);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
    }

    async fn abort_pod_metrics_watches(&self, matches: impl Fn(&(i32, String)) -> bool) {
        let mut watches = self.pod_metrics_watches.lock().await;
        let keys = watches
            .keys()
            .filter(|key| matches(key))
            .cloned()
            .collect::<Vec<_>>();
        let tasks = keys
            .into_iter()
            .filter_map(|key| watches.remove(&key))
            .collect::<Vec<_>>();
        drop(watches);
        for task in tasks {
            abort_task(task).await;
        }
    }

    async fn replace_node_metrics_watch(&self, key: i32, task: JoinHandle<()>) {
        let previous = self.node_metrics_watches.lock().await.insert(key, task);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
    }

    async fn abort_node_metrics_watches(&self, matches: impl Fn(&i32) -> bool) {
        let mut watches = self.node_metrics_watches.lock().await;
        let keys = watches
            .keys()
            .filter(|key| matches(key))
            .copied()
            .collect::<Vec<_>>();
        let tasks = keys
            .into_iter()
            .filter_map(|key| watches.remove(&key))
            .collect::<Vec<_>>();
        drop(watches);
        for task in tasks {
            abort_task(task).await;
        }
    }

    async fn abort_log_streams(&self, matches: impl Fn(&(i32, u64)) -> bool) {
        let mut streams = self.log_streams.lock().await;
        let keys = streams
            .keys()
            .filter(|key| matches(key))
            .copied()
            .collect::<Vec<_>>();
        let tasks = keys
            .into_iter()
            .filter_map(|key| streams.remove(&key))
            .collect::<Vec<_>>();
        drop(streams);
        for task in tasks {
            abort_task(task).await;
        }
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

impl WorkerRuntime {
    fn run(mut self) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        info!("Worker thread running");
        while let Some(command) = self.receiver.blocking_recv() {
            // Only session/watch control operations need linearization. Regular
            // Kubernetes reads and mutations run independently so a slow API
            // call cannot prevent a close, reconnect, or reload from running.
            if command.serializes_session_lifecycle() {
                runtime.block_on(dispatch_command(command, self.state.clone()));
            } else {
                let state = self.state.clone();
                runtime.spawn(dispatch_command(command, state));
            }
        }
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
            connections: Mutex::new(HashMap::new()),
            resource_watches: Mutex::new(HashMap::new()),
            detail_watches: Mutex::new(HashMap::new()),
            pod_metrics_watches: Mutex::new(HashMap::new()),
            node_metrics_watches: Mutex::new(HashMap::new()),
            log_streams: Mutex::new(HashMap::new()),
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
                connections: Mutex::new(HashMap::new()),
                resource_watches: Mutex::new(HashMap::new()),
                detail_watches: Mutex::new(HashMap::new()),
                pod_metrics_watches: Mutex::new(HashMap::new()),
                node_metrics_watches: Mutex::new(HashMap::new()),
                log_streams: Mutex::new(HashMap::new()),
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
                .lock()
                .await
                .insert((1, 3), tokio::spawn(AbortProbe(detail_aborted.clone())));
            state.pod_metrics_watches.lock().await.insert(
                (1, "default".to_owned()),
                tokio::spawn(AbortProbe(metrics_aborted.clone())),
            );
            state
                .node_metrics_watches
                .lock()
                .await
                .insert(1, tokio::spawn(AbortProbe(node_metrics_aborted.clone())));
            state
                .log_streams
                .lock()
                .await
                .insert((1, 4), tokio::spawn(AbortProbe(log_aborted.clone())));

            state.stop_all_clusters().await;
            tokio::task::yield_now().await;

            assert_eq!(resource_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(detail_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(metrics_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(node_metrics_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(log_aborted.load(Ordering::Relaxed), 1);
            assert!(state.resource_watches.lock().await.is_empty());
            assert!(state.detail_watches.lock().await.is_empty());
            assert!(state.pod_metrics_watches.lock().await.is_empty());
            assert!(state.node_metrics_watches.lock().await.is_empty());
            assert!(state.log_streams.lock().await.is_empty());
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

            state.stop_cluster(1).await;

            assert_eq!(first_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(second_aborted.load(Ordering::Relaxed), 0);
            assert_eq!(first_metrics_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(second_metrics_aborted.load(Ordering::Relaxed), 0);
            assert_eq!(first_node_metrics_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(second_node_metrics_aborted.load(Ordering::Relaxed), 0);
            assert!(
                state
                    .resource_watches
                    .lock()
                    .await
                    .contains_key(&ResourceScope {
                        cluster_key: 2,
                        api_resource: pod_resource(),
                        namespace: Some("default".to_owned()),
                    })
            );
            assert!(
                state
                    .pod_metrics_watches
                    .lock()
                    .await
                    .contains_key(&(2, "default".to_owned()))
            );
            assert!(state.node_metrics_watches.lock().await.contains_key(&2));
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
    fn session_control_commands_are_serialized_while_api_requests_are_not() {
        let load_clusters: WorkerCommandBox = Box::new(LoadClusters);
        assert!(load_clusters.serializes_session_lifecycle());
        let stop_logs = StopPodLogStream {
            cluster_key: 1,
            log_window_id: 1,
        };
        let stop_logs: WorkerCommandBox = Box::new(stop_logs);
        assert!(stop_logs.serializes_session_lifecycle());
        let get_yaml = GetResourceYaml {
            editor_id: 1,
            cluster_key: 1,
            api_resource: pod_resource(),
            namespace: Some("default".to_owned()),
            resource_name: "pod".to_owned(),
        };
        let get_yaml: WorkerCommandBox = Box::new(get_yaml);
        assert!(!get_yaml.serializes_session_lifecycle());
    }
}
