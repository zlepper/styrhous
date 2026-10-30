use super::*;

impl WorkerResult for crate::worker::ClusterConnectionFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.connection = ClusterConnectionState::Failed(self.error);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesClustersUpdated {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesClustersUpdated(clusters) = self;
        apply_kubernetes_clusters(ui, clusters, true, commands);
    }
}

impl WorkerResult for crate::worker::ImportedKubernetesClusters {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::ImportedKubernetesClusters(clusters) = self;
        let context_names = clusters
            .iter()
            .map(|cluster| cluster.name.as_str())
            .collect::<std::collections::HashSet<_>>();
        for cluster in &mut ui.managed_cluster_discovery.aws.clusters {
            cluster.configured |= context_names.contains(cluster.arn().as_str());
        }
        for cluster in clusters {
            if ui
                .clusters
                .values()
                .any(|existing| existing.name == cluster.name)
            {
                continue;
            }
            ui.next_cluster_key += 1;
            let cluster_key = ui.next_cluster_key;
            ui.clusters
                .insert(cluster_key, cluster_state(cluster_key, cluster));
        }
        commands.push(Box::new(crate::worker::LoadManagedClusterDiscovery));
    }
}

impl WorkerResult for crate::worker::AwsAccountsUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let aws = &mut ui.managed_cluster_discovery.aws;
        if self.request_id != aws.request_id || !aws.account_loading {
            return;
        }
        aws.cli_installed = self.cli_installed;
        aws.accounts = self.accounts;
        aws.account_warning = self.warning;
        aws.account_error = self.error;
        aws.account_loading = false;
        // Refresh already cleared the previous scan when it was requested.
    }
}

impl WorkerResult for crate::worker::EksClustersUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let aws = &mut ui.managed_cluster_discovery.aws;
        if self.request_id != aws.request_id
            || !aws.scan_loading
            || aws.selected_account_id.as_deref() != Some(&self.account_id)
            || aws.selected_profile.as_ref() != Some(&self.profile)
        {
            return;
        }
        aws.clusters = self.clusters;
        aws.scan_warning = self.warning;
        aws.scan_error = None;
        aws.scan_loading = false;
    }
}

impl WorkerResult for crate::worker::EksDiscoveryFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let aws = &mut ui.managed_cluster_discovery.aws;
        if self.request_id != aws.request_id || !aws.scan_loading {
            return;
        }
        aws.scan_loading = false;
        aws.scan_error = Some(self.error);
    }
}

impl WorkerResult for crate::worker::EksImportFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if !matches!(
            ui.managed_cluster_discovery.importing.as_ref(),
            Some(ManagedClusterImport::Eks { .. })
        ) {
            return;
        }
        if self.request_id == ui.managed_cluster_discovery.aws.request_id {
            ui.managed_cluster_discovery.aws.import_error = Some(self.error);
        }
        ui.managed_cluster_discovery.importing = None;
    }
}

fn apply_kubernetes_clusters(
    ui: &mut UiState,
    clusters: Vec<Cluster>,
    select_cluster: bool,
    commands: &mut Vec<WorkerCommandBox>,
) {
    if ui.global_blades.navigator().is_some_and(|navigator| {
        navigator
            .entries()
            .any(|content| content.resource_detail().is_some())
    }) {
        UiState::stop_discarded_blades(ui.global_blades.clear(), commands);
    }
    ui.clusters.clear();
    ui.selected_cluster = None;
    let mut current_cluster_key = None;
    let mut remembered_cluster_key = None;
    for cluster in clusters {
        ui.next_cluster_key += 1;
        let cluster_key = ui.next_cluster_key;
        if cluster.is_current {
            current_cluster_key = Some(cluster_key);
        }
        if ui.cluster_selections.last_selected_context.as_deref() == Some(cluster.name.as_str()) {
            remembered_cluster_key = Some(cluster_key);
        }
        ui.clusters
            .insert(cluster_key, cluster_state(cluster_key, cluster));
    }
    if select_cluster
        && let Some(cluster_key) = remembered_cluster_key.or(current_cluster_key)
        && let Some(command) = ui.select_cluster_without_remembering(cluster_key)
    {
        commands.push(command);
    }
}

fn cluster_state(cluster_key: i32, cluster: Cluster) -> ClusterState {
    let mut state = ClusterState::new(cluster_key, cluster.name);
    state.presentation = cluster.presentation;
    state
}

impl WorkerResult for crate::worker::ManagedClusterDiscoveryUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        ui.managed_cluster_discovery.tools = self.tools;
        ui.managed_cluster_discovery.aks_clusters = self.aks_clusters;
        ui.managed_cluster_discovery.tailscale_clusters = self.tailscale_clusters;
        ui.managed_cluster_discovery.azure_error = self.azure_error;
        ui.managed_cluster_discovery.azure_warning = self.azure_warning;
        ui.managed_cluster_discovery.tailscale_error = self.tailscale_error;
        ui.managed_cluster_discovery.loading = false;
        ui.managed_cluster_discovery.error = None;
    }
}

impl WorkerResult for crate::worker::ManagedClusterImported {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        ui.managed_cluster_discovery.importing = None;
        ui.managed_cluster_discovery.loading = true;
        ui.managed_cluster_discovery.error = None;
        commands.push(Box::new(crate::worker::LoadImportedClusters));
    }
}

impl WorkerResult for crate::worker::ManagedClusterDiscoveryFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        ui.managed_cluster_discovery.loading = false;
        ui.managed_cluster_discovery.importing = None;
        ui.managed_cluster_discovery.error = Some(self.error);
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesAdded {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesNamespacesAdded {
            cluster_key,
            namespace,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster
                .namespaces
                .insert(SortedName::new(&namespace.name), namespace);
            UiState::reconcile_after_namespace_change(cluster, commands);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesDeleted {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesNamespacesDeleted {
            cluster_key,
            namespace_name,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.namespaces.remove(&SortedName::new(&namespace_name));
            cluster.selected_namespaces.remove(&namespace_name);
            UiState::reconcile_after_namespace_change(cluster, commands);
        }
        if let Some(context_name) = ui
            .clusters
            .get(&cluster_key)
            .map(|cluster| cluster.name.clone())
            && let Some(selection) = ui.cluster_selections.selections.get_mut(&context_name)
        {
            selection.selected_namespaces.remove(&namespace_name);
            ui.prune_empty_cluster_selection(cluster_key);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesReplaced {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesNamespacesReplaced {
            cluster_key,
            namespaces,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.namespaces = namespaces
                .into_iter()
                .map(|namespace| (SortedName::new(&namespace.name), namespace))
                .collect();
            cluster.namespaces_load = ClusterLoadState::Ready;
        }
        ui.restore_selected_namespaces(cluster_key, _commands);
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesLoadFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesNamespacesLoadFailed { cluster_key, error } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.namespaces_load = ClusterLoadState::Failed(error);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesApisLoaded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesApisLoaded {
            cluster_key,
            api_resources,
            scalable_api_resources,
            pod_metrics_api_available,
            node_metrics_api_available,
        } = self;
        let restored_api_resource = ui.restored_api_resource(cluster_key, &api_resources);
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.resource_navigation = build_resource_navigation(api_resources);
            cluster.scalable_api_resources = scalable_api_resources;
            cluster.pod_metrics_api_available = pod_metrics_api_available;
            cluster.node_metrics_api_available = node_metrics_api_available;
            cluster.api_resources_load = ClusterLoadState::Ready;
        }
        if !pod_metrics_api_available {
            ui.mark_pod_metrics_api_unavailable(cluster_key);
        }
        if !node_metrics_api_available {
            ui.mark_node_metrics_api_unavailable(cluster_key);
        }
        if let Some(api_resource) = restored_api_resource {
            ui.select_api_resource(cluster_key, api_resource, _commands);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesCustomResourceColumnsLoaded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesCustomResourceColumnsLoaded {
            cluster_key,
            columns,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.custom_resource_columns.extend(columns);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesResourceSchemasLoaded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesResourceSchemasLoaded {
            cluster_key,
            schemas,
        } = self;
        for (api_resource, schema) in schemas {
            ui.resource_schemas
                .insert((cluster_key, api_resource.clone()), schema.clone());
            for editor in ui.yaml_editors.values_mut().filter(|editor| {
                editor.cluster_key == cluster_key && editor.api_resource == api_resource
            }) {
                editor.schema = Some(schema.clone());
                editor.schema_loading = false;
                editor.validation_revision = 0;
            }
        }
    }
}

impl WorkerResult for crate::worker::KubernetesApisLoadFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesApisLoadFailed { cluster_key, error } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.api_resources_load = ClusterLoadState::Failed(error);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesClusterConnectionCreated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesClusterConnectionCreated { cluster_key } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.connection = ClusterConnectionState::Connected;
        }
    }
}

impl WorkerResult for crate::worker::ContextPresentationSaved {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(save) = ui.context_presentation_saves.get_mut(&self.context_name)
            && save.request_id == self.request_id
        {
            save.result = Some(Ok(()));
            for cluster in ui
                .clusters
                .values_mut()
                .filter(|cluster| cluster.name == self.context_name)
            {
                cluster.presentation = self.presentation.clone();
            }
        }
    }
}

impl WorkerResult for crate::worker::ContextPresentationSaveFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(save) = ui.context_presentation_saves.get_mut(&self.context_name)
            && save.request_id == self.request_id
        {
            save.result = Some(Err(self.error));
        }
    }
}
