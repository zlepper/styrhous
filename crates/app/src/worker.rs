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
use futures_util::{AsyncBufReadExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, LogParams};
#[cfg(test)]
use std::collections::VecDeque;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::info;

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
                clients: RwLock::new(HashMap::new()),
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
        command: Option<WorkerCommand>,
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
        runner: Option<ClusterConnection>,
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
        resource_name: String,
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
    /// Kube clients indexed by cluster_key
    clients: RwLock<HashMap<i32, kube::Client>>,
    /// Detail watches remain active while their visit is retained in an
    /// inspector's history.
    detail_watches: Mutex<HashMap<(i32, u64), JoinHandle<()>>>,
    /// Native log windows each own one cancellable follow stream.
    log_streams: Mutex<HashMap<(i32, u64), JoinHandle<()>>>,
    /// The bounded, disk-backed ingress for pod logs. A Kubernetes stream
    /// awaits this directly rather than routing log data through the UI.
    log_store_appender: Option<LogStoreAppender>,
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
            runtime.spawn(WorkerRuntime::handle_command(
                self.sender.clone(),
                self.shared.clone(),
                command,
            ));
        }
    }

    async fn handle_command(
        result_channel: WorkerResultSender,
        shared: Arc<SharedWorkerState>,
        command: WorkerCommand,
    ) {
        let result = match &command {
            WorkerCommand::LoadClusters => reload_kubeconfig().await.map(Some),
            WorkerCommand::ConnectToCluster {
                cluster_key,
                cluster,
            } => {
                let res =
                    start_cluster_connection(*cluster_key, cluster, result_channel.clone()).await;
                // Store the client for later use by resource watchers
                if let Ok(WorkerResult::KubernetesClusterConnectionCreated {
                    cluster_key,
                    runner: Some(runner),
                }) = &res
                {
                    let client = runner.client();
                    shared.clients.write().await.insert(*cluster_key, client);
                }
                res.map(Some)
            }
            WorkerCommand::StartResourceWatch {
                cluster_key,
                api_resource,
                namespace,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    start_resource_watcher(
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        result_channel.clone(),
                    )
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::StartResourceDetailWatch {
                cluster_key,
                history_entry_id,
                api_resource,
                namespace,
                resource_name,
                resource_uid,
            } => {
                let client = {
                    let clients = shared.clients.read().await;
                    clients.get(cluster_key).cloned()
                };
                if let Some(client) = client {
                    let watch_key = (*cluster_key, *history_entry_id);
                    if let Some(previous) = shared.detail_watches.lock().await.remove(&watch_key) {
                        previous.abort();
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
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::StopResourceDetailWatch {
                cluster_key,
                history_entry_id,
            } => {
                if let Some(handle) = shared
                    .detail_watches
                    .lock()
                    .await
                    .remove(&(*cluster_key, *history_entry_id))
                {
                    handle.abort();
                }
                Ok(None)
            }
            WorkerCommand::GetResourceYaml {
                editor_id,
                cluster_key,
                api_resource,
                namespace,
                resource_name,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    get_resource_yaml(
                        *editor_id,
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                    )
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::LoadResourceSchema {
                editor_id,
                cluster_key,
                api_resource,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    get_resource_schema(
                        *editor_id,
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                    )
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::DeleteResource {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    delete_resource(
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                    )
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::ForceDeleteResource {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                resource_uid,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    force_delete_resource(
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        resource_uid.clone(),
                    )
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::RestartDeployment {
                cluster_key,
                namespace,
                resource_name,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    restart_deployment(client.clone(), namespace.clone(), resource_name.clone())
                        .await
                        .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::GetResourceScale {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    get_resource_scale(
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                    )
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::UpdateResourceScale {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                replicas,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    update_resource_scale(
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        *replicas,
                    )
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::ApplyResourceYaml {
                editor_id,
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                yaml,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    apply_resource_yaml(
                        *editor_id,
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        yaml.clone(),
                    )
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
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
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    validate_resource_yaml(ResourceYamlValidationRequest {
                        editor_id: *editor_id,
                        revision: *revision,
                        cluster_key: *cluster_key,
                        client: client.clone(),
                        api_resource: api_resource.clone(),
                        namespace: namespace.clone(),
                        resource_name: resource_name.clone(),
                        yaml: yaml.clone(),
                    })
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::UpdateResourceData {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                update,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    update_resource_data(ResourceDataUpdateRequest {
                        cluster_key: *cluster_key,
                        client: client.clone(),
                        api_resource: api_resource.clone(),
                        namespace: namespace.clone(),
                        resource_name: resource_name.clone(),
                        expected_values: &update.expected_values,
                        updated_values: &update.updated_values,
                        expected_resource_version: &update.expected_resource_version,
                    })
                    .await
                    .map(Some)
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::StartPodLogStream {
                cluster_key,
                log_window_id,
                namespace,
                pod_name,
                container,
            } => {
                let client = {
                    let clients = shared.clients.read().await;
                    clients.get(cluster_key).cloned()
                };
                if let Some(client) = client {
                    if let Some(log_store_appender) = shared.log_store_appender.clone() {
                        let cluster_key = *cluster_key;
                        let log_window_id = *log_window_id;
                        let stream_key = (cluster_key, log_window_id);
                        if let Some(previous) = shared.log_streams.lock().await.remove(&stream_key)
                        {
                            previous.abort();
                        }
                        let task_shared = shared.clone();
                        let task_sender = result_channel.clone();
                        let namespace = namespace.clone();
                        let pod_name = pod_name.clone();
                        let container = container.clone();
                        let task = tokio::spawn(async move {
                            stream_pod_logs(
                                log_window_id,
                                client,
                                namespace,
                                pod_name,
                                container,
                                log_store_appender,
                                task_sender,
                            )
                            .await;
                            task_shared.log_streams.lock().await.remove(&stream_key);
                        });
                        shared.log_streams.lock().await.insert(stream_key, task);
                        Ok(Some(WorkerResult::PodLogStreamStarted { log_window_id }))
                    } else {
                        Err(anyhow::anyhow!(
                            "Pod log storage is not initialized for cluster_key {}",
                            cluster_key
                        ))
                    }
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::StopPodLogStream {
                cluster_key,
                log_window_id,
            } => {
                if let Some(task) = shared
                    .log_streams
                    .lock()
                    .await
                    .remove(&(*cluster_key, *log_window_id))
                {
                    task.abort();
                }
                Ok(Some(WorkerResult::PodLogStreamEnded {
                    log_window_id: *log_window_id,
                }))
            }
        };

        match result {
            Err(e) => {
                result_channel
                    .send(WorkerResult::CommandFailed {
                        command: Some(command),
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

async fn stream_pod_logs(
    log_window_id: u64,
    client: kube::Client,
    namespace: String,
    pod_name: String,
    container: String,
    log_store_appender: LogStoreAppender,
    sender: WorkerResultSender,
) {
    let result = async {
        let pods: Api<Pod> = Api::namespaced(client, &namespace);
        let backfill_container = container.clone();
        let tail_stream = pods
            .log_stream(
                &pod_name,
                &LogParams {
                    container: Some(container),
                    follow: true,
                    tail_lines: Some(1_000),
                    timestamps: true,
                    ..LogParams::default()
                },
            )
            .await?;
        let backfill_pods = pods.clone();
        let backfill_pod_name = pod_name.clone();
        let backfill_appender = log_store_appender.clone();
        let live = append_pod_log_stream(tail_stream, log_store_appender, log_window_id, false);
        let backfill = async move {
            let stream = backfill_pods
                .log_stream(
                    &backfill_pod_name,
                    &LogParams {
                        container: Some(backfill_container),
                        timestamps: true,
                        ..LogParams::default()
                    },
                )
                .await?;
            append_pod_log_stream(stream, backfill_appender.clone(), log_window_id, true).await?;
            backfill_appender.complete_backfill(log_window_id).await
        };
        tokio::try_join!(live, backfill)?;
        anyhow::Ok(())
    }
    .await;
    let result = match result {
        Ok(()) => WorkerResult::PodLogStreamEnded { log_window_id },
        Err(error) => WorkerResult::PodLogStreamFailed {
            log_window_id,
            error: format!("{error:#}"),
        },
    };
    sender
        .send(result)
        .log_if_error("Failed to send Pod log stream result");
}

async fn append_pod_log_stream(
    stream: impl futures_util::AsyncBufRead + Unpin,
    log_store_appender: LogStoreAppender,
    log_window_id: u64,
    backfill: bool,
) -> anyhow::Result<()> {
    let mut lines = stream.lines();
    let mut batch = Vec::new();
    let mut flush = tokio::time::interval(Duration::from_millis(100));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            line = lines.try_next() => {
                let Some(line) = line? else {
                    break;
                };
                batch.push(line);
                if batch.len() >= 64 {
                    append_log_batch(
                        &log_store_appender,
                        log_window_id,
                        std::mem::take(&mut batch),
                        backfill,
                    ).await?;
                }
            }
            _ = flush.tick(), if !batch.is_empty() => {
                append_log_batch(
                    &log_store_appender,
                    log_window_id,
                    std::mem::take(&mut batch),
                    backfill,
                ).await?;
            }
        }
    }
    if !batch.is_empty() {
        append_log_batch(&log_store_appender, log_window_id, batch, backfill).await?;
    }
    anyhow::Ok(())
}

async fn append_log_batch(
    appender: &LogStoreAppender,
    log_window_id: u64,
    lines: Vec<String>,
    backfill: bool,
) -> anyhow::Result<()> {
    if backfill {
        appender.append_backfill(log_window_id, lines).await
    } else {
        appender.append(log_window_id, lines).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
}
