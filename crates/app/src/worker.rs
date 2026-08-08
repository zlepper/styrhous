use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    Cluster, ClusterConnection, apply_resource_yaml, delete_resource, get_resource_yaml,
    reload_kubeconfig, start_cluster_connection, start_resource_watcher, update_resource_data,
    watch_resource_detail,
};
use crate::helpers::ResultExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::resource_detail::{ManagedResource, ResourceDetail, ResourceEvent};
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
    fn start(&mut self);
    fn get_next_message(&mut self) -> Option<WorkerResult>;
    fn send_command(&mut self, command: WorkerCommand);
}

#[derive(Default)]
pub struct Worker {
    inner: Option<WorkerInner>,
}

impl Worker {
    pub fn start(&mut self) {
        if self.inner.is_none() {
            let (command_channel_sender, command_channel_receiver) = mpsc::sync_channel(10);
            let (result_channel_sender, result_channel_receiver) = mpsc::sync_channel(1024);

            command_channel_sender
                .send(WorkerCommand::LoadClusters)
                .expect("Failed to send initial LoadClusters command");

            let shared = Arc::new(SharedWorkerState {
                clients: RwLock::new(HashMap::new()),
                detail_watches: Mutex::new(HashMap::new()),
                log_streams: Mutex::new(HashMap::new()),
            });

            let worker = WorkerRuntime {
                sender: result_channel_sender,
                receiver: command_channel_receiver,
                shared,
            };

            let worker_thread = std::thread::spawn(move || {
                worker.run();
            });

            self.inner = Some(WorkerInner {
                receiver: result_channel_receiver,
                sender: command_channel_sender,
                worker_thread,
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
}

impl WorkerTrait for Worker {
    fn start(&mut self) {
        Worker::start(self)
    }

    fn get_next_message(&mut self) -> Option<WorkerResult> {
        Worker::get_next_message(self)
    }

    fn send_command(&mut self, command: WorkerCommand) {
        Worker::send_command(self, command)
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
    worker_thread: std::thread::JoinHandle<()>,
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
    ApplyResourceYaml {
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
    },
    KubernetesCustomResourceColumnsLoaded {
        cluster_key: i32,
        columns: std::collections::BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
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
    ResourceDetailWatchStarted {
        cluster_key: i32,
        history_entry_id: u64,
    },
    ResourceDetailWatchStopped {
        cluster_key: i32,
    },
    ResourceDetailUpdated {
        cluster_key: i32,
        history_entry_id: u64,
        detail: ResourceDetail,
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
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
        yaml: String,
    },
    /// Resource was successfully deleted
    ResourceDeleteCompleted {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
    },
    /// Resource YAML was successfully applied
    ResourceApplyCompleted {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        resource_name: String,
    },
    ResourceDataUpdateCompleted {
        cluster_key: i32,
        resource_name: String,
    },
    PodLogStreamStarted {
        cluster_key: i32,
        log_window_id: u64,
    },
    PodLogLinesAppended {
        cluster_key: i32,
        log_window_id: u64,
        lines: Vec<String>,
    },
    PodLogStreamEnded {
        cluster_key: i32,
        log_window_id: u64,
    },
    PodLogStreamFailed {
        cluster_key: i32,
        log_window_id: u64,
        error: String,
    },
}

pub type WorkerResultSender = mpsc::SyncSender<WorkerResult>;

/// Shared state accessible from spawned async tasks
struct SharedWorkerState {
    /// Kube clients indexed by cluster_key
    clients: RwLock<HashMap<i32, kube::Client>>,
    /// Detail watches remain active while their visit is retained in an
    /// inspector's history.
    detail_watches: Mutex<HashMap<(i32, u64), JoinHandle<()>>>,
    /// Native log windows each own one cancellable follow stream.
    log_streams: Mutex<HashMap<(i32, u64), JoinHandle<()>>>,
}

struct WorkerRuntime {
    sender: mpsc::SyncSender<WorkerResult>,
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
            WorkerCommand::LoadClusters => reload_kubeconfig().await,
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
                res
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
                    let handle = tokio::spawn(watch_resource_detail(
                        *cluster_key,
                        client,
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        resource_uid.clone(),
                        *history_entry_id,
                        result_channel.clone(),
                    ));
                    shared.detail_watches.lock().await.insert(watch_key, handle);
                    Ok(WorkerResult::ResourceDetailWatchStarted {
                        cluster_key: *cluster_key,
                        history_entry_id: *history_entry_id,
                    })
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
                Ok(WorkerResult::ResourceDetailWatchStopped {
                    cluster_key: *cluster_key,
                })
            }
            WorkerCommand::GetResourceYaml {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    get_resource_yaml(
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                    )
                    .await
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
                } else {
                    Err(anyhow::anyhow!(
                        "No client found for cluster_key {}",
                        cluster_key
                    ))
                }
            }
            WorkerCommand::ApplyResourceYaml {
                cluster_key,
                api_resource,
                namespace,
                resource_name,
                yaml,
            } => {
                let clients = shared.clients.read().await;
                if let Some(client) = clients.get(cluster_key) {
                    apply_resource_yaml(
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        yaml.clone(),
                    )
                    .await
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
                    update_resource_data(
                        *cluster_key,
                        client.clone(),
                        api_resource.clone(),
                        namespace.clone(),
                        resource_name.clone(),
                        &update.expected_values,
                        &update.updated_values,
                        &update.expected_resource_version,
                    )
                    .await
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
                    let cluster_key = *cluster_key;
                    let log_window_id = *log_window_id;
                    let stream_key = (cluster_key, log_window_id);
                    if let Some(previous) = shared.log_streams.lock().await.remove(&stream_key) {
                        previous.abort();
                    }
                    let task_shared = shared.clone();
                    let task_sender = result_channel.clone();
                    let namespace = namespace.clone();
                    let pod_name = pod_name.clone();
                    let container = container.clone();
                    let task = tokio::spawn(async move {
                        stream_pod_logs(
                            cluster_key,
                            log_window_id,
                            client,
                            namespace,
                            pod_name,
                            container,
                            task_sender,
                        )
                        .await;
                        task_shared.log_streams.lock().await.remove(&stream_key);
                    });
                    shared.log_streams.lock().await.insert(stream_key, task);
                    Ok(WorkerResult::PodLogStreamStarted {
                        cluster_key,
                        log_window_id,
                    })
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
                Ok(WorkerResult::PodLogStreamEnded {
                    cluster_key: *cluster_key,
                    log_window_id: *log_window_id,
                })
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
            Ok(result) => {
                result_channel
                    .send(result)
                    .log_if_error("Failed to send result notification");
            }
        }
    }
}

async fn stream_pod_logs(
    cluster_key: i32,
    log_window_id: u64,
    client: kube::Client,
    namespace: String,
    pod_name: String,
    container: String,
    sender: WorkerResultSender,
) {
    let result = async {
        let pods: Api<Pod> = Api::namespaced(client, &namespace);
        let stream = pods
            .log_stream(
                &pod_name,
                &LogParams {
                    container: Some(container),
                    follow: true,
                    ..LogParams::default()
                },
            )
            .await?;
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
                        sender.send(WorkerResult::PodLogLinesAppended {
                            cluster_key,
                            log_window_id,
                            lines: std::mem::take(&mut batch),
                        })?;
                    }
                }
                _ = flush.tick(), if !batch.is_empty() => {
                    sender.send(WorkerResult::PodLogLinesAppended {
                        cluster_key,
                        log_window_id,
                        lines: std::mem::take(&mut batch),
                    })?;
                }
            }
        }
        if !batch.is_empty() {
            sender.send(WorkerResult::PodLogLinesAppended {
                cluster_key,
                log_window_id,
                lines: batch,
            })?;
        }
        anyhow::Ok(())
    }
    .await;
    let result = match result {
        Ok(()) => WorkerResult::PodLogStreamEnded {
            cluster_key,
            log_window_id,
        },
        Err(error) => WorkerResult::PodLogStreamFailed {
            cluster_key,
            log_window_id,
            error: format!("{error:#}"),
        },
    };
    sender
        .send(result)
        .log_if_error("Failed to send Pod log stream result");
}
