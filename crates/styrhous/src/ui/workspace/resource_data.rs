use crate::api_resource::ApiResource;
use crate::ui::state::{
    ClusterState, NamespaceScope, NodeMetricsState, NodePodCountsState, PodMetricsNamespaceState,
    ResourceSearchState, ResourceWatchKey, ResourceWatchState,
};
use crate::ui::workspace::resource_table::ResourceTableConfiguration;
use std::collections::HashMap;

mod metrics;
mod preparation;
mod watches;

pub(super) use metrics::resolved_resource_cell;
use preparation::prepare_resource_table;
pub(super) use preparation::{CLUSTER_GROUP, resolve_prepared_resource};
pub(in crate::ui) use watches::{ListedCounts, listed_count};
pub(super) use watches::{
    awaited_namespace_list, selected_resources, selected_watch_error, selected_watches_are_loading,
};

#[derive(Clone, Copy)]
pub(super) struct ResourceMetrics<'a> {
    pub(super) pod_metrics_api_available: bool,
    pub(super) pod_metrics: &'a HashMap<String, PodMetricsNamespaceState>,
    pub(super) node_metrics_api_available: bool,
    pub(super) node_metrics: &'a NodeMetricsState,
    pub(super) node_pod_counts: &'a NodePodCountsState,
}

impl<'a> ResourceMetrics<'a> {
    /// The metrics a table shows, with the availability `ClusterState::metrics_shown_as_available`
    /// decides.
    pub(super) fn new(
        (pod_metrics_api_available, node_metrics_api_available): (bool, bool),
        pod_metrics: &'a HashMap<String, PodMetricsNamespaceState>,
        node_metrics: &'a NodeMetricsState,
        node_pod_counts: &'a NodePodCountsState,
    ) -> Self {
        Self {
            pod_metrics_api_available,
            pod_metrics,
            node_metrics_api_available,
            node_metrics,
            node_pod_counts,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ResourceTableData<'a> {
    pub(super) selected_namespaces: NamespaceScope<'a>,
    /// Whether the resource is watched across every namespace, known or not.
    pub(super) watches_every_namespace: bool,
    pub(super) resource_cache: &'a HashMap<ResourceWatchKey, ResourceWatchState>,
    pub(super) metrics: ResourceMetrics<'a>,
}

impl<'a> ResourceTableData<'a> {
    /// What the table of `api_resource` shows from `cluster`.
    pub(super) fn of(cluster: &'a ClusterState, api_resource: &ApiResource) -> Self {
        Self {
            selected_namespaces: cluster.namespace_scope(),
            watches_every_namespace: cluster.watches_every_namespace(api_resource),
            resource_cache: &cluster.resource_cache,
            metrics: ResourceMetrics::new(
                cluster.metrics_shown_as_available(),
                &cluster.pod_metrics,
                &cluster.node_metrics,
                &cluster.node_pod_counts,
            ),
        }
    }
}

/// Prepares `cluster`'s table of `api_resource` in its table cache, reusing what is there
/// when nothing changed.
pub(super) fn prepare_cluster_table(
    cluster: &mut ClusterState,
    api_resource: &ApiResource,
    resource_search: &ResourceSearchState,
    configuration: &ResourceTableConfiguration,
) {
    // The cache is taken out for the duration, so the rest of the cluster can be read.
    let mut cache = std::mem::take(&mut cluster.resources.resource_table_cache);
    prepare_resource_table(
        &mut cache,
        ResourceTableData::of(cluster, api_resource),
        api_resource,
        resource_search,
        configuration,
    );
    cluster.resources.resource_table_cache = cache;
}

#[cfg(test)]
mod tests;
