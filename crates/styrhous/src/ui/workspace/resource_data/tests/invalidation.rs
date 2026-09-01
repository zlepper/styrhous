use super::*;

#[test]
fn replacing_watch_sources_clears_prepared_rows_before_revisions_restart() {
    let deployment = crate::api_resource::ApiResource {
        group: "apps".to_owned(),
        version: "v1".to_owned(),
        kind: "Deployment".to_owned(),
        name: "deployments".to_owned(),
        namespaced: true,
    };
    let mut cluster = cluster_with_resources(&deployment, [table_resource("old")]);
    cluster.active_watchers.insert((deployment.clone(), None));
    let configuration = resource_table_configuration(
        1_280.0,
        &deployment,
        &[],
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    {
        let selected_namespaces = &cluster.selected_namespaces;
        let resources = &mut cluster.resources;
        prepare_resource_table(
            &mut resources.resource_table_cache,
            ResourceTableData {
                selected_namespaces,
                resource_cache: &resources.resource_cache,
                metrics: ResourceMetrics {
                    pod_metrics_api_available: resources.pod_metrics_api_available,
                    pod_metrics: &resources.pod_metrics,
                    node_metrics_api_available: resources.node_metrics_api_available,
                    node_metrics: &resources.node_metrics,
                },
            },
            &deployment,
            &ResourceSearchState::default(),
            &configuration,
        );
    }
    assert!(cluster.resource_table_cache.generation() > 0);

    UiState::request_selected_resource_watches(&mut cluster, &deployment, &mut Vec::new());

    assert_eq!(cluster.resource_table_cache.generation(), 0);
}

#[test]
fn resource_results_refresh_cached_rows_for_add_delete_and_replace() {
    use crate::worker::WorkerResult as _;

    let deployment = crate::api_resource::ApiResource {
        group: "apps".to_owned(),
        version: "v1".to_owned(),
        kind: "Deployment".to_owned(),
        name: "deployments".to_owned(),
        namespaced: true,
    };
    let cluster = cluster_with_resources(&deployment, [table_resource("old")]);
    let mut ui_state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        selected_cluster: Some(1),
        ..Default::default()
    };
    let configuration = resource_table_configuration(
        1_280.0,
        &deployment,
        &[],
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    let search = ResourceSearchState::default();
    let mut cache = ResourceTableCache::default();
    assert_eq!(
        prepared_names(
            &mut cache,
            &ui_state.clusters[&1],
            &deployment,
            &search,
            &configuration,
        ),
        ["old"]
    );

    crate::worker::KubernetesResourceAdded {
        cluster_key: 1,
        api_resource: deployment.clone(),
        namespace: Some("default".to_owned()),
        resource: table_resource("new"),
    }
    .apply(&mut ui_state, &mut Vec::new());
    assert_eq!(
        prepared_names(
            &mut cache,
            &ui_state.clusters[&1],
            &deployment,
            &search,
            &configuration,
        ),
        ["new", "old"]
    );

    crate::worker::KubernetesResourceDeleted {
        cluster_key: 1,
        api_resource: deployment.clone(),
        namespace: Some("default".to_owned()),
        resource_uid: "uid-old".to_owned(),
    }
    .apply(&mut ui_state, &mut Vec::new());
    assert_eq!(
        prepared_names(
            &mut cache,
            &ui_state.clusters[&1],
            &deployment,
            &search,
            &configuration,
        ),
        ["new"]
    );

    crate::worker::KubernetesResourcesReplaced {
        cluster_key: 1,
        api_resource: deployment.clone(),
        namespace: Some("default".to_owned()),
        resources: vec![table_resource("replacement")],
    }
    .apply(&mut ui_state, &mut Vec::new());
    assert_eq!(
        prepared_names(
            &mut cache,
            &ui_state.clusters[&1],
            &deployment,
            &search,
            &configuration,
        ),
        ["replacement"]
    );
}

#[test]
fn metric_revisions_only_invalidate_tables_that_render_them() {
    let mut cluster = ClusterState::for_test(1, "test");
    cluster.selected_namespaces.insert("default".to_owned());
    cluster.pod_metrics.insert(
        "default".to_owned(),
        PodMetricsNamespaceState {
            revision: 1,
            ..Default::default()
        },
    );
    cluster.node_metrics.revision = 1;
    let search = ResourceSearchState::default();

    let deployment = crate::api_resource::ApiResource {
        group: "apps".to_owned(),
        version: "v1".to_owned(),
        kind: "Deployment".to_owned(),
        name: "deployments".to_owned(),
        namespaced: true,
    };
    let deployment_configuration = resource_table_configuration(
        1_280.0,
        &deployment,
        &[],
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    let mut deployment_cache = ResourceTableCache::default();
    prepare_resource_table(
        &mut deployment_cache,
        table_data(&cluster),
        &deployment,
        &search,
        &deployment_configuration,
    );
    let deployment_generation = deployment_cache.generation();

    cluster
        .pod_metrics
        .get_mut("default")
        .expect("pod metrics exist")
        .revision += 1;
    cluster.node_metrics.revision += 1;
    prepare_resource_table(
        &mut deployment_cache,
        table_data(&cluster),
        &deployment,
        &search,
        &deployment_configuration,
    );
    assert_eq!(deployment_cache.generation(), deployment_generation);

    let pod = api_resource("Pod", "pods", true);
    let mut pod_configuration = resource_table_configuration(
        1_280.0,
        &pod,
        &[],
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    let mut unsorted_pod_cache = ResourceTableCache::default();
    prepare_resource_table(
        &mut unsorted_pod_cache,
        table_data(&cluster),
        &pod,
        &search,
        &pod_configuration,
    );
    let unsorted_pod_generation = unsorted_pod_cache.generation();

    cluster
        .pod_metrics
        .get_mut("default")
        .expect("pod metrics exist")
        .revision += 1;
    prepare_resource_table(
        &mut unsorted_pod_cache,
        table_data(&cluster),
        &pod,
        &search,
        &pod_configuration,
    );
    assert_eq!(unsorted_pod_cache.generation(), unsorted_pod_generation);

    pod_configuration.sort_state = Some(components::SortState::new(
        CPU_COLUMN,
        components::SortDirection::Ascending,
    ));
    let mut pod_cache = ResourceTableCache::default();
    prepare_resource_table(
        &mut pod_cache,
        table_data(&cluster),
        &pod,
        &search,
        &pod_configuration,
    );
    let pod_generation = pod_cache.generation();

    cluster
        .pod_metrics
        .get_mut("default")
        .expect("pod metrics exist")
        .revision += 1;
    prepare_resource_table(
        &mut pod_cache,
        table_data(&cluster),
        &pod,
        &search,
        &pod_configuration,
    );
    assert!(pod_cache.generation() > pod_generation);

    let node = api_resource("Node", "nodes", false);
    let mut node_configuration = resource_table_configuration(
        1_280.0,
        &node,
        &[],
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    node_configuration.sort_state = Some(components::SortState::new(
        CPU_COLUMN,
        components::SortDirection::Ascending,
    ));
    let mut node_cache = ResourceTableCache::default();
    prepare_resource_table(
        &mut node_cache,
        table_data(&cluster),
        &node,
        &search,
        &node_configuration,
    );
    let node_generation = node_cache.generation();

    cluster.node_metrics.revision += 1;
    prepare_resource_table(
        &mut node_cache,
        table_data(&cluster),
        &node,
        &search,
        &node_configuration,
    );
    assert!(node_cache.generation() > node_generation);
}
