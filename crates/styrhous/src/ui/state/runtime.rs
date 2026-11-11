use super::*;

impl UiState {
    pub(crate) fn settle_bulk_delete_target(
        &mut self,
        cluster_key: i32,
        bulk_delete_id: Option<u64>,
        api_resource: &ApiResource,
        resource_name: &str,
        namespace: &Option<String>,
        failure: Option<String>,
    ) -> Option<BulkDeleteOutcome> {
        let cluster = self.clusters.get_mut(&cluster_key)?;
        let progress = cluster.resources.bulk_delete_progress.as_mut()?;
        if bulk_delete_id != Some(progress.id) {
            return None;
        }
        let target = progress.target_for(api_resource, resource_name, namespace)?;

        let api_resource = progress.api_resource.clone();
        progress.remaining_targets.remove(&target);
        if let Some(failure) = failure {
            progress.failures.push((target, failure));
        } else if let Some(selection) = cluster.resources.resource_selections.get_mut(&api_resource)
        {
            selection.remove(&target.uid);
        }

        if !progress.remaining_targets.is_empty() {
            return None;
        }

        let failures = std::mem::take(&mut progress.failures);
        let outcome = BulkDeleteOutcome {
            api_resource: progress.api_resource.clone(),
            target_count: progress.target_count,
            failures,
        };
        cluster.resources.bulk_delete_progress = None;
        Some(outcome)
    }
}

impl UiState {
    pub(crate) fn update<W: WorkerTrait>(&mut self, worker: &mut W) -> Vec<WorkerCommandBox> {
        let mut commands_to_send = Vec::new();
        while let Some(result) = worker.get_next_message() {
            result.apply_boxed(self, &mut commands_to_send);
        }
        commands_to_send
    }

    pub(crate) fn apply_log_store_result(&mut self, result: LogStoreResult) {
        super::super::log_state::apply_store_result(&mut self.log_windows, result);
    }

    pub(crate) fn resource_watch_failed(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        error: String,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        cluster
            .active_watchers
            .remove(&(api_resource.clone(), namespace.clone()));
        if api_resource.is_helm_releases()
            && let Some(namespace) = namespace
        {
            let watch = cluster.helm_release_cache.entry(namespace).or_default();
            watch.is_synced = true;
            watch.backend_errors.insert("Helm storage", error);
            return;
        }
        for key in cluster.watch_keys_for(&api_resource, namespace) {
            let watch = cluster.resource_cache.entry(key).or_default();
            watch.is_synced = false;
            watch.error = Some(error.clone());
        }
    }

    /// A running watcher has been retrying past the grace period (`Some`) or has
    /// recovered (`None`). Rows, `is_synced` and the active watcher are left
    /// alone. A late message for a watcher that is no longer active is a no-op,
    /// so it can neither create cache entries nor flag a replaced watcher.
    pub(crate) fn set_resource_watch_error(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        namespace: Option<String>,
        error: Option<String>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        if !cluster
            .active_watchers
            .contains(&(api_resource.clone(), namespace.clone()))
        {
            return;
        }
        for key in cluster.watch_keys_for(&api_resource, namespace) {
            match &error {
                // A watcher still listing for the first time has no entry yet.
                Some(_) => cluster
                    .resource_cache
                    .entry(key)
                    .or_default()
                    .error
                    .clone_from(&error),
                None => {
                    if let Some(watch) = cluster.resource_cache.get_mut(&key) {
                        watch.error = None;
                    }
                }
            }
        }
    }
}

impl ClusterState {
    /// The cache keys a watcher's result applies to. A namespaced resource
    /// watched across namespaces reports without a namespace and covers every
    /// selected one.
    fn watch_keys_for(
        &self,
        api_resource: &ApiResource,
        namespace: Option<String>,
    ) -> Vec<ResourceWatchKey> {
        if api_resource.namespaced && namespace.is_none() {
            self.selected_namespaces
                .iter()
                .map(|namespace| (api_resource.clone(), Some(namespace.clone())))
                .collect()
        } else {
            vec![(api_resource.clone(), namespace)]
        }
    }
}
