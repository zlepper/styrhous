use super::*;

#[test]
fn task_registry_replaces_and_selectively_aborts_tasks() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let registry = TaskRegistry::default();
        let first_aborted = Arc::new(AtomicUsize::new(0));
        let second_aborted = Arc::new(AtomicUsize::new(0));
        let retained_aborted = Arc::new(AtomicUsize::new(0));

        registry
            .replace(1, tokio::spawn(AbortProbe(first_aborted.clone())))
            .await;
        registry
            .replace_after_abort(1, || tokio::spawn(AbortProbe(second_aborted.clone())))
            .await;
        registry
            .replace(2, tokio::spawn(AbortProbe(retained_aborted.clone())))
            .await;
        tokio::task::yield_now().await;

        assert_eq!(first_aborted.load(Ordering::Relaxed), 1);
        registry.abort_matching(|key| *key == 1).await;
        tokio::task::yield_now().await;

        assert_eq!(second_aborted.load(Ordering::Relaxed), 1);
        assert_eq!(retained_aborted.load(Ordering::Relaxed), 0);
        assert!(registry.contains_key(&2).await);
        registry.abort(&2).await;
        tokio::task::yield_now().await;
        assert_eq!(retained_aborted.load(Ordering::Relaxed), 1);
        assert!(registry.is_empty().await);
    });
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
            connections: Arc::new(Mutex::new(HashMap::new())),
            resource_watches: Arc::new(Mutex::new(ResourceWatchRegistry::default())),
            detail_watches: Arc::new(TaskRegistry::default()),
            pod_metrics_watches: Arc::new(TaskRegistry::default()),
            node_metrics_watches: Arc::new(TaskRegistry::default()),
            log_streams: Arc::new(TaskRegistry::default()),
            watch_initialization_slots: Arc::new(Mutex::new(HashMap::new())),
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
