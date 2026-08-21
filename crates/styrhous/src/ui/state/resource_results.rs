use super::*;

impl WorkerResult for crate::worker::KubernetesResourceAdded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceAdded {
            cluster_key,
            api_resource,
            namespace,
            resource,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster
                .resource_cache
                .entry((api_resource, namespace))
                .or_default()
                .resources
                .insert(resource.uid.clone(), resource);
        }
    }
}
impl WorkerResult for crate::worker::KubernetesResourceDeleted {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceDeleted {
            cluster_key,
            api_resource,
            namespace,
            resource_uid,
        } = self;
        let deleted_history_entry_id = ui.global_blades.navigator().and_then(|navigator| {
            navigator
                .entries()
                .filter_map(|content| content.resource_detail())
                .find(|entry| {
                    entry.cluster_key == cluster_key && entry.resource_uid == resource_uid
                })
                .map(|entry| entry.history_entry_id)
        });
        let closes_active_blade = deleted_history_entry_id.is_some_and(|history_entry_id| {
            ui.global_blades.navigator().is_some_and(|navigator| {
                navigator
                    .current()
                    .resource_detail()
                    .is_some_and(|entry| entry.history_entry_id == history_entry_id)
                    || navigator
                        .current()
                        .is_owned_by_resource_detail(history_entry_id)
            })
        });
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            if let Some(watch) = cluster
                .resource_cache
                .get_mut(&(api_resource.clone(), namespace.clone()))
            {
                watch.resources.remove(&resource_uid);
            }
            if closes_active_blade {
                cluster.resource_detail_panel = None;
            }
        }
        if closes_active_blade {
            UiState::stop_discarded_blades(ui.global_blades.clear(), commands);
        }
    }
}
impl WorkerResult for crate::worker::KubernetesResourcesReplaced {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourcesReplaced {
            cluster_key,
            api_resource,
            namespace,
            resources,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            let watch = cluster
                .resource_cache
                .entry((api_resource.clone(), namespace))
                .or_default();
            watch.resources = resources
                .into_iter()
                .map(|resource| (resource.uid.clone(), resource))
                .collect();
            watch.is_synced = true;
            watch.error = None;
            let visible_uids = cluster
                .resource_cache
                .iter()
                .filter(|((cached_resource, cached_namespace), _)| {
                    cached_resource == &api_resource
                        && (!api_resource.namespaced
                            || cached_namespace.as_ref().is_some_and(|namespace| {
                                cluster.selected_namespaces.contains(namespace)
                            }))
                })
                .flat_map(|(_, watch)| watch.resources.keys().cloned())
                .collect::<HashSet<_>>();
            if let Some(selection) = cluster.resource_selections.get_mut(&api_resource) {
                selection.retain(|uid| visible_uids.contains(uid));
            }
        }
    }
}
impl WorkerResult for crate::worker::HelmReleasesReplaced {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            let watch = cluster
                .helm_release_cache
                .entry(self.namespace)
                .or_default();
            watch.releases = self.releases;
            watch.is_synced = true;
        }
    }
}
impl WorkerResult for crate::worker::HelmReleaseBackendFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            let watch = cluster
                .helm_release_cache
                .entry(self.namespace)
                .or_default();
            watch.is_synced = true;
            watch.backend_errors.insert(self.backend, self.error);
        }
    }
}
impl WorkerResult for crate::worker::KubernetesResourceWatchStarted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceWatchStarted {
            cluster_key,
            api_resource,
            namespace,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            let helm_namespace = api_resource
                .is_helm_releases()
                .then_some(namespace.clone())
                .flatten();
            cluster.active_watchers.insert((api_resource, namespace));
            if let Some(namespace) = helm_namespace {
                cluster.helm_release_cache.entry(namespace).or_default();
            }
        }
    }
}
impl WorkerResult for crate::worker::KubernetesResourceWatchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceWatchFailed {
            cluster_key,
            api_resource,
            namespace,
            error,
        } = self;
        ui.resource_watch_failed(cluster_key, api_resource, namespace, error);
    }
}
