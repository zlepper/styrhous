use super::*;

#[derive(Clone)]

pub(crate) struct WorkerState {
    pub(super) results: WorkerResultSender,
    /// Connected clusters and their root watcher tasks. This stays entirely on the
    /// worker side so UI state can never determine a Kubernetes task's lifetime.
    pub(super) connections: Arc<Mutex<HashMap<i32, ClusterConnection>>>,
    /// Resource watches are keyed by their complete scope and are aborted before
    /// replacement and whenever their cluster session is torn down.
    pub(super) resource_watches: Arc<Mutex<ResourceWatchRegistry>>,
    /// Detail watches remain active while their visit is retained in an
    /// inspector's history.
    pub(super) detail_watches: SharedTaskRegistry<(i32, u64)>,
    /// Namespace-scoped Metrics API pollers used only while Pods are visible.
    pub(super) pod_metrics_watches: SharedTaskRegistry<(i32, String)>,
    /// Cluster-scoped Metrics API pollers used only while Nodes are visible.
    pub(super) node_metrics_watches: SharedTaskRegistry<i32>,
    /// Native log windows each own one cancellable follow stream.
    pub(super) log_streams: SharedTaskRegistry<(i32, u64)>,
    /// Each connected cluster gets its own bounded pool for initial list/watch
    /// synchronization. A synchronized watch does not retain a permit.
    pub(super) watch_initialization_slots: Arc<Mutex<HashMap<i32, Arc<Semaphore>>>>,
    /// The bounded, disk-backed ingress for pod logs. A Kubernetes stream
    /// awaits this directly rather than routing log data through the UI.
    pub(super) log_store_appender: Option<LogStoreAppender>,
}

impl WorkerState {
    pub(super) fn new(
        results: WorkerResultSender,
        log_store_appender: Option<LogStoreAppender>,
    ) -> Self {
        Self {
            results,
            connections: Arc::new(Mutex::new(HashMap::new())),
            resource_watches: Arc::new(Mutex::new(ResourceWatchRegistry::default())),
            detail_watches: Arc::new(TaskRegistry::default()),
            pod_metrics_watches: Arc::new(TaskRegistry::default()),
            node_metrics_watches: Arc::new(TaskRegistry::default()),
            log_streams: Arc::new(TaskRegistry::default()),
            watch_initialization_slots: Arc::new(Mutex::new(HashMap::new())),
            log_store_appender,
        }
    }
}

#[derive(Default)]
pub(super) struct ResourceWatchRegistry {
    pub(super) watches: HashMap<ResourceScope, JoinHandle<()>>,
    generations: HashMap<(i32, ApiResource), u64>,
    sessions: HashMap<i32, u64>,
}

impl WorkerState {
    pub(super) async fn register_cluster_runtime(&self, cluster_key: i32) {
        self.resource_watches
            .lock()
            .await
            .sessions
            .entry(cluster_key)
            .or_insert(1);
        self.watch_initialization_slots
            .lock()
            .await
            .entry(cluster_key)
            .or_insert_with(|| Arc::new(Semaphore::new(16)));
    }

    pub(super) async fn watch_initialization_slot(&self, cluster_key: i32) -> Arc<Semaphore> {
        self.watch_initialization_slots
            .lock()
            .await
            .entry(cluster_key)
            .or_insert_with(|| Arc::new(Semaphore::new(16)))
            .clone()
    }
    pub(super) async fn client_for_cluster(
        &self,
        cluster_key: i32,
    ) -> anyhow::Result<kube::Client> {
        self.connections
            .lock()
            .await
            .get(&cluster_key)
            .map(ClusterConnection::client)
            .ok_or_else(|| anyhow::anyhow!("No client found for cluster_key {cluster_key}"))
    }

    pub(super) async fn stop_cluster(&self, cluster_key: i32) {
        self.connections.lock().await.remove(&cluster_key);
        self.invalidate_cluster_resource_watches(cluster_key).await;
        self.detail_watches
            .abort_matching(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
        self.pod_metrics_watches
            .abort_matching(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
        self.node_metrics_watches
            .abort_matching(|watch_cluster_key| *watch_cluster_key == cluster_key)
            .await;
        self.log_streams
            .abort_matching(|(watch_cluster_key, _)| *watch_cluster_key == cluster_key)
            .await;
        self.watch_initialization_slots
            .lock()
            .await
            .remove(&cluster_key);
    }

    pub(super) async fn stop_all_clusters(&self) {
        self.connections.lock().await.clear();
        self.invalidate_all_resource_watches().await;
        self.detail_watches.abort_all().await;
        self.pod_metrics_watches.abort_all().await;
        self.node_metrics_watches.abort_all().await;
        self.log_streams.abort_all().await;
        self.watch_initialization_slots.lock().await.clear();
    }

    #[cfg(test)]
    pub(super) async fn replace_resource_watch(&self, key: ResourceScope, task: JoinHandle<()>) {
        let previous = self.resource_watches.lock().await.watches.insert(key, task);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
    }

    /// Advance a resource's generation and return all currently active tasks
    /// for it. Starts from an earlier generation check this value immediately
    /// before installing their watcher, so queued starts cannot resurrect an
    /// obsolete namespace scope.
    pub(super) async fn replace_resource_watch_sources(
        &self,
        cluster_key: i32,
        api_resource: &ApiResource,
    ) -> u64 {
        let mut registry = self.resource_watches.lock().await;
        registry.sessions.entry(cluster_key).or_insert(1);
        let generation = {
            let generation = registry
                .generations
                .entry((cluster_key, api_resource.clone()))
                .and_modify(|generation| *generation += 1)
                .or_insert(1);
            *generation
        };
        let keys = registry
            .watches
            .keys()
            .filter(|scope| scope.cluster_key == cluster_key && scope.api_resource == *api_resource)
            .cloned()
            .collect::<Vec<_>>();
        let tasks = keys
            .into_iter()
            .filter_map(|key| registry.watches.remove(&key))
            .collect::<Vec<_>>();
        drop(registry);
        for task in tasks {
            abort_task(task).await;
        }
        generation
    }

    pub(super) async fn install_resource_watch_if_current(
        &self,
        key: ResourceScope,
        generation: u64,
        session: u64,
        task: JoinHandle<()>,
    ) -> bool {
        let mut registry = self.resource_watches.lock().await;
        if registry.sessions.get(&key.cluster_key).copied() != Some(session)
            || registry
                .generations
                .get(&(key.cluster_key, key.api_resource.clone()))
                .copied()
                != Some(generation)
        {
            drop(registry);
            abort_task(task).await;
            return false;
        }
        let previous = registry.watches.insert(key, task);
        drop(registry);
        if let Some(previous) = previous {
            abort_task(previous).await;
        }
        true
    }

    pub(super) async fn resource_watch_generation_is_current(
        &self,
        cluster_key: i32,
        api_resource: &ApiResource,
        generation: u64,
        session: u64,
    ) -> bool {
        let registry = self.resource_watches.lock().await;
        registry.sessions.get(&cluster_key).copied() == Some(session)
            && registry
                .generations
                .get(&(cluster_key, api_resource.clone()))
                .copied()
                == Some(generation)
    }

    pub(super) async fn resource_watch_session(&self, cluster_key: i32) -> u64 {
        *self
            .resource_watches
            .lock()
            .await
            .sessions
            .entry(cluster_key)
            .or_insert(1)
    }

    pub(super) async fn invalidate_cluster_resource_watches(&self, cluster_key: i32) {
        let mut registry = self.resource_watches.lock().await;
        *registry.sessions.entry(cluster_key).or_insert(1) += 1;
        registry
            .generations
            .retain(|(key, _), _| *key != cluster_key);
        let keys = registry
            .watches
            .keys()
            .filter(|scope| scope.cluster_key == cluster_key)
            .cloned()
            .collect::<Vec<_>>();
        let tasks = keys
            .into_iter()
            .filter_map(|key| registry.watches.remove(&key))
            .collect::<Vec<_>>();
        drop(registry);
        for task in tasks {
            abort_task(task).await;
        }
    }

    pub(super) async fn invalidate_all_resource_watches(&self) {
        let cluster_keys = {
            let registry = self.resource_watches.lock().await;
            registry
                .sessions
                .keys()
                .chain(registry.watches.keys().map(|scope| &scope.cluster_key))
                .copied()
                .collect::<std::collections::HashSet<_>>()
        };
        for cluster_key in cluster_keys {
            self.invalidate_cluster_resource_watches(cluster_key).await;
        }
    }

    pub(super) async fn replace_pod_metrics_watch(&self, key: (i32, String), task: JoinHandle<()>) {
        self.pod_metrics_watches.replace(key, task).await;
    }

    pub(super) async fn replace_node_metrics_watch(&self, key: i32, task: JoinHandle<()>) {
        self.node_metrics_watches.replace(key, task).await;
    }
}

pub(super) async fn abort_task(task: JoinHandle<()>) {
    task.abort();
    // A watcher may be in a synchronous result-channel send when cancellation
    // is requested. Bound the join so teardown cannot stall the command loop.
    let _ = tokio::time::timeout(Duration::from_millis(100), task).await;
}
