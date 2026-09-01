use super::*;

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
        &mut PersistedResourceTablePreferences::default(),
    );
    configuration.sort_state = Some(components::SortState::new(
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
            table_data(cluster).metrics,
            &pod
        ),
        Some(CellValue::Usage {
            label: format_cpu_cores(5),
            value: 5,
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
            table_data(failed_cluster).metrics,
            &pod,
        ),
        Some(CellValue::Text("Unavailable".to_owned()))
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
        &mut PersistedResourceTablePreferences::default(),
    );
    configuration.sort_state = Some(components::SortState::new(
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
            table_data(unavailable_cluster).metrics,
            &node,
        ),
        Some(CellValue::Text("Unavailable".to_owned()))
    );
}
