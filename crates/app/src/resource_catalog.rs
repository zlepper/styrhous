use crate::api_resource::ApiResource;
use std::collections::{BTreeMap, BTreeSet};

/// A fixed section in the primary resource navigator.
#[derive(Debug)]
pub(super) struct CuratedResourceSection {
    pub(super) name: &'static str,
    pub(super) api_resources: Vec<ApiResource>,
}

/// Resource navigation derived from Kubernetes discovery.
///
/// The section order and the resources assigned to it are intentionally static.
/// Discovery only decides which resources exist on the connected cluster and provides
/// the server-supported API version used by the watcher.
#[derive(Debug, Default)]
pub(super) struct ResourceNavigation {
    pub(super) curated_sections: Vec<CuratedResourceSection>,
    pub(super) other_api_groups: BTreeMap<String, Vec<ApiResource>>,
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

const CORE: &str = "core";

const CATALOG: &[CatalogSection] = &[
    CatalogSection {
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
    },
    CatalogSection {
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
                group: "networking.k8s.io",
                name: "ingresses",
            },
            CatalogEntry {
                group: "networking.k8s.io",
                name: "networkpolicies",
            },
        ],
    },
    CatalogSection {
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
        ],
    },
    CatalogSection {
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
    },
    CatalogSection {
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
    },
    CatalogSection {
        name: "Cluster",
        entries: &[
            CatalogEntry {
                group: CORE,
                name: "nodes",
            },
            CatalogEntry {
                group: CORE,
                name: "namespaces",
            },
            CatalogEntry {
                group: CORE,
                name: "events",
            },
        ],
    },
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
    let curated_sections = CATALOG
        .iter()
        .filter_map(|section| {
            let api_resources = section
                .entries
                .iter()
                .filter_map(|entry| {
                    let key = (entry.group.to_owned(), entry.name.to_owned());
                    discovered.get(&key).cloned().inspect(|_| {
                        categorized.insert(key);
                    })
                })
                .collect::<Vec<_>>();

            (!api_resources.is_empty()).then_some(CuratedResourceSection {
                name: section.name,
                api_resources,
            })
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

    ResourceNavigation {
        curated_sections,
        other_api_groups,
    }
}

fn canonical_group(group: &str) -> &str {
    if group.is_empty() { CORE } else { group }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(group: &str, name: &str) -> ApiResource {
        ApiResource {
            group: group.to_owned(),
            version: "v1".to_owned(),
            kind: name.to_owned(),
            name: name.to_owned(),
        }
    }

    #[test]
    fn builds_fixed_sections_and_nests_uncategorized_resources_by_api_group() {
        let navigation = build_resource_navigation(vec![
            resource("", "pods"),
            resource("apps", "deployments"),
            resource("networking.k8s.io", "ingresses"),
            resource("apps", "controllerrevisions"),
            resource("example.dev", "widgets"),
        ]);

        assert_eq!(
            navigation
                .curated_sections
                .iter()
                .map(|section| section.name)
                .collect::<Vec<_>>(),
            vec!["Apps & Containers", "Networking"]
        );
        assert_eq!(
            navigation.curated_sections[0]
                .api_resources
                .iter()
                .map(|resource| resource.name.as_str())
                .collect::<Vec<_>>(),
            vec!["pods", "deployments"]
        );
        assert_eq!(
            navigation.other_api_groups["apps"][0].name,
            "controllerrevisions"
        );
        assert_eq!(
            navigation.other_api_groups["example.dev"][0].name,
            "widgets"
        );
    }

    #[test]
    fn classifies_every_catalog_entry_into_its_declared_section() {
        let discovered = CATALOG
            .iter()
            .flat_map(|section| {
                section
                    .entries
                    .iter()
                    .map(|entry| resource(entry.group, entry.name))
            })
            .collect();

        let navigation = build_resource_navigation(discovered);

        assert_eq!(navigation.curated_sections.len(), CATALOG.len());
        for (actual, expected) in navigation.curated_sections.iter().zip(CATALOG) {
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.api_resources.len(), expected.entries.len());
            assert_eq!(
                actual
                    .api_resources
                    .iter()
                    .map(|resource| resource.name.as_str())
                    .collect::<Vec<_>>(),
                expected
                    .entries
                    .iter()
                    .map(|entry| entry.name)
                    .collect::<Vec<_>>(),
            );
        }
        assert!(navigation.other_api_groups.is_empty());
    }

    #[test]
    fn omits_catalog_entries_that_the_cluster_does_not_advertise() {
        let navigation = build_resource_navigation(vec![resource("core", "pods")]);

        assert_eq!(navigation.curated_sections.len(), 1);
        assert_eq!(navigation.curated_sections[0].name, "Apps & Containers");
        assert_eq!(navigation.curated_sections[0].api_resources[0].name, "pods");
        assert!(navigation.other_api_groups.is_empty());
    }
}
