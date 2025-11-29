use std::sync::mpsc;
use anyhow::Error;
use tracing::{info, warn};
use crate::cluster_connection_manager::{reload_kubeconfig, Cluster};

#[derive(Default)]
pub struct Worker {
    inner: Option<WorkerInner>,
}

impl Worker {
    pub fn start(&mut self) {
        if self.inner.is_none() {
            let (command_channel_sender, command_channel_receiver) = std::sync::mpsc::sync_channel(10);
            let (result_channel_sender, result_channel_receiver) = std::sync::mpsc::sync_channel(1024);

            command_channel_sender.send(WorkerCommand::LoadClusters).expect("Failed to send initial LoadClusters command");

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
}

struct WorkerInner {
    sender: mpsc::SyncSender<WorkerCommand>,
    receiver: mpsc::Receiver<WorkerResult>,
    worker_thread: std::thread::JoinHandle<()>,
}

/// Messages that can be sent to the worker
#[derive(Debug)]
pub enum WorkerCommand {
    LoadClusters
}

/// Messages that can be received from the worker
#[derive(Debug)]
pub enum WorkerResult {
    CommandFailed {
        command: WorkerCommand,
        error: Error,
    },
    KubernetesClustersUpdated(Vec<Cluster>)
}

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
            runtime.spawn(WorkerRuntime::handle_command(self.sender.clone(),command));
        }


    }

    async fn handle_command(result_channel: mpsc::SyncSender<WorkerResult>, command: WorkerCommand) {
        let result = match command {
            WorkerCommand::LoadClusters => {
                reload_kubeconfig()
            }
        }.await;

        match result {
            Err(e) => {
                if let Err(send_error) = result_channel.send(WorkerResult::CommandFailed {
                    command,
                    error: e,
                }) {
                    warn!("Failed to send command failed notification: {:?}", send_error);
                }
            }
            Ok(result) => {
                if let Err(send_error) = result_channel.send(result) {
                    warn!("Failed to send command result notification: {:?}", send_error);
                }
            }
        }
    }

}
