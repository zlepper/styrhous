use super::*;
use crate::resource_table::MEMORY_COLUMN;

#[test]
fn pod_metric_results_invalidate_cpu_sort_and_update_cells() {
    use crate::worker::WorkerResult as _;

    let pod = api_resource("Pod", "pods", true);
    let mut cluster =
        cluster_with_resources(&pod, [table_resource("api"), table_resource("worker")]);
    cluster.active_pod_metrics.insert("default".to_owned());
    let mut ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        selected_cluster: Some(1),
        ..Default::default()
    };
    crate::worker::PodMetricsUpdated {
        cluster_key: 1,
        namespace: "default".to_owned(),
        usages: BTreeMap::from([
            ("api".to_owned(), pod_usage(20)),
            ("worker".to_owned(), pod_usage(10)),
        ]),
    }
    .apply(&mut ui_state, &mut Vec::new());

    let mut configuration = resource_table_configuration(
        1_280.0,
        &pod,
        &[],
        false,
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    configuration.layout.sort = Some(components::SortState::new(
        CPU_COLUMN,
        components::SortDirection::Ascending,
    ));
    let search = ResourceSearchState::default();
    let mut cache = ResourceTableCache::default();
    assert_eq!(
        prepared_names(
            &mut cache,
            &ui_state.clusters[&1],
            &pod,
            &search,
            &configuration,
        ),
        ["worker", "api"]
    );
    let first_generation = cache.generation();

    crate::worker::PodMetricsUpdated {
        cluster_key: 1,
        namespace: "default".to_owned(),
        usages: BTreeMap::from([
            ("api".to_owned(), pod_usage(5)),
            ("worker".to_owned(), pod_usage(30)),
        ]),
    }
    .apply(&mut ui_state, &mut Vec::new());
    let cluster = &ui_state.clusters[&1];
    assert_eq!(
        prepared_names(&mut cache, cluster, &pod, &search, &configuration),
        ["api", "worker"]
    );
    assert!(cache.generation() > first_generation);
    let prepared = cache.prepared();
    let first_identity = match &prepared.rows[0] {
        PreparedResourceTableRow::Resource(identity) => identity,
        PreparedResourceTableRow::HiddenBySearch(_) => panic!("first row is a resource"),
    };
    let first_resource =
        resolve_prepared_resource(&cluster.resource_cache, prepared, first_identity)
            .expect("prepared row resolves");
    assert_eq!(
        resolved_resource_cell(
            first_resource,
            CPU_COLUMN,
            ResourceTableData::of(cluster, &pod).metrics,
            &pod
        ),
        Some(CellValue::Usage {
            label: format_cpu(5),
            value: 5,
            permille: None,
        })
    );

    crate::worker::PodMetricsWatchFailed {
        cluster_key: 1,
        namespace: "default".to_owned(),
        error: "metrics watch failed".to_owned(),
    }
    .apply(&mut ui_state, &mut Vec::new());
    let failed_cluster = &ui_state.clusters[&1];
    assert_eq!(
        prepared_names(&mut cache, failed_cluster, &pod, &search, &configuration,),
        ["api", "worker"]
    );
    let prepared = cache.prepared();
    let first_identity = match &prepared.rows[0] {
        PreparedResourceTableRow::Resource(identity) => identity,
        PreparedResourceTableRow::HiddenBySearch(_) => panic!("first row is a resource"),
    };
    let first_resource =
        resolve_prepared_resource(&failed_cluster.resource_cache, prepared, first_identity)
            .expect("prepared row resolves");
    assert_eq!(
        resolved_resource_cell(
            first_resource,
            CPU_COLUMN,
            ResourceTableData::of(failed_cluster, &pod).metrics,
            &pod,
        ),
        Some(CellValue::Unavailable)
    );
}

#[test]
fn node_metric_results_and_api_unavailable_refresh_cpu_sort_and_cells() {
    use crate::worker::WorkerResult as _;

    let node = api_resource("Node", "nodes", false);
    let resources = [table_resource("node-a"), table_resource("node-b")];
    let mut cluster = ClusterState::for_test(1, "test");
    cluster.resource_cache.insert(
        (node.clone(), None),
        ResourceWatchState {
            resources: resources
                .into_iter()
                .map(|resource| (resource.uid.clone(), resource))
                .collect(),
            is_synced: true,
            revision: 1,
            ..Default::default()
        },
    );
    cluster.node_metrics_active = true;
    let mut ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        selected_cluster: Some(1),
        ..Default::default()
    };
    let node_usage = |cpu_nanocores| crate::pod_metrics::NodeUsage {
        timestamp: time::OffsetDateTime::UNIX_EPOCH,
        cpu_nanocores,
        memory_bytes: cpu_nanocores,
    };
    crate::worker::NodeMetricsUpdated {
        cluster_key: 1,
        usages: BTreeMap::from([
            ("node-a".to_owned(), node_usage(20)),
            ("node-b".to_owned(), node_usage(10)),
        ]),
    }
    .apply(&mut ui_state, &mut Vec::new());

    let mut configuration = resource_table_configuration(
        1_280.0,
        &node,
        &[],
        false,
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    configuration.layout.sort = Some(components::SortState::new(
        CPU_COLUMN,
        components::SortDirection::Ascending,
    ));
    let search = ResourceSearchState::default();
    let mut cache = ResourceTableCache::default();
    assert_eq!(
        prepared_names(
            &mut cache,
            &ui_state.clusters[&1],
            &node,
            &search,
            &configuration,
        ),
        ["node-b", "node-a"]
    );
    let first_generation = cache.generation();

    crate::worker::NodeMetricsUpdated {
        cluster_key: 1,
        usages: BTreeMap::from([
            ("node-a".to_owned(), node_usage(5)),
            ("node-b".to_owned(), node_usage(30)),
        ]),
    }
    .apply(&mut ui_state, &mut Vec::new());
    assert_eq!(
        prepared_names(
            &mut cache,
            &ui_state.clusters[&1],
            &node,
            &search,
            &configuration,
        ),
        ["node-a", "node-b"]
    );
    assert!(cache.generation() > first_generation);
    let updated_generation = cache.generation();

    crate::worker::NodeMetricsApiUnavailable { cluster_key: 1 }
        .apply(&mut ui_state, &mut Vec::new());
    let unavailable_cluster = &ui_state.clusters[&1];
    assert_eq!(
        prepared_names(
            &mut cache,
            unavailable_cluster,
            &node,
            &search,
            &configuration,
        ),
        ["node-a", "node-b"]
    );
    assert!(cache.generation() > updated_generation);
    let prepared = cache.prepared();
    let first_identity = match &prepared.rows[0] {
        PreparedResourceTableRow::Resource(identity) => identity,
        PreparedResourceTableRow::HiddenBySearch(_) => panic!("first row is a resource"),
    };
    let first_resource = resolve_prepared_resource(
        &unavailable_cluster.resource_cache,
        prepared,
        first_identity,
    )
    .expect("prepared row resolves");
    assert_eq!(
        resolved_resource_cell(
            first_resource,
            CPU_COLUMN,
            ResourceTableData::of(unavailable_cluster, &node).metrics,
            &node,
        ),
        Some(CellValue::Unavailable)
    );
}

#[test]
fn unavailable_usage_sorts_after_available_usage_in_both_directions() {
    use crate::worker::WorkerResult as _;

    let pod = api_resource("Pod", "pods", true);
    let pod_in = |name: &str, namespace: &str| MinimalResource {
        namespace: Some(namespace.to_owned()),
        ..table_resource(name)
    };
    let mut cluster = cluster_with_resources(
        &pod,
        [
            pod_in("broken-a", "broken"),
            pod_in("heavy", "default"),
            pod_in("light", "default"),
            pod_in("broken-b", "broken"),
            pod_in("medium", "default"),
        ],
    );
    cluster.active_pod_metrics.insert("default".to_owned());
    cluster.active_pod_metrics.insert("broken".to_owned());
    let mut ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        selected_cluster: Some(1),
        ..Default::default()
    };
    crate::worker::PodMetricsUpdated {
        cluster_key: 1,
        namespace: "default".to_owned(),
        usages: BTreeMap::from([
            ("heavy".to_owned(), pod_usage(30)),
            ("light".to_owned(), pod_usage(10)),
            ("medium".to_owned(), pod_usage(20)),
        ]),
    }
    .apply(&mut ui_state, &mut Vec::new());
    crate::worker::PodMetricsWatchFailed {
        cluster_key: 1,
        namespace: "broken".to_owned(),
        error: "metrics watch failed".to_owned(),
    }
    .apply(&mut ui_state, &mut Vec::new());

    let search = ResourceSearchState::default();
    for column in [CPU_COLUMN, MEMORY_COLUMN] {
        for (direction, expected) in [
            (
                components::SortDirection::Ascending,
                ["light", "medium", "heavy"],
            ),
            (
                components::SortDirection::Descending,
                ["heavy", "medium", "light"],
            ),
        ] {
            let mut configuration = resource_table_configuration(
                1_280.0,
                &pod,
                &[],
                false,
                false,
                &mut PersistedResourceTablePreferences::default(),
            );
            configuration.grouping = Grouping::None;
            configuration.layout.sort = Some(components::SortState::new(column, direction));
            let names = prepared_names(
                &mut ResourceTableCache::default(),
                &ui_state.clusters[&1],
                &pod,
                &search,
                &configuration,
            );

            assert_eq!(names[..3], expected, "{column} {direction:?}");
            let mut unavailable = names[3..].to_vec();
            unavailable.sort();
            assert_eq!(unavailable, ["broken-a", "broken-b"]);
        }
    }
}

/// A Node's CPU and memory read "used / allocatable" once its allocatable
/// resources are known, and its Pods column follows the Pod count watch,
/// re-sorting when the counts change.
#[test]
fn node_cells_show_usage_of_allocatable_and_follow_pod_counts() {
    use crate::resource_table::PODS_COLUMN;
    use crate::worker::WorkerResult as _;
    use k8s_openapi::api::core::v1::{Node, NodeStatus};
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

    let node_resource = api_resource("Node", "nodes", false);
    let node = |name: &str| {
        crate::resource_handlers::node::extract(&Node {
            metadata: kube::api::ObjectMeta {
                name: Some(name.to_owned()),
                uid: Some(format!("{name}-uid")),
                ..Default::default()
            },
            spec: None,
            status: Some(NodeStatus {
                allocatable: Some(BTreeMap::from([
                    ("cpu".to_owned(), Quantity("4".to_owned())),
                    ("memory".to_owned(), Quantity("7987Mi".to_owned())),
                ])),
                ..Default::default()
            }),
        })
    };
    let mut cluster = ClusterState::for_test(1, "test");
    cluster.resource_cache.insert(
        (node_resource.clone(), None),
        ResourceWatchState {
            resources: [node("node-a"), node("node-b")]
                .into_iter()
                .map(|resource| (resource.uid.clone(), resource))
                .collect(),
            is_synced: true,
            revision: 1,
            ..Default::default()
        },
    );
    cluster.node_metrics_active = true;
    cluster.node_pod_counts_active = true;
    let mut ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        selected_cluster: Some(1),
        ..Default::default()
    };
    crate::worker::NodeMetricsUpdated {
        cluster_key: 1,
        usages: BTreeMap::from([(
            "node-a".to_owned(),
            crate::pod_metrics::NodeUsage {
                timestamp: time::OffsetDateTime::UNIX_EPOCH,
                cpu_nanocores: 820_000_000,
                memory_bytes: 2_254_857_830,
            },
        )]),
    }
    .apply(&mut ui_state, &mut Vec::new());

    let cell = |ui_state: &UiState, name: &str, column: &str| {
        let cluster = &ui_state.clusters[&1];
        let resource = cluster.resource_cache[&(node_resource.clone(), None)]
            .resources
            .get(&format!("{name}-uid"))
            .expect("the Node is listed");
        resolved_resource_cell(
            resource,
            column,
            ResourceTableData::of(cluster, &node_resource).metrics,
            &node_resource,
        )
    };
    assert_eq!(
        cell(&ui_state, "node-a", CPU_COLUMN),
        Some(CellValue::Usage {
            label: "0.82 / 4".to_owned(),
            value: 820_000_000,
            permille: Some(205),
        })
    );
    assert_eq!(
        cell(&ui_state, "node-a", MEMORY_COLUMN),
        Some(CellValue::Usage {
            label: "2.1 / 7.8Gi".to_owned(),
            value: 2_254_857_830,
            permille: Some(269),
        })
    );
    assert_eq!(
        cell(&ui_state, "node-b", CPU_COLUMN),
        None,
        "a Node without a metrics sample shows the missing-metrics dash"
    );
    assert_eq!(
        cell(&ui_state, "node-a", PODS_COLUMN),
        Some(CellValue::Empty),
        "the count is unknown until the first list completes"
    );

    let mut configuration = resource_table_configuration(
        1_280.0,
        &node_resource,
        &[],
        false,
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    configuration.layout.sort = Some(components::SortState::new(
        PODS_COLUMN,
        components::SortDirection::Ascending,
    ));
    let search = ResourceSearchState::default();
    let mut cache = ResourceTableCache::default();
    for (counts, expected) in [
        ([("node-a", 14), ("node-b", 3)], ["node-b", "node-a"]),
        ([("node-a", 2), ("node-b", 31)], ["node-a", "node-b"]),
    ] {
        crate::worker::NodePodCountsUpdated {
            cluster_key: 1,
            generation: ui_state.clusters[&1].node_pod_counts.generation,
            counts: counts
                .into_iter()
                .map(|(name, count)| (name.to_owned(), count))
                .collect(),
        }
        .apply(&mut ui_state, &mut Vec::new());
        assert_eq!(
            prepared_names(
                &mut cache,
                &ui_state.clusters[&1],
                &node_resource,
                &search,
                &configuration,
            ),
            expected
        );
    }
    assert_eq!(
        cell(&ui_state, "node-b", PODS_COLUMN),
        Some(CellValue::Number(31))
    );

    crate::worker::NodePodCountsWatchFailed {
        cluster_key: 1,
        generation: ui_state.clusters[&1].node_pod_counts.generation,
        error: "pods is forbidden".to_owned(),
    }
    .apply(&mut ui_state, &mut Vec::new());
    assert_eq!(
        cell(&ui_state, "node-b", PODS_COLUMN),
        Some(CellValue::Unavailable)
    );
}
