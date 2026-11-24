use super::*;
use futures_util::FutureExt;
use std::time::Duration;

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

/// Work for a cluster's ordered lane.
enum LaneMessage {
    Command(WorkerCommandBox),
    /// Answered once everything queued before it has finished.
    Barrier(oneshot::Sender<()>),
}

impl ClusterWorkerRuntime {
    fn run(mut self, runtime: tokio::runtime::Runtime) {
        let mut in_flight = Vec::<JoinHandle<()>>::new();
        // Commands whose relative order matters (a stop must not overtake its start) run one
        // at a time on this lane. It is a task of its own, so a slow member never delays the
        // independent requests the loop below spawns.
        let (lane, lane_receiver) = mpsc::unbounded_channel();
        runtime.spawn(run_ordered_lane(lane_receiver, self.state.clone()));
        while let Some(message) = self.receiver.blocking_recv() {
            match message {
                ClusterRuntimeMessage::Command(command)
                    if command.serializes_session_lifecycle() =>
                {
                    drain_commands(&runtime, &mut in_flight, &lane);
                    runtime.block_on(dispatch_command(command, self.state.clone()));
                }
                ClusterRuntimeMessage::Command(command) if command.dispatch_in_order() => {
                    if lane.send(LaneMessage::Command(command)).is_err() {
                        tracing::error!(
                            cluster_key = self.cluster_key,
                            "Ordered command lane closed"
                        );
                    }
                }
                ClusterRuntimeMessage::Command(command) => {
                    in_flight.retain(|task| !task.is_finished());
                    let state = self.state.clone();
                    in_flight.push(runtime.spawn(dispatch_command(command, state)));
                }
                ClusterRuntimeMessage::Shutdown(done) => {
                    drain_commands(&runtime, &mut in_flight, &lane);
                    runtime.block_on(self.state.stop_cluster(self.cluster_key));
                    let _ = done.send(());
                    return;
                }
            }
        }
        drain_commands(&runtime, &mut in_flight, &lane);
        runtime.block_on(self.state.stop_cluster(self.cluster_key));
    }
}

async fn run_ordered_lane(
    mut messages: mpsc::UnboundedReceiver<LaneMessage>,
    state: Arc<WorkerState>,
) {
    // A member that hangs would hold up every later ordered command, lifecycle command
    // and shutdown. Some of them legitimately wait for an exec credential plugin.
    let deadline = state.request_deadline * 3;
    while let Some(message) = messages.recv().await {
        match message {
            LaneMessage::Command(command) => {
                dispatch_command_within(command, state.clone(), Some(deadline)).await;
            }
            LaneMessage::Barrier(done) => {
                let _ = done.send(());
            }
        }
    }
}

fn drain_commands(
    runtime: &tokio::runtime::Runtime,
    tasks: &mut Vec<JoinHandle<()>>,
    lane: &mpsc::UnboundedSender<LaneMessage>,
) {
    let pending = std::mem::take(tasks);
    let (reached, lane_drained) = oneshot::channel();
    if lane.send(LaneMessage::Barrier(reached)).is_err() {
        tracing::error!("Ordered command lane closed before a barrier");
    }
    runtime.block_on(async {
        let _ = lane_drained.await;
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
    dispatch_command_within(command, state, None).await;
}

/// Run `command` and send its result. A command that panics, or that has not finished
/// within `deadline`, is answered with its typed failure when it has one, and is logged
/// either way, so whatever awaits its result is not left waiting.
async fn dispatch_command_within(
    command: WorkerCommandBox,
    state: Arc<WorkerState>,
    deadline: Option<Duration>,
) {
    // Read under the guard too: the ordered lane must survive a hook that panics.
    let (name, abandoned) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (command.command_name(), command.failure_if_abandoned())
    }))
    .unwrap_or(("unknown command", None));
    let name = name.rsplit("::").next().unwrap_or(name);
    let execution = std::panic::AssertUnwindSafe(command.execute_boxed(&state)).catch_unwind();
    let outcome = match deadline {
        None => Some(execution.await),
        Some(deadline) => tokio::time::timeout(deadline, execution).await.ok(),
    };
    let result = match outcome {
        Some(Ok(result)) => result,
        Some(Err(payload)) => {
            let message = panic_message(payload.as_ref());
            tracing::error!(command = name, panic = %message, "Worker command panicked");
            abandoned.map(|failure| failure(format!("{name} panicked: {message}")))
        }
        None => {
            let deadline = deadline.unwrap_or_default();
            tracing::error!(
                command = name,
                ?deadline,
                "Worker command abandoned as hung"
            );
            abandoned.map(|failure| failure(format!("{name} did not finish within {deadline:?}")))
        }
    };
    if let Some(result) = result {
        state
            .results
            .send_box(result)
            .await
            .log_if_error("Failed to send worker result");
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
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

    /// What the probes report to the test, in the order they finish.
    type Report = std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>;

    /// How a probe panics.
    #[derive(Debug, Clone, Copy)]
    enum Panic {
        Str,
        String,
        Other,
    }

    #[derive(Debug)]
    struct Probe {
        label: &'static str,
        report: Report,
        /// The probe waits for this before it finishes.
        gate: Option<oneshot::Receiver<()>>,
        /// Fired when the probe starts executing.
        started: Option<oneshot::Sender<()>>,
        ordered: bool,
        lifecycle: bool,
        panics: Option<Panic>,
        /// Whether it has a typed failure to answer with when abandoned.
        answers_failure: bool,
    }

    impl Probe {
        fn new(label: &'static str, report: &Report) -> Self {
            Self {
                label,
                report: report.clone(),
                gate: None,
                started: None,
                ordered: false,
                lifecycle: false,
                panics: None,
                answers_failure: false,
            }
        }

        fn ordered(mut self) -> Self {
            self.ordered = true;
            self
        }

        fn lifecycle(mut self) -> Self {
            self.lifecycle = true;
            self
        }

        fn panicking(mut self, how: Panic) -> Self {
            self.panics = Some(how);
            self
        }

        fn answering_failures(mut self) -> Self {
            self.answers_failure = true;
            self
        }

        /// Holds the probe until the returned sender fires.
        fn gated(mut self) -> (Self, oneshot::Sender<()>) {
            let (release, gate) = oneshot::channel();
            self.gate = Some(gate);
            (self, release)
        }

        /// Reports when the probe is executing.
        fn watched(mut self) -> (Self, oneshot::Receiver<()>) {
            let (started, receiver) = oneshot::channel();
            self.started = Some(started);
            (self, receiver)
        }
    }

    #[derive(Debug)]
    struct ProbeFailure(String);

    impl WorkerResult for ProbeFailure {
        fn apply(self, _: &mut crate::ui::state::UiState, _: &mut Vec<WorkerCommandBox>) {}
    }

    #[async_trait]
    impl WorkerCommand for Probe {
        type Output = Result<NoResult, PodLogStreamFailed>;

        async fn execute(mut self, _state: &WorkerState) -> Self::Output {
            if let Some(started) = self.started.take() {
                let _ = started.send(());
            }
            if let Some(gate) = self.gate.take() {
                let _ = gate.await;
            }
            match self.panics {
                Some(Panic::Str) => panic!("probe panics on purpose"),
                Some(Panic::String) => panic!("{} panics on purpose", self.label),
                Some(Panic::Other) => std::panic::panic_any(42_u8),
                None => {}
            }
            self.report.lock().unwrap().push(self.label);
            Ok(NoResult)
        }

        fn cluster_key(&self) -> Option<i32> {
            Some(1)
        }

        fn dispatch_in_order(&self) -> bool {
            self.ordered
        }

        fn serializes_session_lifecycle(&self) -> bool {
            self.lifecycle
        }

        fn failure_if_abandoned(&self) -> Option<AbandonedFailure> {
            self.answers_failure
                .then(|| abandoned_failure(ProbeFailure))
                .flatten()
        }
    }

    /// A state whose ordered lane gives up after 60 ms, and its results.
    fn quick_state() -> (Arc<WorkerState>, mpsc::Receiver<WorkerResultBox>) {
        let (sender, receiver) = mpsc::channel(16);
        let mut state = WorkerState::new(WorkerResultSender::new(sender, None), None);
        state.request_deadline = Duration::from_millis(20);
        (Arc::new(state), receiver)
    }

    fn failure_text(result: &WorkerResultBox) -> &str {
        &result
            .as_ref()
            .as_any()
            .downcast_ref::<ProbeFailure>()
            .expect("the probe's failure result")
            .0
    }

    fn send(handle: &ClusterRuntimeHandle, probe: Probe) {
        handle
            .sender
            .send(ClusterRuntimeMessage::Command(Box::new(probe)))
            .expect("cluster runtime accepts command");
    }

    async fn eventually(report: &Report, done: impl Fn(&[&str]) -> bool) -> Vec<&'static str> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let finished = report.lock().unwrap().clone();
                if done(&finished) {
                    return finished;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out; finished so far: {:?}", report.lock().unwrap()))
    }

    fn request_shutdown(handle: &ClusterRuntimeHandle) -> oneshot::Receiver<()> {
        let (done_sender, done_receiver) = oneshot::channel();
        handle
            .sender
            .send(ClusterRuntimeMessage::Shutdown(done_sender))
            .expect("cluster runtime accepts shutdown");
        done_receiver
    }

    async fn shutdown(handle: ClusterRuntimeHandle) {
        tokio::time::timeout(Duration::from_secs(5), request_shutdown(&handle))
            .await
            .expect("the cluster runtime shuts down in time")
            .expect("the cluster runtime reports its shutdown");
        handle.thread.join().expect("cluster thread joins");
    }

    #[tokio::test]
    async fn ordered_log_commands_stop_a_connecting_session_before_starting_the_next() {
        let handle = ClusterRuntimeHandle::start(1, quick_state().0);
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
        let (old_exists, new_exists) =
            tokio::time::timeout(Duration::from_secs(5), result_receiver)
                .await
                .expect("ordered commands finish")
                .expect("inspection is sent");
        assert!(!old_exists);
        assert!(new_exists);
        shutdown(handle).await;
    }

    #[tokio::test]
    async fn a_slow_ordered_command_does_not_delay_independent_requests() {
        let handle = ClusterRuntimeHandle::start(1, quick_state().0);
        let report = Report::default();
        let (slow, release) = Probe::new("slow ordered start", &report).ordered().gated();
        let (slow, slow_started) = slow.watched();
        send(&handle, slow);
        slow_started.await.expect("the slow command starts");

        send(&handle, Probe::new("independent request", &report));
        eventually(&report, |done| done == ["independent request"]).await;

        release.send(()).expect("the slow command is waiting");
        eventually(&report, |done| done.len() == 2).await;
        shutdown(handle).await;
    }

    #[tokio::test]
    async fn ordered_commands_still_run_one_at_a_time_in_arrival_order() {
        let handle = ClusterRuntimeHandle::start(1, quick_state().0);
        let report = Report::default();
        let (start, release) = Probe::new("start", &report).ordered().gated();
        let (start, start_began) = start.watched();
        send(&handle, start);
        start_began.await.expect("start begins");
        // Were it not ordered, the ungated stop would finish while start is held.
        send(&handle, Probe::new("stop", &report).ordered());

        release.send(()).expect("start is waiting");
        let finished = eventually(&report, |done| done.len() == 2).await;
        assert_eq!(finished, ["start", "stop"]);
        shutdown(handle).await;
    }

    #[tokio::test]
    async fn a_lifecycle_command_waits_for_earlier_ordered_and_in_flight_commands() {
        let handle = ClusterRuntimeHandle::start(1, quick_state().0);
        let report = Report::default();
        let (ordered, release_ordered) = Probe::new("ordered", &report).ordered().gated();
        let (ordered, ordered_began) = ordered.watched();
        let (request, release_request) = Probe::new("request", &report).gated();
        let (request, request_began) = request.watched();
        send(&handle, ordered);
        send(&handle, request);
        ordered_began.await.expect("ordered begins");
        request_began.await.expect("request begins");
        send(&handle, Probe::new("lifecycle", &report).lifecycle());

        release_ordered.send(()).expect("ordered is waiting");
        release_request.send(()).expect("request is waiting");
        let finished = eventually(&report, |done| done.len() == 3).await;
        assert_eq!(finished.last(), Some(&"lifecycle"), "{finished:?}");
        shutdown(handle).await;
    }

    #[tokio::test]
    async fn a_panicking_ordered_command_does_not_stop_the_lane() {
        let (state, mut results) = quick_state();
        let handle = ClusterRuntimeHandle::start(1, state);
        let report = Report::default();
        send(
            &handle,
            Probe::new("panics", &report)
                .ordered()
                .panicking(Panic::Str)
                .answering_failures(),
        );
        send(&handle, Probe::new("after", &report).ordered());

        eventually(&report, |done| done == ["after"]).await;
        let failure = results.try_recv().expect("the panic is answered");
        assert!(failure_text(&failure).contains("panicked"));
        shutdown(handle).await;
    }

    #[tokio::test]
    async fn shutdown_finishes_although_an_ordered_command_hangs() {
        let (state, mut results) = quick_state();
        let handle = ClusterRuntimeHandle::start(1, state);
        let report = Report::default();
        // Never released: only the lane's deadline can end it.
        let (hung, _never_released) = Probe::new("hung", &report)
            .ordered()
            .answering_failures()
            .gated();
        send(&handle, hung);

        shutdown(handle).await;

        let failure = results.try_recv().expect("the hung command is answered");
        assert!(
            failure_text(&failure).contains("did not finish within 60ms"),
            "{}",
            failure_text(&failure)
        );
        assert!(report.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn shutdown_waits_for_pending_requests_but_not_for_a_hung_ordered_command() {
        let (state, mut results) = quick_state();
        let handle = ClusterRuntimeHandle::start(1, state);
        let report = Report::default();
        let (hung, _never_released) = Probe::new("hung", &report)
            .ordered()
            .answering_failures()
            .gated();
        let (request, release_request) = Probe::new("request", &report).gated();
        let (request, request_began) = request.watched();
        send(&handle, hung);
        send(&handle, request);
        request_began.await.expect("request begins");

        let mut done = request_shutdown(&handle);
        // The lane abandons its hung command, yet the request still holds shutdown back.
        let abandoned = results.recv().await.expect("the hung command is answered");
        assert!(failure_text(&abandoned).contains("did not finish"));
        assert!(matches!(
            done.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));

        release_request.send(()).expect("request is waiting");
        tokio::time::timeout(Duration::from_secs(5), done)
            .await
            .expect("shutdown finishes once the request does")
            .expect("shutdown is reported");
        assert_eq!(*report.lock().unwrap(), ["request"]);
        handle.thread.join().expect("cluster thread joins");
    }

    #[tokio::test]
    async fn a_panicking_command_reports_its_typed_failure_and_the_worker_survives() {
        let (state, mut results) = quick_state();
        let report = Report::default();
        for (how, expected) in [
            (Panic::Str, "Probe panicked: probe panics on purpose"),
            (Panic::String, "Probe panicked: panicking panics on purpose"),
            (Panic::Other, "Probe panicked: unknown panic"),
        ] {
            let probe = Probe::new("panicking", &report)
                .panicking(how)
                .answering_failures();
            dispatch_command(Box::new(probe), state.clone()).await;
            let failure = results.try_recv().expect("a failure result is sent");
            assert_eq!(failure_text(&failure), expected);
        }

        dispatch_command(Box::new(Probe::new("next", &report)), state).await;
        assert_eq!(*report.lock().unwrap(), ["next"]);
    }

    #[tokio::test]
    async fn a_panicking_command_without_a_typed_failure_sends_nothing() {
        let (state, mut results) = quick_state();
        let report = Report::default();

        dispatch_command(
            Box::new(Probe::new("silent", &report).panicking(Panic::Str)),
            state,
        )
        .await;

        assert!(results.try_recv().is_err());
    }

    #[test]
    fn panic_payloads_of_any_type_have_a_message() {
        assert_eq!(panic_message(&"static text"), "static text");
        assert_eq!(panic_message(&"owned text".to_owned()), "owned text");
        assert_eq!(panic_message(&7_u8), "unknown panic");
    }
}
