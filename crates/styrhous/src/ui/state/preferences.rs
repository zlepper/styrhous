use super::*;

impl UiState {
    #[cfg(test)]
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
        let namespaces = cluster.selected_namespaces.clone();
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
        let restored_namespaces =
            if let Some(selection) = self.cluster_selections.selections.get_mut(&cluster.name) {
                selection
                    .selected_namespaces
                    .retain_discovered(&cluster.namespaces);
                selection.selected_namespaces.clone()
            } else {
                SelectedNamespaces::default()
            };

        if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
            cluster.selected_namespaces = restored_namespaces;
            Self::reconcile_after_namespace_change(cluster, commands_to_send);
        }
        self.prune_empty_cluster_selection(cluster_key);
    }

    /// The persisted namespace selection of the cluster's context, without checking it
    /// against the namespaces, so watches can start before the namespace list arrives.
    pub(crate) fn apply_persisted_namespaces(&mut self, cluster_key: i32) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        if let Some(selection) = self.cluster_selections.selections.get(&cluster.name) {
            cluster.selected_namespaces = selection.selected_namespaces.clone();
        }
    }

    /// Keeps the selected resource in line with what discovery has shown so far.
    ///
    /// Without a selection, the context's saved resource is selected once it is known. A
    /// selected resource that discovery has superseded by another version follows it. Once
    /// discovery is complete, a saved or selected resource that does not exist is
    /// forgotten; one in a group whose discovery failed may still exist and is kept.
    pub(crate) fn reconcile_selection_with_discovery(
        &mut self,
        cluster_key: i32,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(cluster) = self.clusters.get(&cluster_key) else {
            return;
        };
        let find = |identity: (&str, &str)| {
            cluster
                .displayed_api_resources
                .api_resources
                .iter()
                .find(|resource| resource.identity() == identity)
                .cloned()
        };
        match cluster.selected_api_resource.clone() {
            Some(selected)
                if selected.is_helm_releases()
                    || cluster
                        .displayed_api_resources
                        .api_resources
                        .contains(&selected) => {}
            Some(selected) => {
                let replacement = find(selected.identity());
                if replacement.is_none() && !cluster.api_discovery.absence_is_final(&selected.group)
                {
                    return;
                }
                if let Some(cluster) = self.clusters.get_mut(&cluster_key) {
                    Self::stop_resource_watches(cluster, &selected, commands_to_send);
                    cluster.selected_api_resource = None;
                    Self::reconcile_node_watches(cluster, commands_to_send);
                }
                match replacement {
                    Some(replacement) => {
                        self.select_api_resource(cluster_key, replacement, commands_to_send);
                    }
                    None => self.forget_selected_api_resource(cluster_key),
                }
            }
            None => {
                let Some(saved) = self
                    .cluster_selections
                    .selections
                    .get(&cluster.name)
                    .and_then(|selection| selection.selected_api_resource.clone())
                else {
                    return;
                };
                let restored = if saved.matches(&ApiResource::helm_releases()) {
                    Some(ApiResource::helm_releases())
                } else {
                    find((saved.group.as_str(), saved.name.as_str()))
                };
                match restored {
                    Some(api_resource) => {
                        self.select_api_resource(cluster_key, api_resource, commands_to_send);
                    }
                    None if cluster.api_discovery.absence_is_final(&saved.group) => {
                        self.forget_selected_api_resource(cluster_key);
                    }
                    None => {}
                }
            }
        }
    }

    fn forget_selected_api_resource(&mut self, cluster_key: i32) {
        let Some(context_name) = self
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.name.clone())
        else {
            return;
        };
        if let Some(selection) = self.cluster_selections.selections.get_mut(&context_name) {
            selection.selected_api_resource = None;
        }
        self.prune_empty_cluster_selection(cluster_key);
    }
}
