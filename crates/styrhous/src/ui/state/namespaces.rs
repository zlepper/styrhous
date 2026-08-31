use super::*;

impl UiState {
    pub(crate) fn toggle_namespace(
        &mut self,
        cluster_key: i32,
        namespace: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let api_resource = {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };
            let was_selected = !cluster.selected_namespaces.insert(namespace.clone());
            if was_selected {
                cluster.selected_namespaces.remove(&namespace);
            }
            cluster.selected_api_resource.clone()
        };
        self.remember_selected_namespaces(cluster_key);
        let Some(api_resource) = api_resource else {
            return;
        };
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
        }
    }

    /// Replace the visible namespace scope and reconcile its watch sources.
    pub(crate) fn replace_selected_namespaces<I>(
        &mut self,
        cluster_key: i32,
        namespaces: I,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) where
        I: IntoIterator<Item = String>,
    {
        {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };
            cluster.selected_namespaces = namespaces.into_iter().collect();
        }
        self.remember_selected_namespaces(cluster_key);
        let Some(api_resource) = self
            .clusters
            .get(&cluster_key)
            .and_then(|cluster| cluster.selected_api_resource.clone())
        else {
            return;
        };
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
        }
    }

    /// Select every discovered namespace and replace per-namespace sources with
    /// one all-namespaces source when the resource supports it.
    pub(crate) fn select_all_namespaces(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };
            cluster.selected_namespaces = cluster
                .namespaces
                .values()
                .map(|namespace| namespace.name.clone())
                .collect();
        }
        self.remember_selected_namespaces(cluster_key);
        let Some(api_resource) = self
            .clusters
            .get(&cluster_key)
            .and_then(|cluster| cluster.selected_api_resource.clone())
        else {
            return;
        };
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
        }
    }

    /// Clear the visible namespace scope and stop its resource sources.
    pub(crate) fn clear_selected_namespaces(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let api_resource = self
            .clusters
            .get(&cluster_key)
            .and_then(|cluster| cluster.selected_api_resource.clone());
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            cluster.selected_namespaces.clear();
            if let Some(api_resource) = api_resource {
                Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
            } else {
                Self::reconcile_pod_metrics(cluster, commands_to_send);
                Self::reconcile_node_metrics(cluster, commands_to_send);
            }
        }
        self.remember_selected_namespaces(cluster_key);
    }

    pub(crate) fn retry_selected_load(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let retry_connection = self.clusters.get(&cluster_key).is_some_and(|cluster| {
            matches!(&cluster.connection, ClusterConnectionState::Failed(_))
                || matches!(&cluster.namespaces_load, ClusterLoadState::Failed(_))
                || matches!(&cluster.api_resources_load, ClusterLoadState::Failed(_))
        });
        if retry_connection {
            if let Some(command) = self.select_cluster_without_remembering(cluster_key) {
                commands_to_send.push(command);
            }
            return;
        }

        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        let Some(api_resource) = cluster.selected_api_resource.clone() else {
            return;
        };
        Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
    }

    pub(crate) fn request_selected_resource_watches(
        cluster: &mut ClusterState,
        api_resource: &ApiResource,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let discovered_namespaces = cluster
            .namespaces
            .values()
            .map(|namespace| namespace.name.clone())
            .collect::<HashSet<_>>();
        let all_namespaces = api_resource.namespaced
            && !api_resource.is_helm_releases()
            && !discovered_namespaces.is_empty()
            && cluster.selected_namespaces == discovered_namespaces;
        let desired = if all_namespaces || !api_resource.namespaced {
            HashSet::from([None])
        } else {
            cluster
                .selected_namespaces
                .iter()
                .cloned()
                .map(Some)
                .collect()
        };
        let active = cluster
            .active_watchers
            .iter()
            .filter(|(resource, _)| resource == api_resource)
            .map(|(_, namespace)| namespace.clone())
            .collect::<HashSet<_>>();

        if active != desired {
            let had_active_sources = !active.is_empty();
            cluster
                .active_watchers
                .retain(|(resource, _)| resource != api_resource);
            if had_active_sources {
                cluster
                    .resource_cache
                    .retain(|(resource, _), _| resource != api_resource);
                cluster.resource_table_cache.clear();
            }
            let sources = if !api_resource.namespaced {
                (!cluster
                    .resource_cache
                    .get(&(api_resource.clone(), None))
                    .is_some_and(|watch| watch.is_synced))
                .then_some(crate::worker::ResourceWatchSource::Cluster)
                .into_iter()
                .collect()
            } else if all_namespaces {
                vec![crate::worker::ResourceWatchSource::AllNamespaces(
                    discovered_namespaces.into_iter().collect(),
                )]
            } else {
                desired
                    .iter()
                    .filter_map(|namespace| namespace.clone())
                    .filter(|namespace| {
                        !cluster
                            .resource_cache
                            .get(&(api_resource.clone(), Some(namespace.clone())))
                            .is_some_and(|watch| watch.is_synced)
                    })
                    .map(crate::worker::ResourceWatchSource::Namespace)
                    .collect()
            };
            cluster.active_watchers.extend(sources.iter().map(|source| {
                let namespace = match source {
                    crate::worker::ResourceWatchSource::Namespace(namespace) => {
                        Some(namespace.clone())
                    }
                    crate::worker::ResourceWatchSource::AllNamespaces(_)
                    | crate::worker::ResourceWatchSource::Cluster => None,
                };
                (api_resource.clone(), namespace)
            }));
            if had_active_sources || !sources.is_empty() {
                commands_to_send.push(Box::new(crate::worker::ReconcileResourceWatches {
                    cluster_key: cluster.cluster_key,
                    api_resource: api_resource.clone(),
                    sources,
                }));
            }
        }
        Self::reconcile_pod_metrics(cluster, commands_to_send);
        Self::reconcile_node_metrics(cluster, commands_to_send);
    }

    pub(crate) fn reconcile_after_namespace_change(
        cluster: &mut ClusterState,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(api_resource) = cluster.selected_api_resource.clone() else {
            return;
        };
        // An all-namespaces watcher filters using the discovered namespace set
        // captured when it started. Refresh it whenever that set changes.
        if api_resource.namespaced && !api_resource.is_helm_releases() {
            cluster
                .active_watchers
                .remove(&(api_resource.clone(), None));
        }
        Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
    }

    pub(crate) fn reconcile_pod_metrics(
        cluster: &mut ClusterState,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let desired = cluster
            .selected_api_resource
            .as_ref()
            .filter(|resource| resource.group == "core" && resource.kind == "Pod")
            .filter(|_| cluster.pod_metrics_api_available)
            .map(|_| cluster.selected_namespaces.clone())
            .unwrap_or_default();
        let inactive = cluster
            .active_pod_metrics
            .difference(&desired)
            .cloned()
            .collect::<Vec<_>>();
        for namespace in inactive {
            cluster.active_pod_metrics.remove(&namespace);
            commands_to_send.push(Box::new(crate::worker::StopPodMetricsWatch {
                cluster_key: cluster.cluster_key,
                namespace,
            }));
        }
        for namespace in desired {
            if cluster.active_pod_metrics.insert(namespace.clone()) {
                commands_to_send.push(Box::new(crate::worker::StartPodMetricsWatch {
                    cluster_key: cluster.cluster_key,
                    namespace,
                }));
            }
        }
    }

    pub(crate) fn reconcile_node_metrics(
        cluster: &mut ClusterState,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let desired = cluster
            .selected_api_resource
            .as_ref()
            .is_some_and(|resource| resource.group == "core" && resource.kind == "Node")
            && cluster.node_metrics_api_available;
        if desired == cluster.node_metrics_active {
            return;
        }
        cluster.node_metrics_active = desired;
        if desired {
            commands_to_send.push(Box::new(crate::worker::StartNodeMetricsWatch {
                cluster_key: cluster.cluster_key,
            }));
        } else {
            commands_to_send.push(Box::new(crate::worker::StopNodeMetricsWatch {
                cluster_key: cluster.cluster_key,
            }));
        }
    }
}
