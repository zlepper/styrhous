use super::*;

impl UiState {
    pub(crate) fn select_cluster(&mut self, cluster_key: i32) -> Option<WorkerCommandBox> {
        self.select_cluster_inner(cluster_key, true)
    }

    pub(crate) fn select_cluster_without_remembering(
        &mut self,
        cluster_key: i32,
    ) -> Option<WorkerCommandBox> {
        self.select_cluster_inner(cluster_key, false)
    }

    pub(crate) fn select_cluster_inner(
        &mut self,
        cluster_key: i32,
        remember_selection: bool,
    ) -> Option<WorkerCommandBox> {
        self.selected_cluster = Some(cluster_key);

        let context_name = self.clusters.get(&cluster_key)?.name.clone();
        if remember_selection {
            self.cluster_selections.last_selected_context = Some(context_name);
        }
        if self
            .clusters
            .get(&cluster_key)
            .is_some_and(|cluster| cluster.resource_detail_panel.is_some())
        {
            // `ConnectToCluster` tears down every worker watch for this session.
            // Drop the matching UI history now so a cancelled inspector cannot
            // remain above the reconnected workspace.
            let _ = self.global_blades.clear();
        }
        let cluster = self.clusters.get_mut(&cluster_key)?;
        if matches!(
            &cluster.connection,
            ClusterConnectionState::Connected | ClusterConnectionState::Connecting
        ) {
            return None;
        }

        cluster.reset_for_connection();

        Some(Box::new(crate::worker::ConnectToCluster {
            cluster: cluster.name.clone(),
            cluster_key,
        }))
    }

    pub(crate) fn select_api_resource(
        &mut self,
        cluster_key: i32,
        api_resource: ApiResource,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let (api_resource, closed_resource_detail) = {
            let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
                return;
            };

            let closed_resource_detail = cluster.resource_detail_panel.take().is_some();
            cluster.selected_api_resource = Some(api_resource);
            (
                cluster
                    .selected_api_resource
                    .clone()
                    .expect("selected API resource was just set"),
                closed_resource_detail,
            )
        };
        if closed_resource_detail {
            Self::stop_discarded_blades(self.global_blades.clear(), commands_to_send);
        }
        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            Self::request_selected_resource_watches(cluster, &api_resource, commands_to_send);
        }
        self.remember_selected_api_resource(cluster_key, &api_resource);
    }
}
