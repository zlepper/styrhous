use super::*;

pub(super) struct WorkerRuntime {
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
    pub(super) fn new(receiver: mpsc::Receiver<WorkerCommandBox>, state: Arc<WorkerState>) -> Self {
        Self { receiver, state }
    }

    pub(super) fn run(mut self) {
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

pub(super) async fn dispatch_command(command: WorkerCommandBox, state: Arc<WorkerState>) {
    if let Some(result) = command.execute_boxed(&state).await {
        state
            .results
            .send_box(result)
            .await
            .log_if_error("Failed to send worker result");
    }
}
