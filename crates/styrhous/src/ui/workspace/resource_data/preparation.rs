use super::{ResourceTableData, watches::resource_watch_namespaces_for};
use crate::api_resource::ApiResource;
use crate::minimal_resource::MinimalResource;
use crate::resource_table::{CPU_COLUMN, MEMORY_COLUMN, PODS_COLUMN, SortValue};
use crate::ui::metadata_fields::MetadataKeySuggestions;
use crate::ui::resource_table_cache::{
    Grouping, PreparedGroup, PreparedResourceIdentity, PreparedResourceTable,
    PreparedResourceTableRow, ResourceTableCache, ResourceTableCacheKey,
};
use crate::ui::state::{ResourceSearchState, ResourceWatchKey, ResourceWatchState};
use crate::ui::workspace::resource_table::{
    ResourceTableConfiguration, resource_column_sort_value,
};
use components::fuzzy::{FuzzyMatchScore, fuzzy_match_score, normalize_for_search};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(in crate::ui::workspace) fn prepare_resource_table<'a>(
    cache: &'a mut ResourceTableCache,
    data: ResourceTableData<'_>,
    api_resource: &ApiResource,
    resource_search: &ResourceSearchState,
    configuration: &ResourceTableConfiguration,
) -> &'a PreparedResourceTable {
    let mut watch_keys = resource_watch_namespaces_for(
        data.selected_namespaces,
        data.watches_every_namespace,
        data.resource_cache,
        api_resource,
    )
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
    let is_pod = api_resource.is_pod();
    let is_node = api_resource.is_node();
    let sort = configuration
        .layout
        .sort
        .as_ref()
        .map(|sort| (sort.column_id.clone(), sort.direction));
    let is_metric_sort = sort
        .as_ref()
        .is_some_and(|(column, _)| column == CPU_COLUMN || column == MEMORY_COLUMN);
    let is_pod_count_sort = sort
        .as_ref()
        .is_some_and(|(column, _)| column == PODS_COLUMN);
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
        node_pod_count_revision: if is_node && is_pod_count_sort {
            data.metrics.node_pod_counts.revision
        } else {
            0
        },
        search_query: resource_search.query.clone(),
        regex_mode: resource_search.regex_mode,
        sort,
        grouping: configuration.grouping,
        problems_only: resource_search.problems_only,
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

/// The name of the one group of a cluster-scoped kind's rows.
pub(in crate::ui::workspace) const CLUSTER_GROUP: &str = "cluster";

struct ResourceCandidate {
    identity: PreparedResourceIdentity,
    name: String,
    default_name: String,
    sort_value: Option<SortValue>,
    fuzzy_score: Option<FuzzyMatchScore>,
    namespace: Option<String>,
}

fn build_prepared_resource_table(
    data: ResourceTableData<'_>,
    key: ResourceTableCacheKey,
    watch_keys: Vec<ResourceWatchKey>,
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
                    problem: resource.problem_tone(),
                },
                namespace: resource.namespace.clone(),
                name: resource.name.clone(),
                default_name: resource.name.to_lowercase(),
                sort_value: configuration.layout.sort.as_ref().map(|sort| {
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
            if configuration.layout.sort.is_none() {
                candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.fuzzy_score));
            }
        }
        None
    };

    if let Some(sort) = &configuration.layout.sort {
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
    let problem_count = candidates
        .iter()
        .filter(|candidate| candidate.identity.problem.is_some())
        .count();
    let (mut rows, mut groups) = group_candidates(candidates, &key);
    let shown_count = rows.len();
    if resource_count > visible_resource_count && regex_error.is_none() {
        let start = rows.len();
        rows.push(PreparedResourceTableRow::HiddenBySearch(
            resource_count - visible_resource_count,
        ));
        groups.push(PreparedGroup {
            name: None,
            total: 0,
            failing: 0,
            rows: start..rows.len(),
        });
    }

    PreparedResourceTable {
        key,
        watch_keys,
        rows,
        groups,
        resource_count,
        visible_resource_count,
        problem_count,
        shown_count,
        regex_error,
        metadata_key_suggestions: MetadataKeySuggestions {
            labels: labels.into_iter().collect(),
            annotations: annotations.into_iter().collect(),
        },
        generation,
    }
}

/// The rows of `candidates` (in their order) grouped as `key` says, with the
/// problem rows only while `key.problems_only`; groups without a row are
/// left out.
fn group_candidates(
    candidates: Vec<ResourceCandidate>,
    key: &ResourceTableCacheKey,
) -> (Vec<PreparedResourceTableRow>, Vec<PreparedGroup>) {
    #[derive(Default)]
    struct Accumulator {
        total: usize,
        failing: usize,
        rows: Vec<PreparedResourceIdentity>,
    }
    let mut accumulators: BTreeMap<Option<String>, Accumulator> = BTreeMap::new();
    for candidate in candidates {
        let name = match key.grouping {
            Grouping::None => None,
            Grouping::Namespace => Some(candidate.namespace.unwrap_or_default()),
            Grouping::Cluster => Some(CLUSTER_GROUP.to_owned()),
        };
        let accumulator = accumulators.entry(name).or_default();
        accumulator.total += 1;
        let problem = candidate.identity.problem.is_some();
        accumulator.failing += usize::from(problem);
        if problem || !key.problems_only {
            accumulator.rows.push(candidate.identity);
        }
    }
    let mut rows = Vec::new();
    let mut groups = Vec::new();
    for (name, accumulator) in accumulators {
        if accumulator.rows.is_empty() {
            continue;
        }
        let start = rows.len();
        rows.extend(
            accumulator
                .rows
                .into_iter()
                .map(PreparedResourceTableRow::Resource),
        );
        groups.push(PreparedGroup {
            name,
            total: accumulator.total,
            failing: accumulator.failing,
            rows: start..rows.len(),
        });
    }
    (rows, groups)
}

pub(in crate::ui::workspace) fn resolve_prepared_resource<'a>(
    resource_cache: &'a HashMap<ResourceWatchKey, ResourceWatchState>,
    prepared: &PreparedResourceTable,
    identity: &PreparedResourceIdentity,
) -> Option<&'a MinimalResource> {
    let watch_key = prepared.watch_keys.get(identity.watch_index)?;
    resource_cache.get(watch_key)?.resources.get(&identity.uid)
}
