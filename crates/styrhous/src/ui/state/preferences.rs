use super::*;

impl UiState {
    pub(crate) fn resource_navigation_node_is_expanded(&self, node_id: &str) -> bool {
        self.resource_navigation_expansion
            .expanded_nodes
            .contains(node_id)
    }

    pub(crate) fn set_resource_navigation_node_expanded(
        &mut self,
        node_id: impl Into<String>,
        is_expanded: bool,
    ) {
        let node_id = node_id.into();
        if is_expanded {
            self.resource_navigation_expansion
                .expanded_nodes
                .insert(node_id);
        } else {
            self.resource_navigation_expansion
                .expanded_nodes
                .remove(&node_id);
        }
    }

    pub(crate) fn remember_selected_namespaces(&mut self, cluster_key: i32) {
        let Some(cluster) = self.clusters.get(&cluster_key) else {
            return;
        };
        let context_name = cluster.name.clone();
        let namespaces = cluster.selected_namespaces.iter().cloned().collect();
        self.cluster_selections
            .selections
            .entry(context_name)
            .or_default()
            .selected_namespaces = namespaces;
        self.prune_empty_cluster_selection(cluster_key);
    }

    pub(crate) fn remember_selected_api_resource(
        &mut self,
        cluster_key: i32,
        api_resource: &ApiResource,
    ) {
        let Some(context_name) = self
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.name.clone())
        else {
            return;
        };
        self.cluster_selections
            .selections
            .entry(context_name)
            .or_default()
            .selected_api_resource = Some(PersistedApiResource::from_api_resource(api_resource));
    }

    pub(crate) fn prune_empty_cluster_selection(&mut self, cluster_key: i32) {
        let Some(context_name) = self
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.name.clone())
        else {
            return;
        };
        if self
            .cluster_selections
            .selections
            .get(&context_name)
            .is_some_and(|selection| selection == &PersistedClusterSelection::default())
        {
            self.cluster_selections.selections.remove(&context_name);
        }
    }

    pub(crate) fn restore_selected_namespaces(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(cluster) = self.clusters.get(&cluster_key) else {
            return;
        };
        let context_name = cluster.name.clone();
        let available_namespaces = cluster
            .namespaces
            .values()
            .map(|namespace| namespace.name.clone())
            .collect::<BTreeSet<_>>();

        let restored_namespaces =
            if let Some(selection) = self.cluster_selections.selections.get_mut(&context_name) {
                selection
                    .selected_namespaces
                    .retain(|namespace| available_namespaces.contains(namespace));
                selection.selected_namespaces.clone()
            } else {
                BTreeSet::new()
            };

        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            cluster.selected_namespaces = restored_namespaces.into_iter().collect();
            Self::reconcile_after_namespace_change(cluster, commands_to_send);
        }
        self.prune_empty_cluster_selection(cluster_key);
    }

    pub(crate) fn restored_api_resource(
        &mut self,
        cluster_key: i32,
        api_resources: &[ApiResource],
    ) -> Option<ApiResource> {
        let context_name = self
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.name.clone())?;
        let saved_resource = self
            .cluster_selections
            .selections
            .get(&context_name)
            .and_then(|selection| selection.selected_api_resource.as_ref());
        let api_resource = saved_resource.and_then(|saved_resource| {
            if saved_resource.matches(&ApiResource::helm_releases()) {
                Some(ApiResource::helm_releases())
            } else {
                api_resources
                    .iter()
                    .find(|api_resource| saved_resource.matches(api_resource))
                    .cloned()
            }
        });

        if saved_resource.is_some() && api_resource.is_none() {
            if let Some(selection) = self.cluster_selections.selections.get_mut(&context_name) {
                selection.selected_api_resource = None;
            }
            self.prune_empty_cluster_selection(cluster_key);
            return None;
        }

        api_resource
    }
}
