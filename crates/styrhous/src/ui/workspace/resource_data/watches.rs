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

/// How many objects of `api_resource` its list shows in the selected namespaces,
/// once every watch behind them has listed them; `None` while a watch is missing
/// (a kind not opened this session) or still listing.
pub(in crate::ui) fn listed_count(
    cluster: &ClusterState,
    api_resource: &ApiResource,
) -> Option<usize> {
    let entries = sorted_entries(
        cluster
            .resource_cache
            .iter()
            .filter(|((resource, _), _)| resource == api_resource),
    );
    count_listed(cluster, api_resource, KindWatches(&entries))
}

/// [`listed_count`] of every kind, from one pass over the cache. The sidebar
/// counts dozens of kinds each frame; looking each up on its own scans the
/// whole cache once per kind when watching every namespace.
pub(in crate::ui) struct ListedCounts<'a> {
    cluster: &'a ClusterState,
    /// Every cache entry, by kind and then namespace.
    entries: Vec<CacheEntry<'a>>,
}

impl<'a> ListedCounts<'a> {
    pub(in crate::ui) fn new(cluster: &'a ClusterState) -> Self {
        Self {
            cluster,
            entries: sorted_entries(cluster.resource_cache.iter()),
        }
    }

    /// [`listed_count`] of `api_resource`.
    pub(in crate::ui) fn get(&self, api_resource: &ApiResource) -> Option<usize> {
        let start = self
            .entries
            .partition_point(|(resource, ..)| *resource < api_resource);
        let end = start
            + self.entries[start..].partition_point(|(resource, ..)| *resource == api_resource);
        count_listed(
            self.cluster,
            api_resource,
            KindWatches(&self.entries[start..end]),
        )
    }
}

/// A cache entry: its kind, namespace and watch.
type CacheEntry<'a> = (&'a ApiResource, Option<&'a str>, &'a ResourceWatchState);

/// `entries` sorted by kind and then namespace (the entry without one first).
fn sorted_entries<'a>(
    entries: impl Iterator<Item = (&'a ResourceWatchKey, &'a ResourceWatchState)>,
) -> Vec<CacheEntry<'a>> {
    let mut entries: Vec<CacheEntry<'a>> = entries
        .map(|((resource, namespace), watch)| (resource, namespace.as_deref(), watch))
        .collect();
    entries.sort_unstable_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    entries
}

/// The cache entries of one kind, by namespace: the entry without a
/// namespace (a cluster-scoped kind, or a watch across every namespace) first.
#[derive(Clone, Copy)]
struct KindWatches<'e, 'a>(&'e [CacheEntry<'a>]);

impl<'a> KindWatches<'_, 'a> {
    fn unscoped(self) -> Option<&'a ResourceWatchState> {
        match self.0.first() {
            Some((_, None, watch)) => Some(watch),
            _ => None,
        }
    }

    fn namespaced(self) -> impl Iterator<Item = &'a ResourceWatchState> {
        self.0
            .iter()
            .filter(|(_, namespace, _)| namespace.is_some())
            .map(|(.., watch)| *watch)
    }

    fn in_namespace(self, namespace: &str) -> Option<&'a ResourceWatchState> {
        self.0
            .binary_search_by(|(_, entry, _)| entry.cmp(&Some(namespace)))
            .ok()
            .map(|index| self.0[index].2)
    }
}

/// [`listed_count`] of `api_resource`, whose cache entries are `watches`.
/// Mirrors what the table shows: [`selected_watches_are_loading`] and
/// [`resource_watch_namespaces`].
fn count_listed(
    cluster: &ClusterState,
    api_resource: &ApiResource,
    watches: KindWatches<'_, '_>,
) -> Option<usize> {
    fn synced(watch: Option<&ResourceWatchState>) -> Option<&ResourceWatchState> {
        watch.filter(|watch| watch.is_synced)
    }
    if !api_resource.namespaced {
        return synced(watches.unscoped()).map(|watch| watch.resources.len());
    }
    if cluster.watches_every_namespace(api_resource) {
        // The watch's own entry carries its sync state; it shows every namespace it has rows for.
        synced(watches.unscoped())?;
        let mut namespaces = watches.namespaced().peekable();
        namespaces.peek()?;
        return Some(namespaces.map(|watch| watch.resources.len()).sum());
    }
    if cluster.awaited_namespace_list() == NamespaceListWait::Loading {
        return None;
    }
    let scope = cluster.namespace_scope();
    // No namespace selected: nothing is listed, which is not a count of zero.
    if scope.is_empty() {
        return None;
    }
    let mut total = 0;
    for namespace in scope.iter() {
        total += synced(watches.in_namespace(namespace))?.resources.len();
    }
    Some(total)
}
