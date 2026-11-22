use crate::api_resource::ApiResource;
use crate::minimal_resource::MinimalResource;
use crate::ui::state::{
    ClusterState, NamespaceListWait, NamespaceScope, ResourceWatchKey, ResourceWatchState,
};
use std::collections::HashMap;

pub(in crate::ui::workspace) fn selected_watch_error(
    cluster: &ClusterState,
    api_resource: &ApiResource,
) -> Option<String> {
    watch_state_keys(cluster, api_resource)
        .into_iter()
        .find_map(|key| {
            cluster
                .resource_cache
                .get(&key)
                .and_then(|watch| watch.error.clone())
        })
}

pub(in crate::ui::workspace) fn selected_watches_are_loading(
    cluster: &ClusterState,
    api_resource: &ApiResource,
) -> bool {
    awaited_namespace_list(cluster, api_resource) == NamespaceListWait::Loading
        || watch_state_keys(cluster, api_resource)
            .into_iter()
            .any(|key| {
                cluster
                    .resource_cache
                    .get(&key)
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

/// What the selected watches wait for in the namespace list: only "All namespaces" without
/// a watch across every namespace (Helm releases are watched per namespace) needs it.
pub(in crate::ui::workspace) fn awaited_namespace_list<'a>(
    cluster: &'a ClusterState,
    api_resource: &ApiResource,
) -> NamespaceListWait<'a> {
    if api_resource.namespaced && !cluster.watches_every_namespace(api_resource) {
        cluster.awaited_namespace_list()
    } else {
        NamespaceListWait::Ready
    }
}

/// The cache entries whose sync state and error describe the selected watches. A watch
/// across every namespace keeps them in its own entry.
fn watch_state_keys(cluster: &ClusterState, api_resource: &ApiResource) -> Vec<ResourceWatchKey> {
    if cluster.watches_every_namespace(api_resource) {
        vec![(api_resource.clone(), None)]
    } else {
        resource_watch_namespaces(cluster, api_resource)
            .into_iter()
            .map(|namespace| (api_resource.clone(), namespace))
            .collect()
    }
}

fn resource_watch_namespaces(
    cluster: &ClusterState,
    api_resource: &ApiResource,
) -> Vec<Option<String>> {
    resource_watch_namespaces_for(
        cluster.namespace_scope(),
        cluster.watches_every_namespace(api_resource),
        &cluster.resource_cache,
        api_resource,
    )
}

/// The cache entries whose rows are shown. A watch across every namespace shows every
/// namespace it has rows for, whether or not the namespace list has arrived.
pub(super) fn resource_watch_namespaces_for(
    selected_namespaces: NamespaceScope<'_>,
    watches_every_namespace: bool,
    resource_cache: &HashMap<ResourceWatchKey, ResourceWatchState>,
    api_resource: &ApiResource,
) -> Vec<Option<String>> {
    if !api_resource.namespaced {
        vec![None]
    } else if watches_every_namespace {
        resource_cache
            .keys()
            .filter(|(resource, namespace)| resource == api_resource && namespace.is_some())
            .map(|(_, namespace)| namespace.clone())
            .collect()
    } else {
        selected_namespaces.iter().cloned().map(Some).collect()
    }
}
