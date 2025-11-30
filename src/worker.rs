use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    Cluster, ClusterConnection, reload_kubeconfig, start_cluster_connection,
};
use crate::helpers::ResultExt;
use crate::minimal_namespace::MinimalNamespace;
use anyhow::Error;
use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use tracing::{info, warn};

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

            let worker = WorkerRuntime {
                sender: result_channel_sender,
                receiver: command_channel_receiver,
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
    None,
}

pub type WorkerResultSender = mpsc::SyncSender<WorkerResult>;

struct WorkerRuntime {
    sender: mpsc::SyncSender<WorkerResult>,
    receiver: mpsc::Receiver<WorkerCommand>,
}

impl WorkerRuntime {
    fn run(self) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        info!("Worker thread running");
        while let Ok(command) = self.receiver.recv() {
            runtime.spawn(WorkerRuntime::handle_command(self.sender.clone(), command));
        }
    }

    async fn handle_command(result_channel: WorkerResultSender, command: WorkerCommand) {
        let result = match &command {
            WorkerCommand::LoadClusters => reload_kubeconfig().await,
            WorkerCommand::ConnectToCluster {
                cluster_key,
                cluster,
            } => start_cluster_connection(*cluster_key, cluster, result_channel.clone()).await,
        };

        match result {
            Err(e) => {
                result_channel
                    .send(WorkerResult::CommandFailed {
                        command: Some(command),
                        error: e,
                    })
                    .log_if_error("Failed to send commend failed notification");
            }
            Ok(result) => {
                result_channel
                    .send(result)
                    .log_if_error("Failed to send result notification");
            }
        }
    }
}
