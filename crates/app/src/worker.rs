use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    Cluster, ClusterConnection, reload_kubeconfig, start_cluster_connection,
    start_resource_watcher, get_resource_yaml, delete_resource, apply_resource_yaml,
};
use crate::helpers::ResultExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use anyhow::Error;
#[cfg(test)]
use std::collections::VecDeque;
use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use tokio::sync::RwLock;
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
    ConnectToCluster { cluster: String, cluster_key: i32 },
    StartResourceWatch {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
    },
    GetResourceYaml {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resource_name: String,
    },
    DeleteResource {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resource_name: String,
    },
    ApplyResourceYaml {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resource_name: String,
        yaml: String,
    },
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
    KubernetesApisLoaded {
        cluster_key: i32,
        api_resources: Vec<ApiResource>,
    },
    KubernetesClusterConnectionCreated {
        cluster_key: i32,
        runner: ClusterConnection,
    },
    /// A resource was added or updated
    KubernetesResourceAdded {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resource: MinimalResource,
    },
    /// A resource was deleted
    KubernetesResourceDeleted {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resource_uid: String,
    },
    /// Initial resource list complete
    KubernetesResourcesReplaced {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resources: Vec<MinimalResource>,
    },
    /// Resource watcher started successfully
    KubernetesResourceWatchStarted {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
    },
    /// Resource YAML fetched for viewing/editing
    ResourceYamlFetched {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resource_name: String,
        yaml: String,
    },
    /// Resource was successfully deleted
    ResourceDeleteCompleted {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resource_name: String,
    },
    /// Resource YAML was successfully applied
    ResourceApplyCompleted {
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: String,
        resource_name: String,
    },
}

pub type WorkerResultSender = mpsc::SyncSender<WorkerResult>;

/// Shared state accessible from spawned async tasks
struct SharedWorkerState {
    /// Kube clients indexed by cluster_key
    clients: RwLock<HashMap<i32, kube::Client>>,
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
                let res = start_cluster_connection(*cluster_key, cluster, result_channel.clone()).await;
                // Store the client for later use by resource watchers
                if let Ok(WorkerResult::KubernetesClusterConnectionCreated { cluster_key, runner }) = &res {
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
                    ).await
                } else {
                    Err(anyhow::anyhow!("No client found for cluster_key {}", cluster_key))
                }
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
                    ).await
                } else {
                    Err(anyhow::anyhow!("No client found for cluster_key {}", cluster_key))
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
                    ).await
                } else {
                    Err(anyhow::anyhow!("No client found for cluster_key {}", cluster_key))
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
                    ).await
                } else {
                    Err(anyhow::anyhow!("No client found for cluster_key {}", cluster_key))
                }
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
