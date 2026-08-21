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
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        let Some(progress) = cluster.resources.bulk_delete_progress.as_mut() else {
            return;
        };
        if bulk_delete_id != Some(progress.id) {
            return;
        }
        let Some(target) = progress.target_for(api_resource, resource_name, namespace) else {
            return;
        };

        let api_resource = progress.api_resource.clone();
        progress.remaining_targets.remove(&target);
        if let Some(failure) = failure {
            progress.failures.push((target, failure));
        } else if let Some(selection) = cluster.resources.resource_selections.get_mut(&api_resource)
        {
            selection.remove(&target.uid);
        }

        if !progress.remaining_targets.is_empty() {
            return;
        }

        let failures = std::mem::take(&mut progress.failures);
        cluster.resources.bulk_delete_progress = None;
        if failures.is_empty() {
            return;
        }
        let details = failures
            .iter()
            .map(|(target, error)| format!("{}: {error}", target.display_name()))
            .collect::<Vec<_>>()
            .join("\n");
        cluster.bulk_delete_error = Some(details);
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
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            let key = (api_resource.clone(), namespace.clone());
            cluster.active_watchers.remove(&key);
            if api_resource.is_helm_releases()
                && let Some(namespace) = namespace
            {
                let watch = cluster.helm_release_cache.entry(namespace).or_default();
                watch.is_synced = true;
                watch.backend_errors.insert("Helm storage", error);
                return;
            }
            if api_resource.namespaced && namespace.is_none() {
                for namespace in cluster.selected_namespaces.clone() {
                    let watch = cluster
                        .resource_cache
                        .entry((api_resource.clone(), Some(namespace)))
                        .or_default();
                    watch.is_synced = false;
                    watch.error = Some(error.clone());
                }
                return;
            }
            let watch = cluster.resource_cache.entry(key).or_default();
            watch.is_synced = false;
            watch.error = Some(error);
        }
    }
}
