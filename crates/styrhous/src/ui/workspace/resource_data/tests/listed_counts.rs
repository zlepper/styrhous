//! Object counts of every kind at once (the sidebar) agree with counting one kind.

use super::super::{ListedCounts, listed_count};
use super::*;
use crate::worker::WatchedNamespaces;

/// A watch entry of `count` objects, listed or still listing.
fn watch(count: usize, is_synced: bool) -> ResourceWatchState {
    ResourceWatchState {
        resources: (0..count)
            .map(|index| {
                (
                    format!("uid-{index}"),
                    table_resource(&format!("object-{index}")),
                )
            })
            .collect(),
        is_synced,
        ..Default::default()
    }
}

#[test]
fn every_kind_is_counted_as_its_own_listed_count() {
    let pods = api_resource("Pod", "pods", true);
    let services = api_resource("Service", "services", true);
    let secrets = api_resource("Secret", "secrets", true);
    let nodes = api_resource("Node", "nodes", false);
    let config_maps = api_resource("ConfigMap", "configmaps", true);

    let mut cluster = ClusterState::for_test(1, "test");
    cluster.selected_namespaces = SelectedNamespaces::from(["a", "b"]);
    // Pods across every namespace: every namespace it has rows for, once its own entry listed.
    cluster
        .all_namespaces_watch_scopes
        .insert(pods.clone(), WatchedNamespaces::Every);
    cluster
        .resource_cache
        .insert((pods.clone(), None), watch(0, true));
    for (namespace, count) in [("a", 2), ("b", 1), ("c", 4)] {
        cluster.resource_cache.insert(
            (pods.clone(), Some(namespace.to_owned())),
            watch(count, false),
        );
    }
    // Services in the selected namespaces; one namespace the selection leaves out.
    for (namespace, count) in [("a", 3), ("b", 5), ("z", 7)] {
        cluster.resource_cache.insert(
            (services.clone(), Some(namespace.to_owned())),
            watch(count, true),
        );
    }
    // Secrets: one selected namespace still listing.
    cluster
        .resource_cache
        .insert((secrets.clone(), Some("a".to_owned())), watch(1, true));
    cluster
        .resource_cache
        .insert((secrets.clone(), Some("b".to_owned())), watch(1, false));
    cluster
        .resource_cache
        .insert((nodes.clone(), None), watch(6, true));

    let counts = ListedCounts::new(&cluster);
    for (kind, expected) in [
        (&pods, Some(7)),
        (&services, Some(8)),
        (&secrets, None),
        (&nodes, Some(6)),
        // Never watched this session.
        (&config_maps, None),
    ] {
        assert_eq!(counts.get(kind), expected, "{}", kind.name);
        assert_eq!(listed_count(&cluster, kind), expected, "{}", kind.name);
    }

    // Until a watch across every namespace has listed, it has no count either.
    cluster
        .resource_cache
        .insert((pods.clone(), None), watch(0, false));
    assert_eq!(ListedCounts::new(&cluster).get(&pods), None);
    assert_eq!(listed_count(&cluster, &pods), None);

    // No namespace selected: nothing is listed, which is not a count of zero.
    cluster.selected_namespaces = SelectedNamespaces::from([] as [&str; 0]);
    assert_eq!(ListedCounts::new(&cluster).get(&services), None);
    assert_eq!(listed_count(&cluster, &services), None);
}

#[test]
fn kinds_still_waiting_on_their_watches_or_the_namespace_list_have_no_count() {
    use crate::ui::state::ClusterLoadState;

    let pods = api_resource("Pod", "pods", true);
    let nodes = api_resource("Node", "nodes", false);
    let services = api_resource("Service", "services", true);
    let mut cluster = ClusterState::for_test(1, "test");
    cluster.namespaces.insert(
        "a".into(),
        crate::minimal_namespace::MinimalNamespace::named("a"),
    );
    cluster.selected_namespaces = SelectedNamespaces::All;
    cluster
        .resource_cache
        .insert((services.clone(), Some("a".to_owned())), watch(3, true));
    // A cluster-scoped kind still listing.
    cluster
        .resource_cache
        .insert((nodes.clone(), None), watch(2, false));
    // A watch across every namespace that listed nothing yet in any namespace.
    cluster
        .all_namespaces_watch_scopes
        .insert(pods.clone(), WatchedNamespaces::Every);
    cluster
        .resource_cache
        .insert((pods.clone(), None), watch(0, true));

    let expect = |cluster: &ClusterState, kind, expected: Option<usize>| {
        assert_eq!(
            ListedCounts::new(cluster).get(kind),
            expected,
            "{}",
            kind.name
        );
        assert_eq!(listed_count(cluster, kind), expected, "{}", kind.name);
    };
    expect(&cluster, &nodes, None);
    expect(&cluster, &pods, None);

    // "All namespaces" waits for the namespace list, unless the kind is watched across every namespace.
    cluster.namespaces_load = ClusterLoadState::Loading;
    expect(&cluster, &services, None);
    cluster
        .resource_cache
        .insert((pods.clone(), Some("a".to_owned())), watch(0, false));
    expect(&cluster, &pods, Some(0));

    // A failed namespace list does not hold the counts of what is listed back.
    cluster.namespaces_load = ClusterLoadState::Failed("forbidden".to_owned());
    expect(&cluster, &services, Some(3));
    cluster.namespaces_load = ClusterLoadState::Ready;
    expect(&cluster, &services, Some(3));
}
