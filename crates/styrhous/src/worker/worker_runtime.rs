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
                ClusterRuntimeMessage::Command(command) if command.dispatch_in_order() => {
                    // A stop may be queued while its start is still connecting.
                    // Dispatch these in channel order so that stop cannot run
                    // before start registers the cancellable stream task.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[derive(Debug)]
    enum LogLifecycleAction {
        Start(u64),
        Stop(u64),
        Inspect(oneshot::Sender<(bool, bool)>),
    }

    #[derive(Debug)]
    struct LogLifecycleProbe(LogLifecycleAction);

    #[async_trait]
    impl WorkerCommand for LogLifecycleProbe {
        type Output = Result<NoResult, PodLogStreamFailed>;

        async fn execute(self, state: &WorkerState) -> Self::Output {
            match self.0 {
                LogLifecycleAction::Start(id) => {
                    // Reproduce a start that is still connecting when its stop is queued.
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    state
                        .log_streams
                        .replace_after_abort((1, id), || tokio::spawn(std::future::pending()))
                        .await;
                }
                LogLifecycleAction::Stop(id) => {
                    state.log_streams.abort(&(1, id)).await;
                }
                LogLifecycleAction::Inspect(sender) => {
                    let _ = sender.send((
                        state.log_streams.contains_key(&(1, 1)).await,
                        state.log_streams.contains_key(&(1, 2)).await,
                    ));
                }
            }
            Ok(NoResult)
        }

        fn cluster_key(&self) -> Option<i32> {
            Some(1)
        }

        fn dispatch_in_order(&self) -> bool {
            true
        }
    }

    #[test]
    fn ordered_log_commands_stop_a_connecting_session_before_starting_the_next() {
        let state = Arc::new(super::super::tests::worker_state());
        let handle = ClusterRuntimeHandle::start(1, state);
        let (result_sender, result_receiver) = oneshot::channel();
        for action in [
            LogLifecycleAction::Start(1),
            LogLifecycleAction::Stop(1),
            LogLifecycleAction::Start(2),
            LogLifecycleAction::Inspect(result_sender),
        ] {
            handle
                .sender
                .send(ClusterRuntimeMessage::Command(Box::new(LogLifecycleProbe(
                    action,
                ))))
                .expect("cluster runtime accepts command");
        }
        let runtime = tokio::runtime::Runtime::new().expect("test runtime starts");
        let (old_exists, new_exists) = runtime
            .block_on(async { tokio::time::timeout(Duration::from_secs(5), result_receiver).await })
            .expect("ordered commands finish")
            .expect("inspection is sent");
        assert!(!old_exists);
        assert!(new_exists);
        let (done_sender, done_receiver) = oneshot::channel();
        handle
            .sender
            .send(ClusterRuntimeMessage::Shutdown(done_sender))
            .expect("cluster runtime accepts shutdown");
        runtime
            .block_on(done_receiver)
            .expect("cluster runtime shuts down");
        handle.thread.join().expect("cluster thread joins");
    }
}
