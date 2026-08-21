use super::*;

impl WorkerResult for crate::worker::PodMetricsUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            if !cluster.active_pod_metrics.contains(&self.namespace) {
                return;
            }
            let metrics = cluster.pod_metrics.entry(self.namespace).or_default();
            metrics.usages = self.usages;
            metrics.error = None;
        }
    }
}
impl WorkerResult for crate::worker::PodMetricsWatchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            if !cluster.active_pod_metrics.contains(&self.namespace) {
                return;
            }
            cluster.pod_metrics.entry(self.namespace).or_default().error = Some(self.error);
        }
    }
}
impl WorkerResult for crate::worker::PodMetricsApiUnavailable {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let namespaces = {
            let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) else {
                return;
            };
            if !cluster.pod_metrics_api_available {
                return;
            }
            cluster.pod_metrics_api_available = false;
            cluster.pod_metrics.clear();
            std::mem::take(&mut cluster.active_pod_metrics)
        };
        ui.mark_pod_metrics_api_unavailable(self.cluster_key);
        for namespace in namespaces {
            commands.push(Box::new(crate::worker::StopPodMetricsWatch {
                cluster_key: self.cluster_key,
                namespace,
            }));
        }
    }
}
impl WorkerResult for crate::worker::NodeMetricsUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key)
            && cluster.node_metrics_active
        {
            cluster.node_metrics.usages = self.usages;
            cluster.node_metrics.error = None;
        }
    }
}
impl WorkerResult for crate::worker::NodeMetricsWatchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key)
            && cluster.node_metrics_active
        {
            cluster.node_metrics.error = Some(self.error);
        }
    }
}
impl WorkerResult for crate::worker::NodeMetricsApiUnavailable {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let was_active = {
            let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) else {
                return;
            };
            if !cluster.node_metrics_api_available {
                return;
            }
            cluster.node_metrics_api_available = false;
            cluster.node_metrics = NodeMetricsState::default();
            std::mem::replace(&mut cluster.node_metrics_active, false)
        };
        ui.mark_node_metrics_api_unavailable(self.cluster_key);
        if was_active {
            commands.push(Box::new(crate::worker::StopNodeMetricsWatch {
                cluster_key: self.cluster_key,
            }));
        }
    }
}
