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
