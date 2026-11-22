use super::*;

fn resource(group: &str, name: &str) -> ApiResource {
    ApiResource {
        group: group.to_owned(),
        version: "v1".to_owned(),
        kind: name.to_owned(),
        name: name.to_owned(),
        namespaced: true,
    }
}

#[test]
fn orders_primary_resources_sections_and_fallback_groups() {
    let navigation = build_resource_navigation(vec![
        resource("", "nodes"),
        resource("", "pods"),
        resource("apps", "deployments"),
        resource("networking.k8s.io", "ingresses"),
        resource("", "namespaces"),
        resource("", "events"),
        resource("apps", "controllerrevisions"),
        resource("example.dev", "widgets"),
    ]);

    assert_eq!(
        navigation
            .curated_entries
            .iter()
            .map(entry_name)
            .collect::<Vec<_>>(),
        vec![
            "nodes",
            "Apps & Containers",
            "Networking",
            "namespaces",
            "events",
            "Helm",
        ]
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
fn classifies_every_catalog_entry_into_its_declared_navigation_entry() {
    let discovered = CATALOG
        .iter()
        .flat_map(|item| match item {
            CatalogItem::Resource(entry) => vec![resource(entry.group, entry.name)],
            CatalogItem::Section(section) => section
                .entries
                .iter()
                .map(|entry| resource(entry.group, entry.name))
                .collect(),
        })
        .collect();

    let navigation = build_resource_navigation(discovered);

    assert_eq!(navigation.curated_entries.len(), CATALOG.len() + 1);
    for (actual, expected) in navigation
        .curated_entries
        .iter()
        .take(CATALOG.len())
        .zip(CATALOG)
    {
        match (actual, expected) {
            (CuratedNavigationEntry::Resource(actual), CatalogItem::Resource(expected)) => {
                assert_eq!(actual.name, expected.name);
            }
            (CuratedNavigationEntry::Section(actual), CatalogItem::Section(expected)) => {
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
            _ => panic!("catalog entry type should match its navigation entry"),
        }
    }
    assert!(matches!(
        navigation.curated_entries.last(),
        Some(CuratedNavigationEntry::Section(section))
            if section.name == "Helm" && section.api_resources == vec![ApiResource::helm_releases()]
    ));
    assert!(navigation.other_api_groups.is_empty());
}

#[test]
fn omits_catalog_entries_that_the_cluster_does_not_advertise() {
    let navigation = build_resource_navigation(vec![resource("core", "pods")]);

    assert_eq!(navigation.curated_entries.len(), 2);
    assert_eq!(
        entry_name(&navigation.curated_entries[0]),
        "Apps & Containers"
    );
    assert_eq!(entry_name(&navigation.curated_entries[1]), "Helm");
    assert!(navigation.other_api_groups.is_empty());
}

#[test]
fn only_shows_gateway_api_when_a_supported_resource_is_discovered() {
    let navigation =
        build_resource_navigation(vec![resource("gateway.networking.k8s.io", "httproutes")]);

    assert_eq!(
        navigation
            .curated_entries
            .iter()
            .map(entry_name)
            .collect::<Vec<_>>(),
        vec!["Gateway API", "Helm"]
    );
    assert!(navigation.other_api_groups.is_empty());
}

#[test]
fn resolves_owner_resources_by_group_and_kind_across_api_versions() {
    let replica_set = ApiResource {
        group: "apps".into(),
        version: "v1".into(),
        kind: "ReplicaSet".into(),
        name: "replicasets".into(),
        namespaced: true,
    };
    let node = ApiResource {
        group: "core".into(),
        version: "v1".into(),
        kind: "Node".into(),
        name: "nodes".into(),
        namespaced: false,
    };
    let navigation = build_resource_navigation(vec![replica_set.clone(), node.clone()]);

    let replica_set_owner = ResourceOwner {
        api_version: "apps/v1beta1".into(),
        kind: "ReplicaSet".into(),
        name: "api-7b948f".into(),
        uid: "replicaset-uid".into(),
        controller: true,
    };
    let node_owner = ResourceOwner {
        api_version: "v1".into(),
        kind: "Node".into(),
        name: "kind-control-plane".into(),
        uid: "node-uid".into(),
        controller: false,
    };
    let unresolved_owner = ResourceOwner {
        api_version: "example.dev/v1".into(),
        kind: "Widget".into(),
        name: "api-widget".into(),
        uid: "widget-uid".into(),
        controller: false,
    };

    assert_eq!(
        navigation.api_resource_for_owner(&replica_set_owner),
        Some(replica_set)
    );
    assert_eq!(navigation.api_resource_for_owner(&node_owner), Some(node));
    assert_eq!(navigation.api_resource_for_owner(&unresolved_owner), None);
}

fn entry_name(entry: &CuratedNavigationEntry) -> &str {
    match entry {
        CuratedNavigationEntry::Resource(resource) => &resource.name,
        CuratedNavigationEntry::Section(section) => section.name,
    }
}

#[test]
fn the_builtin_baseline_covers_the_curated_built_in_resources_exactly() {
    let baseline = builtin_api_resources();
    let baseline_keys = baseline
        .api_resources
        .iter()
        .map(|resource| (resource.group.as_str(), resource.name.as_str()))
        .collect::<BTreeSet<_>>();
    let curated_built_ins = CATALOG
        .iter()
        .flat_map(|item| match item {
            CatalogItem::Resource(entry) => std::slice::from_ref(entry),
            CatalogItem::Section(section) => section.entries,
        })
        // Gateway API resources are CRDs that a cluster may not have installed.
        .filter(|entry| entry.group != "gateway.networking.k8s.io")
        .map(|entry| (entry.group, entry.name))
        .collect::<BTreeSet<_>>();

    assert_eq!(baseline_keys, curated_built_ins);
    let navigation = build_resource_navigation(baseline.api_resources.clone());
    assert!(
        navigation.other_api_groups.is_empty(),
        "every baseline resource has a curated place: {:?}",
        navigation.other_api_groups
    );
    let pods = baseline
        .api_resources
        .iter()
        .find(|resource| resource.name == "pods")
        .expect("pods are built in");
    assert_eq!(
        (
            pods.group.as_str(),
            pods.version.as_str(),
            pods.kind.as_str(),
            pods.namespaced
        ),
        ("core", "v1", "Pod", true)
    );
    let nodes = baseline
        .api_resources
        .iter()
        .find(|resource| resource.name == "nodes")
        .expect("nodes are built in");
    assert!(!nodes.namespaced);
    assert!(
        baseline
            .scalable_api_resources
            .iter()
            .any(|resource| resource.name == "deployments")
    );
}
