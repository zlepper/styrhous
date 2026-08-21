use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    AvailableAksCluster, AvailableTailscaleCluster, Cluster, ClusterConnection,
    ClusterDiscoveryTools, ResourceDataUpdateRequest, ResourceDetailWatchRequest,
    ResourceYamlValidationRequest, add_aks_cluster, add_tailscale_cluster, apply_resource_yaml,
    delete_resource, discover_managed_clusters, force_delete_resource, get_resource_scale,
    get_resource_schema, get_resource_yaml, reload_kubeconfig, restart_deployment, run_cron_job,
    start_all_namespaces_resource_watcher, start_cluster_connection, start_resource_watcher,
    update_resource_data, update_resource_scale, validate_resource_yaml, watch_node_metrics,
    watch_pod_metrics_namespace, watch_resource_detail,
};
use crate::helm_release::HelmRelease;
use crate::helpers::ResultExt;
use crate::log_store::LogStoreAppender;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::{NodeUsage, PodUsage};
use crate::resource_detail::{ManagedResource, ResourceDetail, ResourceEvent};
use crate::resource_schema::ResourceSchema;
use crate::resource_table::CustomResourceColumn;
use anyhow::Error;
use async_trait::async_trait;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::info;

mod pod_logs;

#[allow(dead_code)]
pub(crate) trait AsAny: Any {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The concrete command API. It is deliberately not object-safe: each command
/// owns itself, receives worker state, and returns its normal Rust output.
#[async_trait]
pub(crate) trait WorkerCommand: Send + std::fmt::Debug + 'static {
    type Output: CommandOutput;

    async fn execute(self, state: &WorkerState) -> Self::Output;

    fn serializes_session_lifecycle(&self) -> bool {
        false
    }

    /// Commands carrying a cluster key are routed to that cluster's isolated
    /// runtime. Commands without one execute in the router runtime.
    fn cluster_key(&self) -> Option<i32> {
        None
    }
}

/// The channel-only, object-safe adapter for concrete commands.
#[async_trait]
pub(crate) trait ErasedWorkerCommand: AsAny + Send + std::fmt::Debug {
    async fn execute_boxed(self: Box<Self>, state: &WorkerState) -> Option<WorkerResultBox>;
    fn serializes_session_lifecycle(&self) -> bool;
    fn cluster_key(&self) -> Option<i32>;
}

#[async_trait]
impl<C: WorkerCommand> ErasedWorkerCommand for C {
    async fn execute_boxed(self: Box<Self>, state: &WorkerState) -> Option<WorkerResultBox> {
        (*self).execute(state).await.into_result_box()
    }

    fn serializes_session_lifecycle(&self) -> bool {
        WorkerCommand::serializes_session_lifecycle(self)
    }

    fn cluster_key(&self) -> Option<i32> {
        WorkerCommand::cluster_key(self)
    }
}

pub(crate) type WorkerCommandBox = Box<dyn ErasedWorkerCommand>;

/// Converts a concrete command output into an optional channel update.
/// Commands whose successful outcome is purely worker-local use `NoResult`,
/// while still forwarding a typed failure result.
pub(crate) trait CommandOutput: Send + std::fmt::Debug + 'static {
    fn into_result_box(self) -> Option<WorkerResultBox>;
}

impl<R: WorkerResult> CommandOutput for R {
    fn into_result_box(self) -> Option<WorkerResultBox> {
        Some(Box::new(self))
    }
}

#[derive(Debug)]
pub(crate) struct NoResult;

impl<E: WorkerResult> CommandOutput for Result<NoResult, E> {
    fn into_result_box(self) -> Option<WorkerResultBox> {
        self.err().map(|error| Box::new(error) as WorkerResultBox)
    }
}

/// A concrete UI update emitted by the worker.
pub(crate) trait WorkerResult: Send + std::fmt::Debug + 'static {
    fn apply(self, ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>);
}

impl<S: WorkerResult, E: WorkerResult> WorkerResult for Result<S, E> {
    fn apply(self, ui: &mut crate::ui::state::UiState, commands: &mut Vec<WorkerCommandBox>) {
        match self {
            Ok(result) => result.apply(ui, commands),
            Err(result) => result.apply(ui, commands),
        }
    }
}

impl WorkerResult for () {
    fn apply(self, _ui: &mut crate::ui::state::UiState, _commands: &mut Vec<WorkerCommandBox>) {}
}

/// The channel-only, object-safe adapter for concrete UI updates.
pub(crate) trait ErasedWorkerResult: AsAny + Send + std::fmt::Debug {
    fn apply_boxed(
        self: Box<Self>,
        ui: &mut crate::ui::state::UiState,
        commands: &mut Vec<WorkerCommandBox>,
    );
}

impl<R: WorkerResult> ErasedWorkerResult for R {
    fn apply_boxed(
        self: Box<Self>,
        ui: &mut crate::ui::state::UiState,
        commands: &mut Vec<WorkerCommandBox>,
    ) {
        (*self).apply(ui, commands);
    }
}

pub(crate) type WorkerResultBox = Box<dyn ErasedWorkerResult>;

/// Trait abstracting the worker interface for testability
pub trait WorkerTrait: Default {
    fn with_repaint_context(_context: egui::Context) -> Self {
        Self::default()
    }

    fn start(&mut self);
    fn get_next_message(&mut self) -> Option<WorkerResultBox>;
    fn send_command(&mut self, command: WorkerCommandBox);
    fn set_log_store_appender(&mut self, _appender: LogStoreAppender) {}
}

#[derive(Default)]
pub struct Worker {
    inner: Option<WorkerInner>,
    pending_commands: VecDeque<WorkerCommandBox>,
    repaint_context: Option<egui::Context>,
    log_store_appender: Option<LogStoreAppender>,
}

impl Worker {
    pub(crate) fn with_repaint_context(context: egui::Context) -> Self {
        Self {
            inner: None,
            pending_commands: VecDeque::new(),
            repaint_context: Some(context),
            log_store_appender: None,
        }
    }

    pub fn start(&mut self) {
        if self.inner.is_none() {
            let (command_channel_sender, command_channel_receiver) =
                mpsc::channel::<WorkerCommandBox>(64);
            let (result_channel_sender, result_channel_receiver) = mpsc::channel(1024);
            let result_sender =
                WorkerResultSender::new(result_channel_sender, self.repaint_context.clone());

            command_channel_sender
                .try_send(Box::new(LoadClusters))
                .expect("Failed to send initial LoadClusters command");

            let state = Arc::new(WorkerState::new(
                result_sender,
                self.log_store_appender.clone(),
            ));
            let worker = WorkerRuntime::new(command_channel_receiver, state);

            let _ = std::thread::spawn(move || {
                worker.run();
            });

            self.inner = Some(WorkerInner {
                receiver: result_channel_receiver,
                sender: command_channel_sender,
            })
        }
    }

    pub(crate) fn get_next_message(&mut self) -> Option<WorkerResultBox> {
        self.flush_pending_commands();
        if let Some(inner) = &mut self.inner {
            inner.receiver.try_recv().ok()
        } else {
            None
        }
    }

    pub(crate) fn send_command(&mut self, command: WorkerCommandBox) {
        self.pending_commands.push_back(command);
        self.flush_pending_commands();
    }

    /// Move as many queued UI commands as the bounded worker channel currently accepts.
    /// This method deliberately never waits: a slow Kubernetes operation must not freeze the UI.
    fn flush_pending_commands(&mut self) {
        let Some(inner) = &self.inner else {
            return;
        };
        while let Some(command) = self.pending_commands.pop_front() {
            match inner.sender.try_send(command) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(command)) => {
                    self.pending_commands.push_front(command);
                    // Channel capacity changes do not themselves wake egui. Keep retrying at a
                    // modest cadence so queued stop commands are delivered even when the worker
                    // command produces no result and the user is otherwise idle.
                    if let Some(context) = &self.repaint_context {
                        context.request_repaint_after(Duration::from_millis(10));
                    }
                    break;
                }
                Err(mpsc::error::TrySendError::Closed(command)) => {
                    tracing::error!(?command, "Worker command channel closed");
                }
            }
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

    fn get_next_message(&mut self) -> Option<WorkerResultBox> {
        Worker::get_next_message(self)
    }

    fn send_command(&mut self, command: WorkerCommandBox) {
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
    pub results: VecDeque<WorkerResultBox>,
    pub commands: Vec<WorkerCommandBox>,
}

#[cfg(test)]
impl MockWorker {
    /// Queue a typed worker update for delivery on the next UI frame.
    pub fn enqueue_result<R: WorkerResult>(&mut self, result: R) {
        self.results.push_back(Box::new(result));
    }

    /// Return the latest command when it has the requested concrete type.
    pub fn last_command<C: WorkerCommand>(&self) -> Option<&C> {
        self.commands
            .last()
            .and_then(|command| command.as_ref().as_any().downcast_ref())
    }

    /// Iterate over the commands of one concrete type in their dispatch order.
    pub fn commands_of<C: WorkerCommand>(&self) -> impl Iterator<Item = &C> {
        self.commands
            .iter()
            .filter_map(|command| command.as_ref().as_any().downcast_ref())
    }
}

#[cfg(test)]
impl WorkerTrait for MockWorker {
    fn start(&mut self) {
        // No-op for mock
    }

    fn get_next_message(&mut self) -> Option<WorkerResultBox> {
        self.results.pop_front()
    }

    fn send_command(&mut self, command: WorkerCommandBox) {
        self.commands.push(command);
    }
}

struct WorkerInner {
    sender: mpsc::Sender<WorkerCommandBox>,
    receiver: mpsc::Receiver<WorkerResultBox>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
/// Worker watch registry key for a resource scope.
struct ResourceScope {
    pub cluster_key: i32,
    pub api_resource: ApiResource,
    pub namespace: Option<String>,
}

mod cluster_commands;
mod command_types;
mod log_commands;
mod resource_commands;
mod result_types;
mod results;
mod task_registry;
mod watch_commands;
mod worker_runtime;
mod worker_state;
mod yaml_commands;

pub(crate) use command_types::*;
pub(crate) use result_types::*;
pub(crate) use results::*;
use task_registry::*;
use worker_runtime::*;
pub(crate) use worker_state::*;

#[cfg(test)]
mod tests;
