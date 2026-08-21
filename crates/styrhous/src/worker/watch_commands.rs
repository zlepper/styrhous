use super::*;

#[async_trait]
impl WorkerCommand for ReconcileResourceWatches {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let generation = state
            .replace_resource_watch_sources(self.cluster_key, &self.api_resource)
            .await;
        let cluster_key = self.cluster_key;
        let session = state.resource_watch_session(cluster_key).await;
        for source in self.sources {
            let state = Arc::new(state.clone());
            let api_resource = self.api_resource.clone();
            tokio::spawn(async move {
                start_reconciled_resource_watch(
                    state,
                    cluster_key,
                    generation,
                    session,
                    api_resource,
                    source,
                )
                .await;
            });
        }
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

async fn start_reconciled_resource_watch(
    state: Arc<WorkerState>,
    cluster_key: i32,
    generation: u64,
    session: u64,
    api_resource: ApiResource,
    source: ResourceWatchSource,
) {
    let (namespace, watched_namespaces) = match source {
        ResourceWatchSource::Namespace(namespace) => (Some(namespace), None),
        ResourceWatchSource::AllNamespaces(namespaces) => (None, Some(namespaces)),
        ResourceWatchSource::Cluster => (None, None),
    };
    if !state
        .resource_watch_generation_is_current(cluster_key, &api_resource, generation, session)
        .await
    {
        return;
    }
    let failure = |error| KubernetesResourceWatchFailed {
        cluster_key,
        api_resource: api_resource.clone(),
        namespace: namespace.clone(),
        error,
    };
    let client = match state.client_for_cluster(cluster_key).await {
        Ok(client) => client,
        Err(error) => {
            let _ = state.results.send(failure(format!("{error:#?}"))).await;
            return;
        }
    };
    let initialization_slot = match state
        .watch_initialization_slot(cluster_key)
        .await
        .acquire_owned()
        .await
    {
        Ok(slot) => slot,
        Err(error) => {
            let _ = state
                .results
                .send(failure(format!(
                    "Unable to acquire watch initialization slot: {error}"
                )))
                .await;
            return;
        }
    };
    if !state
        .resource_watch_generation_is_current(cluster_key, &api_resource, generation, session)
        .await
    {
        return;
    }
    let (initialized_sender, initialized_receiver) = oneshot::channel();
    let started = if let Some(namespaces) = watched_namespaces {
        start_all_namespaces_resource_watcher(
            cluster_key,
            client,
            api_resource.clone(),
            namespaces,
            state.results.clone(),
            Some(initialized_sender),
        )
        .await
    } else {
        start_resource_watcher(
            cluster_key,
            client,
            api_resource.clone(),
            namespace.clone(),
            state.results.clone(),
            Some(initialized_sender),
        )
        .await
    };
    match started {
        Ok((result, task)) => {
            tokio::spawn(async move {
                let _ = initialized_receiver.await;
                drop(initialization_slot);
            });
            let key = ResourceScope {
                cluster_key,
                api_resource,
                namespace,
            };
            if state
                .install_resource_watch_if_current(key, generation, session, task)
                .await
            {
                state
                    .results
                    .send(result)
                    .await
                    .log_if_error("Failed to send resource watch start result");
            }
        }
        Err(error) => {
            let _ = state.results.send(failure(format!("{error:#?}"))).await;
        }
    }
}

#[async_trait]
impl WorkerCommand for StartResourceDetailWatch {
    type Output = Result<NoResult, ResourceDetailWatchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let failure = |error| ResourceDetailWatchFailed {
            cluster_key: self.cluster_key,
            history_entry_id: self.history_entry_id,
            events: false,
            error: format!("{error:#?}"),
        };
        let client = state
            .client_for_cluster(self.cluster_key)
            .await
            .map_err(failure)?;
        let key = (self.cluster_key, self.history_entry_id);
        let event_sender = state.results.clone();
        state
            .detail_watches
            .replace_after_abort(key, move || {
                tokio::spawn(watch_resource_detail(ResourceDetailWatchRequest {
                    cluster_key: self.cluster_key,
                    client,
                    api_resource: self.api_resource,
                    namespace: self.namespace,
                    resource_name: self.resource_name,
                    resource_uid: self.resource_uid,
                    history_entry_id: self.history_entry_id,
                    pod_metrics_api_available: self.pod_metrics_api_available,
                    node_metrics_api_available: self.node_metrics_api_available,
                    event_sender,
                }))
            })
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StopResourceDetailWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .detail_watches
            .abort(&(self.cluster_key, self.history_entry_id))
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StartPodMetricsWatch {
    type Output = Result<NoResult, PodMetricsWatchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let client = state
            .client_for_cluster(self.cluster_key)
            .await
            .map_err(|error| PodMetricsWatchFailed {
                cluster_key: self.cluster_key,
                namespace: self.namespace.clone(),
                error: format!("{error:#?}"),
            })?;
        let key = (self.cluster_key, self.namespace.clone());
        let task = tokio::spawn(watch_pod_metrics_namespace(
            self.cluster_key,
            client,
            self.namespace,
            state.results.clone(),
        ));
        state.replace_pod_metrics_watch(key, task).await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StopPodMetricsWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .pod_metrics_watches
            .abort_matching(|key| key.0 == self.cluster_key && key.1 == self.namespace)
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StartNodeMetricsWatch {
    type Output = Result<NoResult, NodeMetricsWatchFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let client = state
            .client_for_cluster(self.cluster_key)
            .await
            .map_err(|error| NodeMetricsWatchFailed {
                cluster_key: self.cluster_key,
                error: format!("{error:#?}"),
            })?;
        let task = tokio::spawn(watch_node_metrics(
            self.cluster_key,
            client,
            state.results.clone(),
        ));
        state
            .replace_node_metrics_watch(self.cluster_key, task)
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}

#[async_trait]
impl WorkerCommand for StopNodeMetricsWatch {
    type Output = Result<NoResult, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state
            .node_metrics_watches
            .abort_matching(|cluster_key| *cluster_key == self.cluster_key)
            .await;
        Ok(NoResult)
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}
