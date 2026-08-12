use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    Cluster, ClusterConnection, ResourceDataUpdateRequest, ResourceDetailWatchRequest,
    ResourceYamlValidationRequest, apply_resource_yaml, delete_resource, force_delete_resource,
    get_resource_scale, get_resource_schema, get_resource_yaml, reload_kubeconfig,
    restart_deployment, start_cluster_connection, start_resource_watcher, update_resource_data,
    update_resource_scale, validate_resource_yaml, watch_resource_detail,
};
use crate::helpers::ResultExt;
use crate::log_store::LogStoreAppender;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::resource_detail::{ManagedResource, ResourceDetail, ResourceEvent};
use crate::resource_schema::ResourceSchema;
use crate::resource_table::CustomResourceColumn;
use anyhow::Error;
#[cfg(test)]
use std::collections::VecDeque;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::info;

mod pod_logs;

/// Trait abstracting the worker interface for testability
pub trait WorkerTrait: Default {
    fn with_repaint_context(_context: egui::Context) -> Self {
        Self::default()
    }

    fn start(&mut self);
    fn get_next_message(&mut self) -> Option<WorkerResult>;
    fn send_command(&mut self, command: WorkerCommand);
    fn set_log_store_appender(&mut self, _appender: LogStoreAppender) {}
}

#[derive(Default)]
pub struct Worker {
    inner: Option<WorkerInner>,
    repaint_context: Option<egui::Context>,
    log_store_appender: Option<LogStoreAppender>,
}

impl Worker {
    pub(crate) fn with_repaint_context(context: egui::Context) -> Self {
        Self {
            inner: None,
            repaint_context: Some(context),
            log_store_appender: None,
        }
    }

    pub fn start(&mut self) {
        if self.inner.is_none() {
            let (command_channel_sender, command_channel_receiver) = mpsc::sync_channel(10);
            let (result_channel_sender, result_channel_receiver) = mpsc::sync_channel(1024);
            let result_sender =
                WorkerResultSender::new(result_channel_sender, self.repaint_context.clone());

            command_channel_sender
                .send(WorkerCommand::LoadClusters)
                .expect("Failed to send initial LoadClusters command");

            let shared = Arc::new(SharedWorkerState {
                connections: Mutex::new(HashMap::new()),
                resource_watches: Mutex::new(HashMap::new()),
                detail_watches: Mutex::new(HashMap::new()),
                log_streams: Mutex::new(HashMap::new()),
                log_store_appender: self.log_store_appender.clone(),
            });

            let worker = WorkerRuntime {
                sender: result_sender,
                receiver: command_channel_receiver,
                shared,
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

    pub(crate) fn get_next_message(&mut self) -> Option<WorkerResult> {
        if let Some(inner) = &mut self.inner {
            inner.receiver.try_recv().ok()
        } else {
            None
        }
    }

    pub(crate) fn send_command(&mut self, command: WorkerCommand) {
        if let Some(inner) = &mut self.inner {
            inner
                .sender
                .send(command)
                .log_if_error("Failed to send command");
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

    fn get_next_message(&mut self) -> Option<WorkerResult> {
        Worker::get_next_message(self)
    }

    fn send_command(&mut self, command: WorkerCommand) {
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
    pub results: VecDeque<WorkerResult>,
    pub commands: Vec<WorkerCommand>,
}

#[cfg(test)]
impl WorkerTrait for MockWorker {
    fn start(&mut self) {
        // No-op for mock
    }

    fn get_next_message(&mut self) -> Option<WorkerResult> {
        self.results.pop_front()
    }

    fn send_command(&mut self, command: WorkerCommand) {
        self.commands.push(command);
    }
}

struct WorkerInner {
    sender: mpsc::SyncSender<WorkerCommand>,
    receiver: mpsc::Receiver<WorkerResult>,
}

/// Identifies a Kubernetes resource collection within a connected cluster.
///
/// This is shared by worker outcomes so the UI never needs to reconstruct a
/// watch identity from an entire failed command.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ResourceScope {
    pub cluster_key: i32,
    pub api_resource: ApiResource,
    pub namespace: Option<String>,
}

/// The operation that produced a worker failure. Values such as YAML and
/// Secret data intentionally never appear here.
#[derive(Debug)]
pub enum WorkerOperation {
    LoadClusters,
    ConnectCluster {
        cluster_key: i32,
    },
    StartResourceWatch {
        scope: ResourceScope,
    },
    StartResourceDetailWatch {
        cluster_key: i32,
        history_entry_id: u64,
    },
    StopResourceDetailWatch,
    GetResourceYaml {
        editor_id: u64,
    },
    DeleteResource {
        scope: ResourceScope,
        resource_name: String,
        bulk_delete_id: Option<u64>,
    },
    ForceDeleteResource {
        cluster_key: i32,
    },
    RestartDeployment {
        cluster_key: i32,
    },
    GetOrUpdateResourceScale {
        cluster_key: i32,
    },
    ApplyResourceYaml {
        editor_id: u64,
    },
    LoadResourceSchema {
        editor_id: u64,
    },
    ValidateResourceYaml {
        editor_id: u64,
        revision: u64,
    },
    UpdateResourceData {
        cluster_key: i32,
        resource_name: String,
    },
    StartPodLogStream {
        log_window_id: u64,
    },
    StopPodLogStream,
}

/// Messages that can be sent to the worker
#[derive(Debug)]
pub enum WorkerCommand {
    LoadClusters,
    ConnectToCluster {
        cluster: String,
        cluster_key: i32,
    },
    StartResourceWatch {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
    },
    StartResourceDetailWatch {
        cluster_key: i32,
        /// Stable identity of this visit in the inspector history. A resource
        /// may be revisited, so it cannot be keyed by Kubernetes UID alone.
        history_entry_id: u64,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        resource_uid: String,
    },
    StopResourceDetailWatch {
        cluster_key: i32,
        history_entry_id: u64,
    },
    GetResourceYaml {
        editor_id: u64,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
    },
    DeleteResource {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        resource_uid: Option<String>,
        bulk_delete_id: Option<u64>,
    },
    ForceDeleteResource {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        resource_uid: String,
    },
    RestartDeployment {
        cluster_key: i32,
        namespace: String,
        resource_name: String,
    },
    GetResourceScale {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
    },
    UpdateResourceScale {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        replicas: i32,
    },
    ApplyResourceYaml {
        editor_id: u64,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        yaml: String,
    },
    LoadResourceSchema {
        editor_id: u64,
        cluster_key: i32,
        api_resource: ApiResource,
    },
    ValidateResourceYaml {
        editor_id: u64,
        revision: u64,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        yaml: String,
    },
    UpdateResourceData {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resource_name: String,
        update: ResourceDataUpdate,
    },
    StartPodLogStream {
        cluster_key: i32,
        log_window_id: u64,
        namespace: String,
        pod_name: String,
        container: String,
    },
    StopPodLogStream {
        cluster_key: i32,
        log_window_id: u64,
    },
}

/// The values are intentionally omitted from Debug output because this command can
/// contain Secret plaintext. The worker logs failed commands at debug format.
pub struct ResourceDataUpdate {
    pub expected_resource_version: String,
    pub expected_values: BTreeMap<String, String>,
    pub updated_values: BTreeMap<String, String>,
}

impl WorkerCommand {
    fn serializes_session_lifecycle(&self) -> bool {
        matches!(
            self,
            Self::LoadClusters
                | Self::ConnectToCluster { .. }
                | Self::StartResourceWatch { .. }
                | Self::StartResourceDetailWatch { .. }
                | Self::StopResourceDetailWatch { .. }
                | Self::StartPodLogStream { .. }
                | Self::StopPodLogStream { .. }
        )
    }

    fn operation(&self) -> WorkerOperation {
        match self {
            Self::LoadClusters => WorkerOperation::LoadClusters,
            Self::ConnectToCluster { cluster_key, .. } => WorkerOperation::ConnectCluster {
                cluster_key: *cluster_key,
            },
            Self::StartResourceWatch {
                cluster_key,
                api_resource,
                namespace,
            } => WorkerOperation::StartResourceWatch {
                scope: ResourceScope {
                    cluster_key: *cluster_key,
                    api_resource: api_resource.clone(),
                    namespace: namespace.clone(),
                },
            },
            Self::StartResourceDetailWatch {
                cluster_key,
                history_entry_id,
                ..
            } => WorkerOperation::StartResourceDetailWatch {
                cluster_key: *cluster_key,
                history_entry_id: *history_entry_id,
            },
            Self::StopResourceDetailWatch { .. } => WorkerOperation::StopResourceDetailWatch,
            Self::GetResourceYaml { editor_id, .. } => WorkerOperation::GetResourceYaml {
                editor_id: *editor_id,
            },
            Self::DeleteResource {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                bulk_delete_id,
                ..
            } => WorkerOperation::DeleteResource {
                scope: ResourceScope {
                    cluster_key: *cluster_key,
                    api_resource: api_resource.clone(),
                    namespace: namespace.clone(),
                },
                resource_name: resource_name.clone(),
                bulk_delete_id: *bulk_delete_id,
            },
            Self::ForceDeleteResource { cluster_key, .. } => WorkerOperation::ForceDeleteResource {
                cluster_key: *cluster_key,
            },
            Self::RestartDeployment { cluster_key, .. } => WorkerOperation::RestartDeployment {
                cluster_key: *cluster_key,
            },
            Self::GetResourceScale { cluster_key, .. }
            | Self::UpdateResourceScale { cluster_key, .. } => {
                WorkerOperation::GetOrUpdateResourceScale {
                    cluster_key: *cluster_key,
                }
            }
            Self::ApplyResourceYaml { editor_id, .. } => WorkerOperation::ApplyResourceYaml {
                editor_id: *editor_id,
            },
            Self::LoadResourceSchema { editor_id, .. } => WorkerOperation::LoadResourceSchema {
                editor_id: *editor_id,
            },
            Self::ValidateResourceYaml {
                editor_id,
                revision,
                ..
            } => WorkerOperation::ValidateResourceYaml {
                editor_id: *editor_id,
                revision: *revision,
            },
            Self::UpdateResourceData {
                cluster_key,
                resource_name,
                ..
            } => WorkerOperation::UpdateResourceData {
                cluster_key: *cluster_key,
                resource_name: resource_name.clone(),
            },
            Self::StartPodLogStream { log_window_id, .. } => WorkerOperation::StartPodLogStream {
                log_window_id: *log_window_id,
            },
            Self::StopPodLogStream { .. } => WorkerOperation::StopPodLogStream,
        }
    }
}

impl std::fmt::Debug for ResourceDataUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceDataUpdate")
            .field("keys", &self.updated_values.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Messages that can be received from the worker
#[derive(Debug)]
pub enum WorkerResult {
    CommandFailed {
        operation: WorkerOperation,
        error: Error,
    },
    KubernetesClustersUpdated(Vec<Cluster>),
    KubernetesNamespacesAdded {
        cluster_key: i32,
        namespace: MinimalNamespace,
    },
    KubernetesNamespacesDeleted {
        cluster_key: i32,
        namespace_name: String,
    },
    KubernetesNamespacesReplaced {
        cluster_key: i32,
        namespaces: Vec<MinimalNamespace>,
    },
    KubernetesNamespacesLoadFailed {
        cluster_key: i32,
        error: String,
    },
    KubernetesApisLoaded {
        cluster_key: i32,
        api_resources: Vec<ApiResource>,
        scalable_api_resources: std::collections::BTreeSet<ApiResource>,
    },
    KubernetesCustomResourceColumnsLoaded {
        cluster_key: i32,
        columns: std::collections::BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    },
    KubernetesResourceSchemasLoaded {
        cluster_key: i32,
        schemas: std::collections::BTreeMap<ApiResource, ResourceSchema>,
    },
    KubernetesApisLoadFailed {
        cluster_key: i32,
        error: String,
    },
    KubernetesClusterConnectionCreated {
        cluster_key: i32,
    },
    /// A resource was added or updated
    KubernetesResourceAdded {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource: MinimalResource,
    },
    /// A resource was deleted
    KubernetesResourceDeleted {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_uid: String,
    },
    /// Initial resource list complete
    KubernetesResourcesReplaced {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resources: Vec<MinimalResource>,
    },
    /// Resource watcher started successfully
    KubernetesResourceWatchStarted {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
    },
    KubernetesResourceWatchFailed {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        error: String,
    },
    ResourceDetailUpdated {
        cluster_key: i32,
        history_entry_id: u64,
        detail: Box<ResourceDetail>,
    },
    ResourceDetailDeleted {
        cluster_key: i32,
        history_entry_id: u64,
    },
    ManagedResourcesReplaced {
        cluster_key: i32,
        history_entry_id: u64,
        resources: Vec<ManagedResource>,
    },
    ManagedResourcesWatchFailed {
        cluster_key: i32,
        history_entry_id: u64,
        error: String,
    },
    ResourceEventsReplaced {
        cluster_key: i32,
        history_entry_id: u64,
        events: Vec<ResourceEvent>,
    },
    ResourceDetailWatchFailed {
        cluster_key: i32,
        history_entry_id: u64,
        events: bool,
        error: String,
    },
    /// Resource YAML fetched for viewing/editing
    ResourceYamlFetched {
        editor_id: u64,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        yaml: String,
    },
    ResourceSchemaLoaded {
        editor_id: u64,
        cluster_key: i32,
        api_resource: ApiResource,
        schema: ResourceSchema,
    },
    ResourceYamlValidated {
        editor_id: u64,
        revision: u64,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
    },
    ResourceYamlValidationFailed {
        editor_id: u64,
        revision: u64,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        error: ResourceApiError,
    },
    /// Resource was successfully deleted
    ResourceDeleteCompleted {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        bulk_delete_id: Option<u64>,
    },
    /// A deleting resource's finalizers were removed.
    ResourceForceDeleteCompleted {
        cluster_key: i32,
        resource_name: String,
    },
    /// Resource YAML was successfully applied
    ResourceApplyCompleted {
        editor_id: u64,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
    },
    ResourceApplyFailed {
        editor_id: u64,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        error: ResourceApiError,
    },
    /// A Deployment rollout restart patch was accepted by the API server.
    DeploymentRestartCompleted {
        namespace: String,
        resource_name: String,
    },
    ResourceScaleFetched {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        replicas: i32,
    },
    ResourceScaleUpdated {
        cluster_key: i32,
        resource_name: String,
    },
    ResourceDataUpdateCompleted {
        cluster_key: i32,
        resource_name: String,
    },
    PodLogStreamStarted {
        log_window_id: u64,
    },
    PodLogStreamEnded {
        log_window_id: u64,
    },
    PodLogStreamFailed {
        log_window_id: u64,
        error: String,
    },
}

#[derive(Clone)]
pub struct WorkerResultSender {
    sender: mpsc::SyncSender<WorkerResult>,
    repaint_context: Option<egui::Context>,
}

impl WorkerResultSender {
    fn new(sender: mpsc::SyncSender<WorkerResult>, repaint_context: Option<egui::Context>) -> Self {
        Self {
            sender,
            repaint_context,
        }
    }

    pub fn send(&self, result: WorkerResult) -> Result<(), Box<mpsc::SendError<WorkerResult>>> {
        self.sender.send(result).map_err(Box::new)?;
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
struct SharedWorkerState {
    /// Connected clusters and their root watcher tasks. This stays entirely on the
    /// worker side so UI state can never determine a Kubernetes task's lifetime.
    connections: Mutex<HashMap<i32, ClusterConnection>>,
    /// Resource watches are keyed by their complete scope and are aborted before
    /// replacement and whenever their cluster session is torn down.
    resource_watches: Mutex<HashMap<ResourceScope, JoinHandle<()>>>,
    /// Detail watches remain active while their visit is retained in an
    /// inspector's history.
    detail_watches: Mutex<HashMap<(i32, u64), JoinHandle<()>>>,
    /// Native log windows each own one cancellable follow stream.
    log_streams: Mutex<HashMap<(i32, u64), JoinHandle<()>>>,
    /// The bounded, disk-backed ingress for pod logs. A Kubernetes stream
    /// awaits this directly rather than routing log data through the UI.
    log_store_appender: Option<LogStoreAppender>,
}

impl SharedWorkerState {
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
        self.abort_log_streams(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
    }

    async fn stop_all_clusters(&self) {
        self.connections.lock().await.clear();
        self.abort_resource_watches(|_| true).await;
        self.abort_detail_watches(|_| true).await;
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
    sender: WorkerResultSender,
    receiver: mpsc::Receiver<WorkerCommand>,
    shared: Arc<SharedWorkerState>,
}

impl WorkerRuntime {
    fn run(self) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        info!("Worker thread running");
        while let Ok(command) = self.receiver.recv() {
            // Only session/watch control operations need linearization. Regular
            // Kubernetes reads and mutations run independently so a slow API
            // call cannot prevent a close, reconnect, or reload from running.
            if command.serializes_session_lifecycle() {
                runtime.block_on(WorkerRuntime::handle_command(
                    self.sender.clone(),
                    self.shared.clone(),
                    command,
                ));
            } else {
                runtime.spawn(WorkerRuntime::handle_command(
                    self.sender.clone(),
                    self.shared.clone(),
                    command,
                ));
            }
        }
    }

    async fn handle_command(
        result_channel: WorkerResultSender,
        shared: Arc<SharedWorkerState>,
        command: WorkerCommand,
    ) {
        let result = match &command {
            WorkerCommand::LoadClusters => {
                shared.stop_all_clusters().await;
                reload_kubeconfig().await.map(Some)
            }
            WorkerCommand::ConnectToCluster {
                cluster_key,
                cluster,
            } => {
                async {
                    shared.stop_cluster(*cluster_key).await;
                    let connection =
                        start_cluster_connection(*cluster_key, cluster, result_channel.clone())
                            .await?;
                    shared
                        .connections
                        .lock()
                        .await
                        .insert(*cluster_key, connection);
                    Ok(Some(WorkerResult::KubernetesClusterConnectionCreated {
                        cluster_key: *cluster_key,
                    }))
                }
                .await
            }
            WorkerCommand::StartResourceWatch {
                cluster_key,
                api_resource,
                namespace,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    let watch_key = ResourceScope {
                        cluster_key: *cluster_key,
                        api_resource: api_resource.clone(),
                        namespace: namespace.clone(),
                    };
                    let (result, task) = start_resource_watcher(
                        *cluster_key,
                        client,
                        api_resource.clone(),
                        namespace.clone(),
                        result_channel.clone(),
                    )
                    .await?;
                    shared.replace_resource_watch(watch_key, task).await;
                    Ok(Some(result))
                }
                .await
            }
            WorkerCommand::StartResourceDetailWatch {
                cluster_key,
                history_entry_id,
                api_resource,
                namespace,
                resource_name,
                resource_uid,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    let watch_key = (*cluster_key, *history_entry_id);
                    let previous = shared.detail_watches.lock().await.remove(&watch_key);
                    if let Some(previous) = previous {
                        abort_task(previous).await;
                    }
                    let handle = tokio::spawn(watch_resource_detail(ResourceDetailWatchRequest {
                        cluster_key: *cluster_key,
                        client,
                        api_resource: api_resource.clone(),
                        namespace: namespace.clone(),
                        resource_name: resource_name.clone(),
                        resource_uid: resource_uid.clone(),
                        history_entry_id: *history_entry_id,
                        event_sender: result_channel.clone(),
                    }));
                    shared.detail_watches.lock().await.insert(watch_key, handle);
                    Ok(None)
                }
                .await
            }
            WorkerCommand::StopResourceDetailWatch {
                cluster_key,
                history_entry_id,
            } => {
                async {
                    let handle = shared
                        .detail_watches
                        .lock()
                        .await
                        .remove(&(*cluster_key, *history_entry_id));
                    if let Some(handle) = handle {
                        abort_task(handle).await;
                    }
                    Ok(None)
                }
                .await
            }
            WorkerCommand::GetResourceYaml {
                editor_id,
                cluster_key,
                api_resource,
                namespace,
                resource_name,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    get_resource_yaml(
                        *editor_id,
                        *cluster_key,
                        client,
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                    )
                    .await
                    .map(Some)
                }
                .await
            }
            WorkerCommand::LoadResourceSchema {
                editor_id,
                cluster_key,
                api_resource,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    get_resource_schema(*editor_id, *cluster_key, client, api_resource.clone())
                        .await
                        .map(Some)
                }
                .await
            }
            WorkerCommand::DeleteResource {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                resource_uid,
                bulk_delete_id,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    delete_resource(
                        *cluster_key,
                        client,
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        resource_uid.clone(),
                        *bulk_delete_id,
                    )
                    .await
                    .map(Some)
                }
                .await
            }
            WorkerCommand::ForceDeleteResource {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                resource_uid,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    force_delete_resource(
                        *cluster_key,
                        client,
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        resource_uid.clone(),
                    )
                    .await
                    .map(Some)
                }
                .await
            }
            WorkerCommand::RestartDeployment {
                cluster_key,
                namespace,
                resource_name,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    restart_deployment(client, namespace.clone(), resource_name.clone())
                        .await
                        .map(Some)
                }
                .await
            }
            WorkerCommand::GetResourceScale {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    get_resource_scale(
                        *cluster_key,
                        client,
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                    )
                    .await
                    .map(Some)
                }
                .await
            }
            WorkerCommand::UpdateResourceScale {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                replicas,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    update_resource_scale(
                        *cluster_key,
                        client,
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        *replicas,
                    )
                    .await
                    .map(Some)
                }
                .await
            }
            WorkerCommand::ApplyResourceYaml {
                editor_id,
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                yaml,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    apply_resource_yaml(
                        *editor_id,
                        *cluster_key,
                        client,
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        yaml.clone(),
                    )
                    .await
                    .map(Some)
                }
                .await
            }
            WorkerCommand::ValidateResourceYaml {
                editor_id,
                revision,
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                yaml,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    validate_resource_yaml(ResourceYamlValidationRequest {
                        editor_id: *editor_id,
                        revision: *revision,
                        cluster_key: *cluster_key,
                        client,
                        api_resource: api_resource.clone(),
                        namespace: namespace.clone(),
                        resource_name: resource_name.clone(),
                        yaml: yaml.clone(),
                    })
                    .await
                    .map(Some)
                }
                .await
            }
            WorkerCommand::UpdateResourceData {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                update,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    update_resource_data(ResourceDataUpdateRequest {
                        cluster_key: *cluster_key,
                        client,
                        api_resource: api_resource.clone(),
                        namespace: namespace.clone(),
                        resource_name: resource_name.clone(),
                        expected_values: &update.expected_values,
                        updated_values: &update.updated_values,
                        expected_resource_version: &update.expected_resource_version,
                    })
                    .await
                    .map(Some)
                }
                .await
            }
            WorkerCommand::StartPodLogStream {
                cluster_key,
                log_window_id,
                namespace,
                pod_name,
                container,
            } => {
                async {
                    let client = shared.client_for_cluster(*cluster_key).await?;
                    if let Some(log_store_appender) = shared.log_store_appender.clone() {
                        let cluster_key = *cluster_key;
                        let log_window_id = *log_window_id;
                        let stream_key = (cluster_key, log_window_id);
                        let previous = shared.log_streams.lock().await.remove(&stream_key);
                        if let Some(previous) = previous {
                            abort_task(previous).await;
                        }
                        let task_sender = result_channel.clone();
                        let namespace = namespace.clone();
                        let pod_name = pod_name.clone();
                        let container = container.clone();
                        let task = tokio::spawn(async move {
                            pod_logs::stream(
                                log_window_id,
                                client,
                                namespace,
                                pod_name,
                                container,
                                log_store_appender,
                                task_sender,
                            )
                            .await;
                            // The registry is owned by command handling. A
                            // replaced stream must never remove its successor.
                        });
                        shared.log_streams.lock().await.insert(stream_key, task);
                        Ok(Some(WorkerResult::PodLogStreamStarted { log_window_id }))
                    } else {
                        Err(anyhow::anyhow!(
                            "Pod log storage is not initialized for cluster_key {}",
                            cluster_key
                        ))
                    }
                }
                .await
            }
            WorkerCommand::StopPodLogStream {
                cluster_key,
                log_window_id,
            } => {
                async {
                    let task = shared
                        .log_streams
                        .lock()
                        .await
                        .remove(&(*cluster_key, *log_window_id));
                    if let Some(task) = task {
                        abort_task(task).await;
                    }
                    Ok(Some(WorkerResult::PodLogStreamEnded {
                        log_window_id: *log_window_id,
                    }))
                }
                .await
            }
        };

        match result {
            Err(e) => {
                result_channel
                    .send(WorkerResult::CommandFailed {
                        operation: command.operation(),
                        error: e,
                    })
                    .log_if_error("Failed to send command failed notification");
            }
            Ok(Some(result)) => {
                result_channel
                    .send(result)
                    .log_if_error("Failed to send result notification");
            }
            Ok(None) => {}
        }
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

    fn shared_worker_state() -> SharedWorkerState {
        SharedWorkerState {
            connections: Mutex::new(HashMap::new()),
            resource_watches: Mutex::new(HashMap::new()),
            detail_watches: Mutex::new(HashMap::new()),
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
        let context = egui::Context::default();
        let repaint_count = Arc::new(AtomicUsize::new(0));
        let repaint_count_for_callback = repaint_count.clone();
        context.set_request_repaint_callback(move |_| {
            repaint_count_for_callback.fetch_add(1, Ordering::Relaxed);
        });
        let (sender, receiver) = mpsc::sync_channel(1);
        let result_sender = WorkerResultSender::new(sender, Some(context));

        result_sender
            .send(WorkerResult::PodLogStreamEnded { log_window_id: 2 })
            .expect("result receiver is open");

        assert!(matches!(
            receiver.try_recv(),
            Ok(WorkerResult::PodLogStreamEnded { log_window_id: 2 })
        ));
        assert_eq!(repaint_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn replacing_a_resource_watch_aborts_the_previous_task() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = shared_worker_state();
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
    fn stopping_all_clusters_aborts_supervised_tasks() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = shared_worker_state();
            let resource_aborted = Arc::new(AtomicUsize::new(0));
            let detail_aborted = Arc::new(AtomicUsize::new(0));
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
            state
                .log_streams
                .lock()
                .await
                .insert((1, 4), tokio::spawn(AbortProbe(log_aborted.clone())));

            state.stop_all_clusters().await;
            tokio::task::yield_now().await;

            assert_eq!(resource_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(detail_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(log_aborted.load(Ordering::Relaxed), 1);
            assert!(state.resource_watches.lock().await.is_empty());
            assert!(state.detail_watches.lock().await.is_empty());
            assert!(state.log_streams.lock().await.is_empty());
        });
    }

    #[test]
    fn stopping_one_cluster_preserves_other_cluster_tasks() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
        runtime.block_on(async {
            let state = shared_worker_state();
            let first_aborted = Arc::new(AtomicUsize::new(0));
            let second_aborted = Arc::new(AtomicUsize::new(0));
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

            state.stop_cluster(1).await;

            assert_eq!(first_aborted.load(Ordering::Relaxed), 1);
            assert_eq!(second_aborted.load(Ordering::Relaxed), 0);
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
        });
    }

    #[test]
    fn failure_operation_omits_resource_data_values() {
        let operation = WorkerCommand::UpdateResourceData {
            cluster_key: 7,
            api_resource: pod_resource(),
            namespace: "default".to_owned(),
            resource_name: "credentials".to_owned(),
            update: ResourceDataUpdate {
                expected_resource_version: "42".to_owned(),
                expected_values: BTreeMap::from([("token".to_owned(), "old-secret".to_owned())]),
                updated_values: BTreeMap::from([("token".to_owned(), "new-secret".to_owned())]),
            },
        }
        .operation();

        assert!(matches!(
            &operation,
            WorkerOperation::UpdateResourceData {
                cluster_key: 7,
                resource_name,
            } if resource_name == "credentials"
        ));
        assert!(!format!("{operation:?}").contains("secret"));
    }

    #[test]
    fn yaml_failure_operations_omit_document_text() {
        let secret_yaml = "data:\n  token: definitely-secret".to_owned();
        let apply = WorkerCommand::ApplyResourceYaml {
            editor_id: 9,
            cluster_key: 7,
            api_resource: pod_resource(),
            namespace: Some("default".to_owned()),
            resource_name: "credentials".to_owned(),
            yaml: secret_yaml.clone(),
        }
        .operation();
        let validation = WorkerCommand::ValidateResourceYaml {
            editor_id: 9,
            revision: 4,
            cluster_key: 7,
            api_resource: pod_resource(),
            namespace: Some("default".to_owned()),
            resource_name: "credentials".to_owned(),
            yaml: secret_yaml,
        }
        .operation();

        assert!(matches!(
            apply,
            WorkerOperation::ApplyResourceYaml { editor_id: 9 }
        ));
        assert!(matches!(
            validation,
            WorkerOperation::ValidateResourceYaml {
                editor_id: 9,
                revision: 4,
            }
        ));
        assert!(!format!("{apply:?}{validation:?}").contains("definitely-secret"));
    }

    #[test]
    fn session_control_commands_are_serialized_while_api_requests_are_not() {
        assert!(WorkerCommand::LoadClusters.serializes_session_lifecycle());
        assert!(
            WorkerCommand::StopPodLogStream {
                cluster_key: 1,
                log_window_id: 1,
            }
            .serializes_session_lifecycle()
        );
        assert!(
            !WorkerCommand::GetResourceYaml {
                editor_id: 1,
                cluster_key: 1,
                api_resource: pod_resource(),
                namespace: Some("default".to_owned()),
                resource_name: "pod".to_owned(),
            }
            .serializes_session_lifecycle()
        );
    }
}
