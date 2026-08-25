use super::*;

pub(super) fn selected_watch_error(
    cluster: &super::super::state::ClusterState,
    api_resource: &crate::api_resource::ApiResource,
) -> Option<String> {
    resource_watch_namespaces(cluster, api_resource)
        .into_iter()
        .find_map(|namespace| {
            cluster
                .resource_cache
                .get(&(api_resource.clone(), namespace))
                .and_then(|watch| watch.error.clone())
        })
}

pub(super) fn selected_watches_are_loading(
    cluster: &super::super::state::ClusterState,
    api_resource: &crate::api_resource::ApiResource,
) -> bool {
    resource_watch_namespaces(cluster, api_resource)
        .into_iter()
        .any(|namespace| {
            cluster
                .resource_cache
                .get(&(api_resource.clone(), namespace))
                .is_none_or(|watch| !watch.is_synced)
        })
}

pub(super) fn selected_resources(
    cluster: &super::super::state::ClusterState,
    api_resource: Option<&crate::api_resource::ApiResource>,
) -> Vec<MinimalResource> {
    let Some(api_resource) = api_resource else {
        return Vec::new();
    };
    let mut resources = Vec::new();
    for namespace in resource_watch_namespaces(cluster, api_resource) {
        if let Some(state) = cluster
            .resource_cache
            .get(&(api_resource.clone(), namespace))
        {
            resources.extend(state.resources.values().cloned());
        }
    }
    resources.sort_by_key(|resource| resource.name.to_lowercase());
    resources
}

pub(super) fn decorate_usage_rows(
    cluster: &super::super::state::ClusterState,
    api_resource: Option<&crate::api_resource::ApiResource>,
    mut resources: Vec<MinimalResource>,
) -> Vec<MinimalResource> {
    let is_pod =
        api_resource.is_some_and(|resource| resource.group == "core" && resource.kind == "Pod");
    let is_node =
        api_resource.is_some_and(|resource| resource.group == "core" && resource.kind == "Node");
    if !is_pod && !is_node {
        return resources;
    }
    for resource in &mut resources {
        if is_node {
            if !cluster.node_metrics_api_available || cluster.node_metrics.error.is_some() {
                resource
                    .cells
                    .insert(CPU_COLUMN.into(), CellValue::Text("Unavailable".into()));
                resource
                    .cells
                    .insert(MEMORY_COLUMN.into(), CellValue::Text("Unavailable".into()));
            } else if let Some(usage) = cluster.node_metrics.usages.get(&resource.name) {
                resource.cells.insert(
                    CPU_COLUMN.into(),
                    CellValue::Usage {
                        label: format_cpu_cores(usage.cpu_nanocores),
                        value: usage.cpu_nanocores,
                    },
                );
                resource.cells.insert(
                    MEMORY_COLUMN.into(),
                    CellValue::Usage {
                        label: format_memory(usage.memory_bytes),
                        value: usage.memory_bytes,
                    },
                );
            }
            continue;
        }
        let Some(namespace) = resource.namespace.as_deref() else {
            continue;
        };
        let metrics = cluster.pod_metrics.get(namespace);
        if !cluster.pod_metrics_api_available
            || metrics.is_some_and(|metrics| metrics.error.is_some())
        {
            resource
                .cells
                .insert(CPU_COLUMN.into(), CellValue::Text("Unavailable".into()));
            resource
                .cells
                .insert(MEMORY_COLUMN.into(), CellValue::Text("Unavailable".into()));
        } else if let Some(usage) = metrics.and_then(|metrics| metrics.usages.get(&resource.name)) {
            resource.cells.insert(
                CPU_COLUMN.into(),
                CellValue::Usage {
                    label: format_cpu_cores(usage.cpu_nanocores),
                    value: usage.cpu_nanocores,
                },
            );
            resource.cells.insert(
                MEMORY_COLUMN.into(),
                CellValue::Usage {
                    label: format_memory(usage.memory_bytes),
                    value: usage.memory_bytes,
                },
            );
        }
    }
    resources
}

pub(super) fn resource_watch_namespaces(
    cluster: &super::super::state::ClusterState,
    api_resource: &crate::api_resource::ApiResource,
) -> Vec<Option<String>> {
    if api_resource.namespaced {
        cluster
            .selected_namespaces
            .iter()
            .cloned()
            .map(Some)
            .collect()
    } else {
        vec![None]
    }
}
