use crate::ui::state::{
    NodeMetricsState, PodMetricsNamespaceState, ResourceWatchKey, ResourceWatchState,
};
use std::collections::{HashMap, HashSet};

mod metrics;
mod preparation;
mod watches;

pub(super) use metrics::resolved_resource_cell;
pub(super) use preparation::{prepare_resource_table, resolve_prepared_resource};
pub(super) use watches::{selected_resources, selected_watch_error, selected_watches_are_loading};

#[derive(Clone, Copy)]
pub(super) struct ResourceMetrics<'a> {
    pub(super) pod_metrics_api_available: bool,
    pub(super) pod_metrics: &'a HashMap<String, PodMetricsNamespaceState>,
    pub(super) node_metrics_api_available: bool,
    pub(super) node_metrics: &'a NodeMetricsState,
}

#[derive(Clone, Copy)]
pub(super) struct ResourceTableData<'a> {
    pub(super) selected_namespaces: &'a HashSet<String>,
    pub(super) resource_cache: &'a HashMap<ResourceWatchKey, ResourceWatchState>,
    pub(super) metrics: ResourceMetrics<'a>,
}

#[cfg(test)]
mod tests;
