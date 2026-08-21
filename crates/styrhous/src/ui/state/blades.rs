use super::*;

#[derive(Debug)]
pub(crate) struct ResourceDetailPanelState {
    /// Avoid treating the row click which opened the overlay as a scrim dismissal.
    pub(crate) dismiss_on_outside_click: bool,
}

#[derive(Debug)]
pub(crate) struct ResourceDetailHistoryEntry {
    /// Distinguishes repeated visits to the same Kubernetes resource.
    pub(crate) history_entry_id: u64,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
    pub(crate) resource_name: String,
    pub(crate) resource_uid: String,
    pub(crate) detail: Option<ResourceDetail>,
    pub(crate) events: Vec<ResourceEvent>,
    pub(crate) detail_error: Option<String>,
    pub(crate) events_error: Option<String>,
    pub(crate) managed_resources: Vec<ManagedResource>,
    pub(crate) managed_resources_error: Option<String>,
    /// Latest sample and short rolling history are intentionally local to this inspector visit.
    pub(crate) pod_usage: Option<PodUsage>,
    pub(crate) pod_usage_history: Vec<PodUsage>,
    /// A successful Metrics API response confirmed that this Pod has no sample yet.
    pub(crate) pod_usage_missing: bool,
    /// The cluster does not serve PodMetrics, so resource usage cannot be collected.
    pub(crate) pod_metrics_api_unavailable: bool,
    pub(crate) pod_usage_error: Option<String>,
    pub(crate) node_usage: Option<NodeUsage>,
    pub(crate) node_usage_history: Vec<NodeUsage>,
    pub(crate) node_metrics_api_unavailable: bool,
    pub(crate) node_usage_error: Option<String>,
    pub(crate) data_editor: Option<ResourceDataEditorState>,
    /// UI interactions are recorded while rendering, then consumed by the
    /// global blade coordinator after the navigator borrow ends.
    pub(crate) pending_action: Option<ResourceAction>,
}

impl ResourceDetailHistoryEntry {
    pub(crate) fn record_pod_usage(&mut self, usage: PodUsage) {
        self.pod_usage = Some(usage.clone());
        self.pod_usage_missing = false;
        if let Some(existing) = self
            .pod_usage_history
            .iter_mut()
            .find(|sample| sample.timestamp == usage.timestamp)
        {
            *existing = usage;
        } else {
            self.pod_usage_history.push(usage);
            self.pod_usage_history
                .sort_by_key(|sample| sample.timestamp);
        }
        self.prune_pod_usage_history(time::OffsetDateTime::now_utc());
        self.pod_usage_error = None;
    }

    pub(crate) fn prune_pod_usage_history(&mut self, now: time::OffsetDateTime) {
        let oldest = now - POD_USAGE_HISTORY_WINDOW;
        self.pod_usage_history
            .retain(|sample| sample.timestamp >= oldest);
    }

    pub(crate) fn record_node_usage(&mut self, usage: NodeUsage) {
        self.node_usage = Some(usage.clone());
        if let Some(existing) = self
            .node_usage_history
            .iter_mut()
            .find(|sample| sample.timestamp == usage.timestamp)
        {
            *existing = usage;
        } else {
            self.node_usage_history.push(usage);
            self.node_usage_history
                .sort_by_key(|sample| sample.timestamp);
        }
        self.prune_node_usage_history(time::OffsetDateTime::now_utc());
        self.node_usage_error = None;
    }

    pub(crate) fn prune_node_usage_history(&mut self, now: time::OffsetDateTime) {
        let oldest = now - POD_USAGE_HISTORY_WINDOW;
        self.node_usage_history
            .retain(|sample| sample.timestamp >= oldest);
    }
}

impl UiState {
    /// Replace the sole global blade root and perform the lifecycle cleanup
    /// that every root replacement requires. Feature modules must use this
    /// instead of manipulating the coordinator directly.
    pub(in crate::ui) fn replace_global_blade(
        &mut self,
        content: Box<dyn GlobalBladeContent>,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let discarded = self.global_blades.open(content);
        Self::stop_discarded_blades(discarded, commands_to_send);
        for cluster in self.clusters.values_mut() {
            cluster.resource_detail_panel = None;
        }
    }

    #[cfg(test)]
    pub(crate) fn terminal_settings_blade(
        &self,
    ) -> Option<&super::super::settings::TerminalSettingsBlade> {
        self.global_blades
            .navigator()?
            .current()
            .terminal_settings()
    }
    pub(crate) fn resource_detail_entry_mut(
        &mut self,
        history_entry_id: u64,
    ) -> Option<&mut ResourceDetailHistoryEntry> {
        self.global_blades
            .navigator_mut()?
            .entries_mut()
            .filter_map(|entry| entry.resource_detail_mut())
            .find(|entry| entry.history_entry_id == history_entry_id)
    }

    pub(crate) fn mark_pod_metrics_api_unavailable(&mut self, cluster_key: i32) {
        let Some(navigator) = self.global_blades.navigator_mut() else {
            return;
        };
        for entry in navigator
            .entries_mut()
            .filter_map(|entry| entry.resource_detail_mut())
            .filter(|entry| entry.cluster_key == cluster_key)
        {
            entry.pod_metrics_api_unavailable = true;
        }
    }

    pub(crate) fn mark_node_metrics_api_unavailable(&mut self, cluster_key: i32) {
        let Some(navigator) = self.global_blades.navigator_mut() else {
            return;
        };
        for entry in navigator
            .entries_mut()
            .filter_map(|entry| entry.resource_detail_mut())
            .filter(|entry| {
                entry.cluster_key == cluster_key
                    && entry.api_resource.group == "core"
                    && entry.api_resource.kind == "Node"
            })
        {
            entry.node_metrics_api_unavailable = true;
        }
    }

    pub(in crate::ui) fn stop_discarded_blades(
        discarded: impl IntoIterator<Item = Box<dyn GlobalBladeContent>>,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let mut entries_by_cluster = HashMap::<i32, Vec<u64>>::new();
        for content in discarded {
            if let Some(entry) = content.resource_detail() {
                entries_by_cluster
                    .entry(entry.cluster_key)
                    .or_default()
                    .push(entry.history_entry_id);
            }
        }
        for (cluster_key, history_entry_ids) in entries_by_cluster {
            stop_resource_detail_watches(cluster_key, history_entry_ids, commands_to_send);
        }
    }
}
