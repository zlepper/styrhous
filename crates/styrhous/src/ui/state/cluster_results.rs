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
    ui.discard_resource_detail_blades(commands);
    ui.clusters.clear();
    ui.api_discovery_cache
        .contexts
        .retain(|context, _| clusters.iter().any(|cluster| &cluster.name == context));
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
    if select_cluster && let Some(cluster_key) = remembered_cluster_key.or(current_cluster_key) {
        ui.select_cluster_without_remembering(cluster_key, commands);
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
            cluster.namespaces_reconnecting = None;
            cluster.reachable = true;
        }
        ui.restore_selected_namespaces(cluster_key, _commands);
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesReconnecting {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesNamespacesReconnecting { cluster_key, error } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            if matches!(cluster.namespaces_load, ClusterLoadState::Ready) {
                cluster.namespaces_reconnecting = Some(error);
            } else {
                // Nothing usable to keep on screen yet, so surface the failure.
                cluster.namespaces_load = ClusterLoadState::Failed(error);
            }
        }
    }
}

impl WorkerResult for crate::worker::KubernetesNamespacesRecovered {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.namespaces_reconnecting = None;
        }
    }
}

/// API groups named in a toast before the rest are summarised as "and N more".
const NAMED_API_GROUPS: usize = 3;

struct FailedApiGroupsSummary {
    title: String,
    target: String,
    details: String,
}

fn api_groups_label(count: usize) -> String {
    match count {
        1 => "1 API group".to_owned(),
        _ => format!("{count} API groups"),
    }
}

/// The first few group versions, with the remainder counted.
fn name_api_groups<'a>(group_versions: impl ExactSizeIterator<Item = &'a str>) -> String {
    use std::fmt::Write;

    let count = group_versions.len();
    let mut names = group_versions
        .take(NAMED_API_GROUPS)
        .collect::<Vec<_>>()
        .join(", ");
    if count > NAMED_API_GROUPS {
        write!(names, " and {} more", count - NAMED_API_GROUPS)
            .expect("writing to a String cannot fail");
    }
    names
}

/// Ends the title of the outcome recorded when API groups could not be
/// discovered; they are retried in the background.
pub(crate) const FAILED_API_GROUPS_TITLE_SUFFIX: &str = "could not be loaded";

fn describe_failed_api_groups(
    failed_api_groups: &[crate::worker::FailedApiGroup],
) -> FailedApiGroupsSummary {
    let details = failed_api_groups
        .iter()
        .map(|group| format!("{}: {}", group.group_version, group.error))
        .collect::<Vec<_>>()
        .join("\n");
    FailedApiGroupsSummary {
        title: format!(
            "{} {FAILED_API_GROUPS_TITLE_SUFFIX}",
            api_groups_label(failed_api_groups.len())
        ),
        target: name_api_groups(
            failed_api_groups
                .iter()
                .map(|group| group.group_version.as_str()),
        ),
        details,
    }
}

impl WorkerResult for crate::worker::KubernetesApiResourcesDiscovered {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesApiResourcesDiscovered {
            cluster_key,
            api_resources,
            scalable_api_resources,
        } = self;
        let Some(cluster) = ui.clusters.get_mut(&cluster_key) else {
            return;
        };
        cluster.reachable = true;
        cluster.api_discovery.fresh.extend(ApiResourceSet {
            api_resources,
            scalable_api_resources,
        });
        ui.apply_api_discovery(cluster_key, commands);
    }
}

impl WorkerResult for crate::worker::KubernetesApiDiscoveryCompleted {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesApiDiscoveryCompleted {
            cluster_key,
            failed_api_groups,
        } = self;
        let Some(cluster) = ui.clusters.get_mut(&cluster_key) else {
            return;
        };
        cluster.api_discovery.complete = true;
        cluster.api_discovery.set_failed_group_versions(
            failed_api_groups
                .iter()
                .map(|group| group.group_version.as_str()),
        );
        ui.apply_api_discovery(cluster_key, commands);
        ui.remember_api_discovery(cluster_key);
        if !failed_api_groups.is_empty() {
            let summary = describe_failed_api_groups(&failed_api_groups);
            ui.push_operation_outcome(
                cluster_key,
                OperationOutcome::Failure,
                summary.title,
                None,
                summary.target,
                Some(format!(
                    "{}\n\nStyrhous keeps retrying these groups in the background.",
                    summary.details
                )),
            );
        }
        let Some(cluster) = ui.clusters.get(&cluster_key) else {
            return;
        };
        let (pod_metrics_available, node_metrics_available) = (
            cluster.pod_metrics_api_available,
            cluster.node_metrics_api_available,
        );
        if !pod_metrics_available {
            ui.mark_pod_metrics_api_unavailable(cluster_key);
        }
        if !node_metrics_available {
            ui.mark_node_metrics_api_unavailable(cluster_key);
        }
    }
}

impl WorkerResult for crate::worker::KubernetesApiGroupsRecovered {
    fn apply(self, ui: &mut UiState, commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesApiGroupsRecovered {
            cluster_key,
            group_versions,
            api_resources,
            scalable_api_resources,
            still_failing_group_versions,
        } = self;
        let Some(cluster) = ui.clusters.get_mut(&cluster_key) else {
            return;
        };
        // Retries only follow a completed discovery; anything else outlived its session.
        if !cluster.api_discovery.complete {
            return;
        }
        cluster.api_discovery.fresh.extend(ApiResourceSet {
            api_resources,
            scalable_api_resources,
        });
        cluster
            .api_discovery
            .set_failed_group_versions(still_failing_group_versions.iter().map(String::as_str));
        ui.apply_api_discovery(cluster_key, commands);
        ui.remember_api_discovery(cluster_key);

        let mut details = format!("Loaded: {}", group_versions.join(", "));
        if !still_failing_group_versions.is_empty() {
            details.push_str(&format!(
                "\nStill retrying: {}",
                still_failing_group_versions.join(", ")
            ));
        }
        ui.push_operation_outcome(
            cluster_key,
            OperationOutcome::Success,
            format!("{} loaded", api_groups_label(group_versions.len())),
            None,
            name_api_groups(group_versions.iter().map(String::as_str)),
            Some(details),
        );
    }
}

impl UiState {
    /// Shows what discovery knows now: rebuilds the navigation, follows the availability
    /// of the Metrics API (decided once discovery completes, or as soon as it is
    /// reported), and keeps the selection in line.
    fn apply_api_discovery(&mut self, cluster_key: i32, commands: &mut Vec<WorkerCommandBox>) {
        let Some(cluster) = self.clusters.get_mut(&cluster_key) else {
            return;
        };
        cluster.refresh_api_resources();
        let complete = cluster.api_discovery.complete;
        let pod_metrics = cluster.api_discovery.has_fresh(ApiResource::is_pod_metrics);
        let node_metrics = cluster
            .api_discovery
            .has_fresh(ApiResource::is_node_metrics);
        let restore_pod_metrics = pod_metrics && !cluster.pod_metrics_api_available;
        let restore_node_metrics = node_metrics && !cluster.node_metrics_api_available;
        if pod_metrics != cluster.pod_metrics_api_available && (pod_metrics || complete) {
            cluster.pod_metrics_api_available = pod_metrics;
            UiState::reconcile_pod_metrics(cluster, commands);
        }
        if node_metrics != cluster.node_metrics_api_available && (node_metrics || complete) {
            cluster.node_metrics_api_available = node_metrics;
            UiState::reconcile_node_metrics(cluster, commands);
        }
        self.restore_metrics_in_resource_detail_blades(
            cluster_key,
            restore_pod_metrics,
            restore_node_metrics,
            commands,
        );
        self.reconcile_selection_with_discovery(cluster_key, commands);
    }

    /// Keeps the completed discovery of a context for its next connection.
    fn remember_api_discovery(&mut self, cluster_key: i32) {
        let Some(cluster) = self.clusters.get(&cluster_key) else {
            return;
        };
        self.api_discovery_cache.contexts.insert(
            cluster.name.clone(),
            ApiResourceSet {
                api_resources: cluster.discovered_api_resources.clone(),
                scalable_api_resources: cluster.scalable_api_resources.clone(),
            },
        );
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

impl WorkerResult for crate::worker::KubernetesClusterConnectionCreated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let crate::worker::KubernetesClusterConnectionCreated { cluster_key } = self;
        // Discovery starts with the connection and may already have failed it.
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key)
            && matches!(cluster.connection, ClusterConnectionState::Connecting)
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_groups(count: usize) -> Vec<crate::worker::FailedApiGroup> {
        (1..=count)
            .map(|index| crate::worker::FailedApiGroup {
                group_version: format!("g{index}/v1"),
                error: format!("error {index}"),
            })
            .collect()
    }

    #[test]
    fn failed_api_group_summary_names_at_most_three_groups_but_details_list_all() {
        for (count, title, target) in [
            (1, "1 API group could not be loaded", "g1/v1"),
            (3, "3 API groups could not be loaded", "g1/v1, g2/v1, g3/v1"),
            (
                4,
                "4 API groups could not be loaded",
                "g1/v1, g2/v1, g3/v1 and 1 more",
            ),
            (
                5,
                "5 API groups could not be loaded",
                "g1/v1, g2/v1, g3/v1 and 2 more",
            ),
        ] {
            let summary = describe_failed_api_groups(&failed_groups(count));

            assert_eq!(summary.title, title);
            assert_eq!(summary.target, target);
            let expected_details = (1..=count)
                .map(|index| format!("g{index}/v1: error {index}"))
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(summary.details, expected_details);
        }
    }
}
