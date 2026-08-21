use crate::api_resource::ApiResource;
use crate::resource_detail::ResourceOwner;
use std::collections::{BTreeMap, BTreeSet};

/// A fixed section in the primary resource navigator.
#[derive(Debug, Clone)]
pub(super) struct CuratedResourceSection {
    pub(super) name: &'static str,
    pub(super) api_resources: Vec<ApiResource>,
}

/// A primary resource or a fixed section in the resource navigator.
#[derive(Debug, Clone)]
pub(super) enum CuratedNavigationEntry {
    Resource(ApiResource),
    Section(CuratedResourceSection),
}

/// Resource navigation derived from Kubernetes discovery.
///
/// The section order and the resources assigned to it are intentionally static.
/// Discovery only decides which resources exist on the connected cluster and provides
/// the server-supported API version used by the watcher.
#[derive(Debug, Clone, Default)]
pub(super) struct ResourceNavigation {
    pub(super) curated_entries: Vec<CuratedNavigationEntry>,
    pub(super) other_api_groups: BTreeMap<String, Vec<ApiResource>>,
}

impl ResourceNavigation {
    pub(super) fn api_resource_for_owner(&self, owner: &ResourceOwner) -> Option<ApiResource> {
        let group = owner.api_group();
        self.api_resources()
            .find(|resource| resource.group == group && resource.kind == owner.kind)
            .cloned()
    }

    fn api_resources(&self) -> impl Iterator<Item = &ApiResource> {
        self.curated_entries
            .iter()
            .flat_map(|entry| match entry {
                CuratedNavigationEntry::Resource(resource) => std::slice::from_ref(resource),
                CuratedNavigationEntry::Section(section) => section.api_resources.as_slice(),
            })
            .chain(self.other_api_groups.values().flatten())
    }
}

#[derive(Clone, Copy)]
struct CatalogEntry {
    group: &'static str,
    name: &'static str,
}

struct CatalogSection {
    name: &'static str,
    entries: &'static [CatalogEntry],
}

enum CatalogItem {
    Resource(CatalogEntry),
    Section(CatalogSection),
}

const CORE: &str = "core";

const CATALOG: &[CatalogItem] = &[
    CatalogItem::Resource(CatalogEntry {
        group: CORE,
        name: "nodes",
    }),
    CatalogItem::Section(CatalogSection {
        name: "Apps & Containers",
        entries: &[
            CatalogEntry {
                group: CORE,
                name: "pods",
            },
            CatalogEntry {
                group: "apps",
                name: "deployments",
            },
            CatalogEntry {
                group: "apps",
                name: "statefulsets",
            },
            CatalogEntry {
                group: "apps",
                name: "daemonsets",
            },
            CatalogEntry {
                group: "apps",
                name: "replicasets",
            },
            CatalogEntry {
                group: CORE,
                name: "replicationcontrollers",
            },
            CatalogEntry {
                group: "batch",
                name: "jobs",
            },
            CatalogEntry {
                group: "batch",
                name: "cronjobs",
            },
        ],
    }),
    CatalogItem::Section(CatalogSection {
        name: "Config",
        entries: &[
            CatalogEntry {
                group: CORE,
                name: "configmaps",
            },
            CatalogEntry {
                group: CORE,
                name: "secrets",
            },
            CatalogEntry {
                group: CORE,
                name: "resourcequotas",
            },
            CatalogEntry {
                group: CORE,
                name: "limitranges",
            },
            CatalogEntry {
                group: "autoscaling",
                name: "horizontalpodautoscalers",
            },
            CatalogEntry {
                group: "policy",
                name: "poddisruptionbudgets",
            },
            CatalogEntry {
                group: "scheduling.k8s.io",
                name: "priorityclasses",
            },
            CatalogEntry {
                group: "node.k8s.io",
                name: "runtimeclasses",
            },
            CatalogEntry {
                group: "coordination.k8s.io",
                name: "leases",
            },
        ],
    }),
    CatalogItem::Section(CatalogSection {
        name: "Networking",
        entries: &[
            CatalogEntry {
                group: CORE,
                name: "services",
            },
            CatalogEntry {
                group: CORE,
                name: "endpoints",
            },
            CatalogEntry {
                group: "discovery.k8s.io",
                name: "endpointslices",
            },
            CatalogEntry {
                group: "networking.k8s.io",
                name: "ingresses",
            },
            CatalogEntry {
                group: "networking.k8s.io",
                name: "ingressclasses",
            },
            CatalogEntry {
                group: "networking.k8s.io",
                name: "networkpolicies",
            },
        ],
    }),
    CatalogItem::Section(CatalogSection {
        name: "Gateway API",
        entries: &[
            CatalogEntry {
                group: "gateway.networking.k8s.io",
                name: "gatewayclasses",
            },
            CatalogEntry {
                group: "gateway.networking.k8s.io",
                name: "gateways",
            },
            CatalogEntry {
                group: "gateway.networking.k8s.io",
                name: "httproutes",
            },
            CatalogEntry {
                group: "gateway.networking.k8s.io",
                name: "grpcroutes",
            },
            CatalogEntry {
                group: "gateway.networking.k8s.io",
                name: "tlsroutes",
            },
            CatalogEntry {
                group: "gateway.networking.k8s.io",
                name: "referencegrants",
            },
            CatalogEntry {
                group: "gateway.networking.k8s.io",
                name: "backendtlspolicies",
            },
        ],
    }),
    CatalogItem::Section(CatalogSection {
        name: "Storage",
        entries: &[
            CatalogEntry {
                group: CORE,
                name: "persistentvolumes",
            },
            CatalogEntry {
                group: CORE,
                name: "persistentvolumeclaims",
            },
            CatalogEntry {
                group: "storage.k8s.io",
                name: "storageclasses",
            },
        ],
    }),
    CatalogItem::Resource(CatalogEntry {
        group: CORE,
        name: "namespaces",
    }),
    CatalogItem::Resource(CatalogEntry {
        group: CORE,
        name: "events",
    }),
    CatalogItem::Section(CatalogSection {
        name: "Security & Access Control",
        entries: &[
            CatalogEntry {
                group: CORE,
                name: "serviceaccounts",
            },
            CatalogEntry {
                group: "rbac.authorization.k8s.io",
                name: "roles",
            },
            CatalogEntry {
                group: "rbac.authorization.k8s.io",
                name: "rolebindings",
            },
            CatalogEntry {
                group: "rbac.authorization.k8s.io",
                name: "clusterroles",
            },
            CatalogEntry {
                group: "rbac.authorization.k8s.io",
                name: "clusterrolebindings",
            },
        ],
    }),
];

pub(super) fn build_resource_navigation(api_resources: Vec<ApiResource>) -> ResourceNavigation {
    let mut discovered = BTreeMap::new();
    for resource in api_resources {
        let key = (
            canonical_group(&resource.group).to_owned(),
            resource.name.clone(),
        );
        discovered.entry(key).or_insert(resource);
    }

    let mut categorized = BTreeSet::new();
    let curated_entries: Vec<_> = CATALOG
        .iter()
        .filter_map(|item| match item {
            CatalogItem::Resource(entry) => {
                discovered_resource(entry, &discovered, &mut categorized)
                    .map(CuratedNavigationEntry::Resource)
            }
            CatalogItem::Section(section) => {
                let api_resources = section
                    .entries
                    .iter()
                    .filter_map(|entry| discovered_resource(entry, &discovered, &mut categorized))
                    .collect::<Vec<_>>();

                (!api_resources.is_empty()).then_some(CuratedNavigationEntry::Section(
                    CuratedResourceSection {
                        name: section.name,
                        api_resources,
                    },
                ))
            }
        })
        .collect();

    let mut other_api_groups: BTreeMap<String, Vec<ApiResource>> = BTreeMap::new();
    for ((group, name), resource) in discovered {
        if !categorized.contains(&(group.clone(), name)) {
            other_api_groups.entry(group).or_default().push(resource);
        }
    }
    for resources in other_api_groups.values_mut() {
        resources.sort_by(|left, right| left.name.cmp(&right.name));
    }

    let mut curated_entries = curated_entries;
    curated_entries.push(CuratedNavigationEntry::Section(CuratedResourceSection {
        name: "Helm",
        api_resources: vec![ApiResource::helm_releases()],
    }));

    ResourceNavigation {
        curated_entries,
        other_api_groups,
    }
}

fn discovered_resource(
    entry: &CatalogEntry,
    discovered: &BTreeMap<(String, String), ApiResource>,
    categorized: &mut BTreeSet<(String, String)>,
) -> Option<ApiResource> {
    let key = (entry.group.to_owned(), entry.name.to_owned());
    discovered.get(&key).cloned().inspect(|_| {
        categorized.insert(key);
    })
}

fn canonical_group(group: &str) -> &str {
    if group.is_empty() { CORE } else { group }
}

#[cfg(test)]
mod tests;
