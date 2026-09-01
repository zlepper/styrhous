use crate::api_resource::ApiResource;
use crate::minimal_resource::MinimalResource;
use crate::ui::state::ClusterState;
use std::collections::HashSet;

pub(in crate::ui::workspace) fn selected_watch_error(
    cluster: &ClusterState,
    api_resource: &ApiResource,
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

pub(in crate::ui::workspace) fn selected_watches_are_loading(
    cluster: &ClusterState,
    api_resource: &ApiResource,
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

pub(in crate::ui::workspace) fn selected_resources(
    cluster: &ClusterState,
    api_resource: Option<&ApiResource>,
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

fn resource_watch_namespaces(
    cluster: &ClusterState,
    api_resource: &ApiResource,
) -> Vec<Option<String>> {
    resource_watch_namespaces_for(&cluster.selected_namespaces, api_resource)
}

pub(super) fn resource_watch_namespaces_for(
    selected_namespaces: &HashSet<String>,
    api_resource: &ApiResource,
) -> Vec<Option<String>> {
    if api_resource.namespaced {
        selected_namespaces.iter().cloned().map(Some).collect()
    } else {
        vec![None]
    }
}
