use super::*;

pub(super) fn selected_watch_error(
    cluster: &super::super::state::ClusterState,
    api_resource: &crate::api_resource::ApiResource,
) -> Option<String> {
    resource_watch_namespaces(cluster, api_resource)
        .into_iter()
        .find_map(|namespace| {
            cluster
                .resource_cache
                .get(&(api_resource.clone(), namespace))
                .and_then(|watch| watch.error.clone())
        })
}

pub(super) fn selected_watches_are_loading(
    cluster: &super::super::state::ClusterState,
    api_resource: &crate::api_resource::ApiResource,
) -> bool {
    resource_watch_namespaces(cluster, api_resource)
        .into_iter()
        .any(|namespace| {
            cluster
                .resource_cache
                .get(&(api_resource.clone(), namespace))
                .is_none_or(|watch| !watch.is_synced)
        })
}

pub(super) fn selected_resources(
    cluster: &super::super::state::ClusterState,
    api_resource: Option<&crate::api_resource::ApiResource>,
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

pub(super) fn prepare_resource_table<'a>(
    cache: &'a mut ResourceTableCache,
    data: ResourceTableData<'_>,
    api_resource: &crate::api_resource::ApiResource,
    resource_search: &ResourceSearchState,
    configuration: &ResourceTableConfiguration,
) -> &'a PreparedResourceTable {
    let mut watch_keys = resource_watch_namespaces_for(data.selected_namespaces, api_resource)
        .into_iter()
        .map(|namespace| (api_resource.clone(), namespace))
        .collect::<Vec<_>>();
    watch_keys.sort();
    let watch_revisions = watch_keys
        .iter()
        .map(|key| {
            (
                key.clone(),
                data.resource_cache
                    .get(key)
                    .map_or(0, |watch| watch.revision),
            )
        })
        .collect::<Vec<_>>();
    let is_pod = api_resource.group == "core" && api_resource.kind == "Pod";
    let is_node = api_resource.group == "core" && api_resource.kind == "Node";
    let sort = configuration
        .sort_state
        .as_ref()
        .map(|sort| (sort.column_id.clone(), sort.direction));
    let is_metric_sort = sort
        .as_ref()
        .is_some_and(|(column, _)| column == CPU_COLUMN || column == MEMORY_COLUMN);
    let mut pod_metric_revisions = if is_pod && is_metric_sort {
        data.selected_namespaces
            .iter()
            .map(|namespace| {
                (
                    namespace.clone(),
                    data.metrics
                        .pod_metrics
                        .get(namespace)
                        .map_or(0, |metrics| metrics.revision),
                )
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    pod_metric_revisions.sort();
    let key = ResourceTableCacheKey {
        api_resource: api_resource.clone(),
        watch_revisions,
        pod_metric_revisions,
        node_metric_revision: if is_node && is_metric_sort {
            data.metrics.node_metrics.revision
        } else {
            0
        },
        pod_metrics_api_available: is_pod
            && is_metric_sort
            && data.metrics.pod_metrics_api_available,
        node_metrics_api_available: is_node
            && is_metric_sort
            && data.metrics.node_metrics_api_available,
        search_query: resource_search.query.clone(),
        regex_mode: resource_search.regex_mode,
        sort,
    };

    if !cache.matches(&key) {
        let generation = cache.generation().wrapping_add(1);
        cache.replace(build_prepared_resource_table(
            data,
            key,
            watch_keys,
            resource_search,
            configuration,
            generation,
        ));
    }
    cache.prepared()
}

struct ResourceCandidate {
    identity: PreparedResourceIdentity,
    name: String,
    default_name: String,
    sort_value: Option<SortValue>,
    fuzzy_score: Option<FuzzyMatchScore>,
}

fn build_prepared_resource_table(
    data: ResourceTableData<'_>,
    key: ResourceTableCacheKey,
    watch_keys: Vec<super::super::state::ResourceWatchKey>,
    resource_search: &ResourceSearchState,
    configuration: &ResourceTableConfiguration,
    generation: u64,
) -> PreparedResourceTable {
    let metrics = data.metrics;
    let mut labels = BTreeSet::new();
    let mut annotations = BTreeSet::new();
    let mut candidates = Vec::new();
    for (watch_index, watch_key) in watch_keys.iter().enumerate() {
        let Some(watch) = data.resource_cache.get(watch_key) else {
            continue;
        };
        for resource in watch.resources.values() {
            labels.extend(resource.labels.keys().cloned());
            annotations.extend(resource.annotations.keys().cloned());
            candidates.push(ResourceCandidate {
                identity: PreparedResourceIdentity {
                    watch_index,
                    uid: resource.uid.clone(),
                },
                name: resource.name.clone(),
                default_name: resource.name.to_lowercase(),
                sort_value: configuration.sort_state.as_ref().map(|sort| {
                    resource_column_sort_value(
                        resource,
                        &sort.column_id,
                        &configuration.metadata_columns,
                        metrics,
                        &key.api_resource,
                    )
                }),
                fuzzy_score: None,
            });
        }
    }
    candidates.sort_by(|left, right| left.default_name.cmp(&right.default_name));
    let resource_count = candidates.len();

    let regex_error = if resource_search.query.is_empty() {
        None
    } else if resource_search.regex_mode {
        match regex::RegexBuilder::new(&resource_search.query)
            .case_insensitive(true)
            .build()
        {
            Ok(regex) => {
                candidates.retain(|candidate| {
                    let normalized_name: String = normalize_for_search(&candidate.name).collect();
                    regex.is_match(&normalized_name)
                });
                None
            }
            Err(error) => {
                candidates.clear();
                Some(error.to_string())
            }
        }
    } else {
        let query = normalize_for_search(&resource_search.query).collect::<Vec<_>>();
        if !query.is_empty() {
            candidates.retain_mut(|candidate| {
                candidate.fuzzy_score = fuzzy_match_score(&candidate.name, &query);
                candidate.fuzzy_score.is_some()
            });
            if configuration.sort_state.is_none() {
                candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.fuzzy_score));
            }
        }
        None
    };

    if let Some(sort) = &configuration.sort_state {
        candidates.sort_by(|left, right| {
            crate::resource_table::compare_sort_value_refs(
                left.sort_value
                    .as_ref()
                    .expect("sort value exists when sorting"),
                right
                    .sort_value
                    .as_ref()
                    .expect("sort value exists when sorting"),
                sort.direction,
            )
            .then_with(|| right.fuzzy_score.cmp(&left.fuzzy_score))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.identity.uid.cmp(&right.identity.uid))
        });
    }

    let visible_resource_count = candidates.len();
    let mut rows = candidates
        .into_iter()
        .map(|candidate| PreparedResourceTableRow::Resource(candidate.identity))
        .collect::<Vec<_>>();
    if resource_count > visible_resource_count && regex_error.is_none() {
        rows.push(PreparedResourceTableRow::HiddenBySearch(
            resource_count - visible_resource_count,
        ));
    }

    PreparedResourceTable {
        key,
        watch_keys,
        rows,
        resource_count,
        visible_resource_count,
        regex_error,
        metadata_key_suggestions: super::super::metadata_fields::MetadataKeySuggestions {
            labels: labels.into_iter().collect(),
            annotations: annotations.into_iter().collect(),
        },
        generation,
    }
}

pub(super) fn resolve_prepared_resource<'a>(
    resource_cache: &'a HashMap<
        super::super::state::ResourceWatchKey,
        super::super::state::ResourceWatchState,
    >,
    prepared: &PreparedResourceTable,
    identity: &PreparedResourceIdentity,
) -> Option<&'a MinimalResource> {
    let watch_key = prepared.watch_keys.get(identity.watch_index)?;
    resource_cache.get(watch_key)?.resources.get(&identity.uid)
}

pub(super) fn resolved_resource_cell(
    resource: &MinimalResource,
    column_id: &str,
    metrics: ResourceMetrics<'_>,
    api_resource: &crate::api_resource::ApiResource,
) -> Option<CellValue> {
    if column_id != CPU_COLUMN && column_id != MEMORY_COLUMN {
        return None;
    }
    let is_cpu = column_id == CPU_COLUMN;
    if api_resource.group == "core" && api_resource.kind == "Node" {
        if !metrics.node_metrics_api_available || metrics.node_metrics.error.is_some() {
            return Some(CellValue::Text("Unavailable".into()));
        }
        return metrics
            .node_metrics
            .usages
            .get(&resource.name)
            .map(|usage| CellValue::Usage {
                label: if is_cpu {
                    format_cpu_cores(usage.cpu_nanocores)
                } else {
                    format_memory(usage.memory_bytes)
                },
                value: if is_cpu {
                    usage.cpu_nanocores
                } else {
                    usage.memory_bytes
                },
            });
    }
    if api_resource.group != "core" || api_resource.kind != "Pod" {
        return None;
    }
    let namespace = resource.namespace.as_deref()?;
    let namespace_metrics = metrics.pod_metrics.get(namespace);
    if !metrics.pod_metrics_api_available
        || namespace_metrics.is_some_and(|metrics| metrics.error.is_some())
    {
        return Some(CellValue::Text("Unavailable".into()));
    }
    namespace_metrics
        .and_then(|metrics| metrics.usages.get(&resource.name))
        .map(|usage| CellValue::Usage {
            label: if is_cpu {
                format_cpu_cores(usage.cpu_nanocores)
            } else {
                format_memory(usage.memory_bytes)
            },
            value: if is_cpu {
                usage.cpu_nanocores
            } else {
                usage.memory_bytes
            },
        })
}

pub(super) fn resource_watch_namespaces(
    cluster: &super::super::state::ClusterState,
    api_resource: &crate::api_resource::ApiResource,
) -> Vec<Option<String>> {
    resource_watch_namespaces_for(&cluster.selected_namespaces, api_resource)
}

fn resource_watch_namespaces_for(
    selected_namespaces: &HashSet<String>,
    api_resource: &crate::api_resource::ApiResource,
) -> Vec<Option<String>> {
    if api_resource.namespaced {
        selected_namespaces.iter().cloned().map(Some).collect()
    } else {
        vec![None]
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::state::{ClusterState, PodMetricsNamespaceState, ResourceWatchState};
    use super::super::resource_table::resource_table_configuration;
    use super::*;
    use std::collections::BTreeMap;

    fn api_resource(kind: &str, name: &str, namespaced: bool) -> crate::api_resource::ApiResource {
        crate::api_resource::ApiResource {
            group: "core".to_owned(),
            version: "v1".to_owned(),
            kind: kind.to_owned(),
            name: name.to_owned(),
            namespaced,
        }
    }

    fn table_data(cluster: &ClusterState) -> ResourceTableData<'_> {
        ResourceTableData {
            selected_namespaces: &cluster.selected_namespaces,
            resource_cache: &cluster.resource_cache,
            metrics: ResourceMetrics {
                pod_metrics_api_available: cluster.pod_metrics_api_available,
                pod_metrics: &cluster.pod_metrics,
                node_metrics_api_available: cluster.node_metrics_api_available,
                node_metrics: &cluster.node_metrics,
            },
        }
    }

    fn table_resource(name: &str) -> MinimalResource {
        MinimalResource {
            uid: format!("uid-{name}"),
            name: name.to_owned(),
            namespace: Some("default".to_owned()),
            creation_timestamp: None,
            controller_owner: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            cells: BTreeMap::new(),
            log_containers: Vec::new(),
        }
    }

    fn cluster_with_resources(
        api_resource: &crate::api_resource::ApiResource,
        resources: impl IntoIterator<Item = MinimalResource>,
    ) -> ClusterState {
        let mut cluster = ClusterState::for_test(1, "test");
        cluster.selected_namespaces.insert("default".to_owned());
        cluster.resource_cache.insert(
            (api_resource.clone(), Some("default".to_owned())),
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
        cluster
    }

    fn prepared_names(
        cache: &mut ResourceTableCache,
        cluster: &ClusterState,
        api_resource: &crate::api_resource::ApiResource,
        search: &ResourceSearchState,
        configuration: &ResourceTableConfiguration,
    ) -> Vec<String> {
        let prepared = prepare_resource_table(
            cache,
            table_data(cluster),
            api_resource,
            search,
            configuration,
        );
        prepared
            .rows
            .iter()
            .filter_map(|row| match row {
                PreparedResourceTableRow::Resource(identity) => {
                    resolve_prepared_resource(&cluster.resource_cache, prepared, identity)
                        .map(|resource| resource.name.clone())
                }
                PreparedResourceTableRow::HiddenBySearch(_) => None,
            })
            .collect()
    }

    fn pod_usage(cpu_nanocores: i64) -> crate::pod_metrics::PodUsage {
        crate::pod_metrics::PodUsage {
            timestamp: time::OffsetDateTime::UNIX_EPOCH,
            cpu_nanocores,
            memory_bytes: cpu_nanocores,
            containers: BTreeMap::new(),
        }
    }

    #[test]
    fn prepared_table_uses_production_filter_sort_and_metadata_paths() {
        let deployment = crate::api_resource::ApiResource {
            group: "apps".to_owned(),
            version: "v1".to_owned(),
            kind: "Deployment".to_owned(),
            name: "deployments".to_owned(),
            namespaced: true,
        };
        let mut resources = ["my-api", "a-p-i", "api-server", "api", "worker"].map(table_resource);
        for (resource, rank) in resources.iter_mut().zip(["c", "a", "b", "d", "e"]) {
            resource.labels.insert("rank".to_owned(), rank.to_owned());
        }
        resources[0]
            .annotations
            .insert("example.com/team".to_owned(), "platform".to_owned());
        let cluster = cluster_with_resources(&deployment, resources);
        let mut configuration = resource_table_configuration(
            1_280.0,
            &deployment,
            &[],
            false,
            &mut PersistedResourceTablePreferences::default(),
        );
        let mut cache = ResourceTableCache::default();

        assert_eq!(
            prepared_names(
                &mut cache,
                &cluster,
                &deployment,
                &ResourceSearchState::default(),
                &configuration,
            ),
            ["a-p-i", "api", "api-server", "my-api", "worker"]
        );

        let fuzzy_search = ResourceSearchState {
            query: "api".to_owned(),
            regex_mode: false,
        };
        assert_eq!(
            prepared_names(
                &mut cache,
                &cluster,
                &deployment,
                &fuzzy_search,
                &configuration,
            ),
            ["api", "api-server", "my-api", "a-p-i"]
        );
        let prepared = cache.prepared();
        assert_eq!(prepared.resource_count, 5);
        assert_eq!(prepared.visible_resource_count, 4);
        assert!(matches!(
            prepared.rows.last(),
            Some(PreparedResourceTableRow::HiddenBySearch(1))
        ));
        assert_eq!(prepared.metadata_key_suggestions.labels, ["rank"]);
        assert_eq!(
            prepared.metadata_key_suggestions.annotations,
            ["example.com/team"]
        );

        configuration.sort_state = Some(components::SortState::new(
            "owner",
            components::SortDirection::Ascending,
        ));
        assert_eq!(
            prepared_names(
                &mut cache,
                &cluster,
                &deployment,
                &fuzzy_search,
                &configuration,
            ),
            ["api", "api-server", "my-api", "a-p-i"]
        );

        let metadata_column = super::super::super::table_preferences::CustomMetadataColumn {
            source: MetadataColumnSource::Label,
            key: "rank".to_owned(),
            label: "Rank".to_owned(),
        };
        let metadata_column_id = metadata_column.id();
        configuration.metadata_columns = vec![metadata_column];
        configuration.sort_state = Some(components::SortState::new(
            &metadata_column_id,
            components::SortDirection::Ascending,
        ));
        assert_eq!(
            prepared_names(
                &mut cache,
                &cluster,
                &deployment,
                &ResourceSearchState::default(),
                &configuration,
            ),
            ["a-p-i", "api-server", "my-api", "api", "worker"]
        );

        let invalid_regex = ResourceSearchState {
            query: "[".to_owned(),
            regex_mode: true,
        };
        assert!(
            prepared_names(
                &mut cache,
                &cluster,
                &deployment,
                &invalid_regex,
                &configuration,
            )
            .is_empty()
        );
        assert!(cache.prepared().regex_error.is_some());
    }

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
}
