use super::*;

impl UiState {
    pub(crate) fn open_resource_detail(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        name: String,
        namespace: Option<String>,
        uid: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let (selection_generation, pod_metrics_api_available, node_metrics_api_available) = {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };
            cluster.next_detail_generation += 1;
            (
                cluster.next_detail_generation,
                cluster.pod_metrics_api_available,
                cluster.node_metrics_api_available,
            )
        };
        self.replace_global_blade(
            Box::new(ResourceDetailHistoryEntry {
                history_entry_id: selection_generation,
                cluster_key,
                api_resource: api_resource.clone(),
                namespace: namespace.clone(),
                resource_name: name.clone(),
                resource_uid: uid.clone(),
                detail: None,
                events: Vec::new(),
                detail_error: None,
                events_error: None,
                managed_resources: Vec::new(),
                managed_resources_error: None,
                pod_usage: None,
                pod_usage_history: Vec::new(),
                pod_usage_missing: false,
                pod_metrics_api_unavailable: !pod_metrics_api_available,
                pod_usage_error: None,
                node_usage: None,
                node_usage_history: Vec::new(),
                node_metrics_api_unavailable: !node_metrics_api_available,
                node_usage_error: None,
                data_editor: None,
                pending_action: None,
            }),
            commands_to_send,
        );
        let cluster = self
            .clusters
            .get_mut(&cluster_key)
            .expect("cluster was checked before opening its blade");
        cluster.resource_detail_panel = Some(ResourceDetailPanelState {
            dismiss_on_outside_click: false,
        });
        commands_to_send.push(Box::new(crate::worker::StartResourceDetailWatch {
            cluster_key: cluster.cluster_key,
            history_entry_id: selection_generation,
            api_resource,
            namespace,
            resource_name: name,
            resource_uid: uid,
            pod_metrics_api_available,
            node_metrics_api_available,
        }));
    }

    pub(crate) fn helm_releases(
        &self,
        cluster_key: i32,
        namespace: &str,
        release_name: &str,
    ) -> Vec<HelmRelease> {
        self.clusters
            .get(&cluster_key)
            .and_then(|cluster| cluster.helm_release_cache.get(namespace))
            .map(|watch| {
                watch
                    .releases
                    .iter()
                    .filter(|release| release.name == release_name)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn open_helm_release_detail(
        &mut self,
        cluster_key: i32,
        release_name: String,
        namespace: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let releases = self.helm_releases(cluster_key, &namespace, &release_name);
        if releases.is_empty() {
            return;
        }
        self.replace_global_blade(
            Box::new(super::super::helm_releases::HelmReleaseDetailBlade::new(
                cluster_key,
                release_name,
                namespace,
            )),
            commands_to_send,
        );
    }

    #[cfg(test)]
    pub(crate) fn navigate_resource_detail(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        name: String,
        namespace: Option<String>,
        uid: String,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        cluster.next_detail_generation += 1;
        let selection_generation = cluster.next_detail_generation;
        let pod_metrics_api_available = cluster.pod_metrics_api_available;
        let node_metrics_api_available = cluster.node_metrics_api_available;
        if cluster.resource_detail_panel.is_none() {
            return;
        }
        let entry = ResourceDetailHistoryEntry {
            history_entry_id: selection_generation,
            cluster_key,
            api_resource: api_resource.clone(),
            namespace: namespace.clone(),
            resource_name: name.clone(),
            resource_uid: uid.clone(),
            detail: None,
            events: Vec::new(),
            detail_error: None,
            events_error: None,
            managed_resources: Vec::new(),
            managed_resources_error: None,
            pod_usage: None,
            pod_usage_history: Vec::new(),
            pod_usage_missing: false,
            pod_metrics_api_unavailable: !pod_metrics_api_available,
            pod_usage_error: None,
            node_usage: None,
            node_usage_history: Vec::new(),
            node_metrics_api_unavailable: !node_metrics_api_available,
            node_usage_error: None,
            data_editor: None,
            pending_action: None,
        };
        let discarded = self.global_blades.push(Box::new(entry));
        Self::stop_discarded_blades(discarded, commands_to_send);
        commands_to_send.push(Box::new(crate::worker::StartResourceDetailWatch {
            cluster_key: cluster.cluster_key,
            history_entry_id: selection_generation,
            api_resource,
            namespace,
            resource_name: name,
            resource_uid: uid,
            pod_metrics_api_available,
            node_metrics_api_available,
        }));
    }

    #[cfg(test)]
    pub(crate) fn navigate_resource_detail_history(
        &mut self,
        cluster_key: i32,
        forward: bool,
        _commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        let Some(_panel) = cluster.resource_detail_panel.as_mut() else {
            return;
        };
        if forward {
            let _ = self
                .global_blades
                .navigator_mut()
                .is_some_and(BladeNavigator::go_forward);
        } else {
            let _ = self
                .global_blades
                .navigator_mut()
                .is_some_and(BladeNavigator::go_back);
        }
    }

    pub(crate) fn close_all_resource_details(
        &mut self,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        for cluster in self.clusters.values_mut() {
            cluster.resource_detail_panel = None;
        }
        if self.global_blades.navigator().is_some_and(|navigator| {
            navigator
                .entries()
                .any(|entry| entry.resource_detail().is_some())
        }) {
            Self::stop_discarded_blades(self.global_blades.clear(), commands_to_send);
        }
    }
}
