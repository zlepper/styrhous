//! The sidebar's kind list (C09 `KindList`): what it shows for a cluster, as
//! plain rows the sidebar paints.

use super::super::persistence::PinnedKinds;
use super::super::state::{ClusterLoadState, ClusterState, ResourceNavigationExpansion};
use super::super::workspace::ListedCounts;
use crate::api_resource::ApiResource;
use crate::resource_catalog::{CuratedNavigationEntry, kind_category};
use components::fuzzy::normalize_for_search;

/// A kind the list can open, with what the app knows of its objects.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct KindRow {
    pub(super) api_resource: ApiResource,
    pub(super) label: String,
    /// The category colour leading a top-level kind (`theme.category.nth`).
    pub(super) category: usize,
    /// How many objects the list shows; `None` until a watch has listed the
    /// kind (it is opened at least once in the session).
    pub(super) count: Option<usize>,
    pub(super) pinned: bool,
}

/// One row of the kind list, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum KindListItem {
    /// A section caption with an optional detail at its right.
    Caption {
        label: &'static str,
        detail: Option<String>,
    },
    /// A kind led by its colour: pinned, standing on its own, or a filter match.
    Kind(KindRow),
    /// A collapsible group of the Kubernetes taxonomy with how many kinds it
    /// holds.
    Group {
        node_id: String,
        label: &'static str,
        kinds: usize,
        expanded: bool,
    },
    /// A kind inside an expanded group.
    Child(KindRow),
    /// A collapsible API group of custom resources.
    ApiGroup {
        node_id: String,
        group: String,
        kinds: usize,
        expanded: bool,
    },
}

/// The node id that keeps a taxonomy group's expansion.
pub(super) fn group_node_id(group: &str) -> String {
    format!("section:{group}")
}

/// The node id that keeps a custom resource API group's expansion.
pub(super) fn api_group_node_id(group: &str) -> String {
    format!("other-resource-group:{group}")
}

/// What the kind list of `cluster` holds.
pub(super) struct KindList {
    pub(super) items: Vec<KindListItem>,
    /// Every kind the list can open, for the filter's placeholder.
    pub(super) kind_count: usize,
}

impl KindList {
    /// The list of `cluster`'s kinds: pinned kinds first, then the Kubernetes
    /// taxonomy (pinned kinds leave their group's children but count in it),
    /// then custom resources by API group. A non-empty `filter` instead lists
    /// the kinds it matches, best first.
    pub(super) fn new(
        cluster: &ClusterState,
        pinned: &PinnedKinds,
        expansion: &ResourceNavigationExpansion,
        filter: &str,
    ) -> Self {
        let navigation = &cluster.resource_navigation;
        let counts = ListedCounts::new(cluster);
        let row = |api_resource: &ApiResource| KindRow {
            api_resource: api_resource.clone(),
            label: api_resource.display_name(),
            category: kind_category(api_resource),
            count: kind_count(cluster, &counts, api_resource),
            pinned: pinned.contains(api_resource),
        };
        let kubernetes_kinds = navigation
            .curated_entries
            .iter()
            .map(|entry| match entry {
                CuratedNavigationEntry::Resource(_) => 1,
                CuratedNavigationEntry::Section(section) => section.api_resources.len(),
            })
            .sum::<usize>();
        let custom_kinds: usize = navigation.other_api_groups.values().map(Vec::len).sum();
        let kind_count = kubernetes_kinds + custom_kinds;

        if !filter.trim().is_empty() {
            return Self {
                items: filtered(navigation, filter, row),
                kind_count,
            };
        }

        let mut items = Vec::new();
        let pinned_rows = pinned
            .kinds
            .iter()
            .filter_map(|kind| {
                navigation
                    .all_api_resources()
                    .find(|api_resource| kind.matches(api_resource))
            })
            .map(row)
            .collect::<Vec<_>>();
        if !pinned_rows.is_empty() {
            items.push(KindListItem::Caption {
                label: "Pinned",
                detail: None,
            });
            items.extend(pinned_rows.into_iter().map(KindListItem::Kind));
        }

        if kubernetes_kinds > 0 {
            items.push(KindListItem::Caption {
                label: "Kubernetes",
                detail: Some(kind_count.to_string()),
            });
        }
        for entry in &navigation.curated_entries {
            match entry {
                CuratedNavigationEntry::Resource(api_resource) => {
                    if !pinned.contains(api_resource) {
                        items.push(KindListItem::Kind(row(api_resource)));
                    }
                }
                CuratedNavigationEntry::Section(section) => {
                    let node_id = group_node_id(section.name);
                    let expanded = expansion.expanded_nodes.contains(&node_id);
                    items.push(KindListItem::Group {
                        node_id,
                        label: section.name,
                        kinds: section.api_resources.len(),
                        expanded,
                    });
                    if expanded {
                        items.extend(
                            section
                                .api_resources
                                .iter()
                                .filter(|api_resource| !pinned.contains(api_resource))
                                .map(|api_resource| KindListItem::Child(row(api_resource))),
                        );
                    }
                }
            }
        }

        if !navigation.other_api_groups.is_empty() {
            let groups = navigation.other_api_groups.len();
            items.push(KindListItem::Caption {
                label: "Custom resources",
                detail: Some(crate::ui::widgets::count_noun(groups, "group")),
            });
            for (group, api_resources) in &navigation.other_api_groups {
                let node_id = api_group_node_id(group);
                let expanded = expansion.expanded_nodes.contains(&node_id);
                items.push(KindListItem::ApiGroup {
                    node_id,
                    group: group.clone(),
                    kinds: api_resources.len(),
                    expanded,
                });
                if expanded {
                    items.extend(
                        api_resources
                            .iter()
                            .filter(|api_resource| !pinned.contains(api_resource))
                            .map(|api_resource| KindListItem::Child(row(api_resource))),
                    );
                }
            }
        }
        Self { items, kind_count }
    }

    /// The first kind the list offers (the filter's Enter opens it).
    pub(super) fn first_kind(&self) -> Option<&ApiResource> {
        self.items.iter().find_map(|item| match item {
            KindListItem::Kind(row) | KindListItem::Child(row) => Some(&row.api_resource),
            _ => None,
        })
    }
}

/// How well `api_resource` matches the normalized `needle`, best first: its
/// name starts with it, a word of its name does, or its name, resource name or
/// API group contains it. `None` when it does not match.
fn match_rank(api_resource: &ApiResource, needle: &str) -> Option<u8> {
    let normalize = |text: &str| normalize_for_search(text).collect::<String>();
    let label = normalize(&api_resource.display_name());
    if label.starts_with(needle) || api_resource.name.starts_with(needle) {
        Some(0)
    } else if label.split(' ').any(|word| word.starts_with(needle)) {
        Some(1)
    } else if label.contains(needle)
        || api_resource.name.contains(needle)
        || normalize(&api_resource.group).contains(needle)
    {
        Some(2)
    } else {
        None
    }
}

/// The kinds `filter` matches, best match first and otherwise in the list's
/// order, under a caption counting them.
fn filtered(
    navigation: &crate::resource_catalog::ResourceNavigation,
    filter: &str,
    row: impl Fn(&ApiResource) -> KindRow,
) -> Vec<KindListItem> {
    let needle = normalize_for_search(filter.trim()).collect::<String>();
    let mut matches = navigation
        .all_api_resources()
        .filter_map(|api_resource| Some((match_rank(api_resource, &needle)?, api_resource)))
        .collect::<Vec<_>>();
    matches.sort_by_key(|(rank, _)| *rank);
    let mut items = vec![KindListItem::Caption {
        label: "Matches",
        detail: Some(matches.len().to_string()),
    }];
    items.extend(
        matches
            .into_iter()
            .map(|(_, api_resource)| KindListItem::Kind(row(api_resource))),
    );
    items
}

/// What the app knows of `api_resource`'s objects in the selected namespaces.
fn kind_count(
    cluster: &ClusterState,
    counts: &ListedCounts<'_>,
    api_resource: &ApiResource,
) -> Option<usize> {
    if api_resource.is_helm_releases() {
        let scope = cluster.namespace_scope();
        let mut total = 0;
        for namespace in scope.iter() {
            let watch = cluster
                .helm_release_cache
                .get(namespace)
                .filter(|watch| watch.is_synced)?;
            total += watch.releases.len();
        }
        return (!scope.is_empty()).then_some(total);
    }
    if api_resource.identity() == (crate::api_resource::CORE_GROUP, "namespaces") {
        // The namespace list is always watched.
        return matches!(cluster.namespaces_load, ClusterLoadState::Ready)
            .then(|| cluster.namespaces.len());
    }
    counts.get(api_resource)
}
