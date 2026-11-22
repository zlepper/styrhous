use super::*;
use crate::ui::log_windows::LogExportNotification;
use crate::ui::save_file_dialog::SaveFileAnswer;

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
        let orphaned = match &result {
            LogStoreResult::Exported {
                window_id,
                export_id,
                ..
            }
            | LogStoreResult::ExportFailed {
                window_id,
                export_id,
                ..
            } => self
                .orphaned_log_exports
                .remove(&(*window_id, *export_id))
                .and_then(|cluster_key| LogExportNotification::for_result(cluster_key, &result)),
            _ => None,
        };
        let export = orphaned
            .or_else(|| super::super::log_state::apply_store_result(&mut self.log_windows, result));
        if let Some(export) = export {
            self.record_log_export(export);
        }
    }

    /// Hands a save dialog's answer to the window that asked.
    pub(crate) fn answer_log_export_dialog(
        &mut self,
        answer: SaveFileAnswer,
        log_store: &crate::log_store::LogStoreService,
    ) {
        let failure = self
            .log_windows
            .get_mut(&answer.window_id)
            .and_then(|window| window.answer_save_request(answer, log_store));
        if let Some(failure) = failure {
            self.record_log_export(failure);
        }
    }

    fn record_log_export(&mut self, export: LogExportNotification) {
        // Log windows outlive the cluster session, so record the outcome even
        // while the cluster is reconnecting.
        self.push_operation_outcome(
            export.cluster_key,
            export.outcome,
            export.title,
            None,
            export.target,
            Some(export.details),
        );
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
    /// Whether `api_resource` is watched across every namespace, including ones that are
    /// not known yet.
    pub(crate) fn watches_every_namespace(&self, api_resource: &ApiResource) -> bool {
        api_resource.namespaced
            && self.all_namespaces_watch_scopes.get(api_resource)
                == Some(&crate::worker::WatchedNamespaces::Every)
    }

    /// Whether rows of `api_resource` in `namespace` are shown.
    pub(crate) fn shows_namespace(&self, api_resource: &ApiResource, namespace: &str) -> bool {
        self.watches_every_namespace(api_resource) || self.namespace_scope().contains(namespace)
    }

    /// The cache keys a watcher's result applies to. A namespaced resource
    /// watched across namespaces reports without a namespace and covers every
    /// selected one, as well as the watch's own entry.
    fn watch_keys_for(
        &self,
        api_resource: &ApiResource,
        namespace: Option<String>,
    ) -> Vec<ResourceWatchKey> {
        if api_resource.namespaced && namespace.is_none() {
            self.namespace_scope()
                .iter()
                .map(|namespace| (api_resource.clone(), Some(namespace.clone())))
                .chain(std::iter::once((api_resource.clone(), None)))
                .collect()
        } else {
            vec![(api_resource.clone(), namespace)]
        }
    }
}
