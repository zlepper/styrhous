//! Tables that do not wait for the namespace list: watches across every namespace, and
//! metrics columns whose availability discovery has not decided yet.

use super::super::{selected_resources, selected_watch_error, selected_watches_are_loading};
use super::*;
use crate::api_resource::ApiResource;
use crate::ui::state::ClusterLoadState;
use crate::worker::{
    KubernetesResourceWatchFailed, KubernetesResourcesReplacedInEveryNamespace, WorkerResult,
};

/// Cluster 1 showing pods across every namespace before the namespace list arrived.
fn every_namespace_state() -> (UiState, ApiResource) {
    let pods = api_resource("Pod", "pods", true);
    let mut cluster = ClusterState::for_test(1, "test");
    cluster.namespaces_load = ClusterLoadState::Loading;
    cluster.selected_namespaces = SelectedNamespaces::All;
    let mut state = UiState {
        clusters: HashMap::from([(1, cluster)]),
        selected_cluster: Some(1),
        ..Default::default()
    };
    state.select_api_resource(1, pods.clone(), &mut Vec::new());
    (state, pods)
}

fn in_namespace(name: &str, namespace: &str) -> MinimalResource {
    MinimalResource {
        namespace: Some(namespace.to_owned()),
        ..table_resource(name)
    }
}

#[test]
fn rows_of_namespaces_missing_from_the_list_are_shown_once_the_watch_listed() {
    let (mut state, pods) = every_namespace_state();
    assert!(state.clusters[&1].watches_every_namespace(&pods));
    assert!(selected_watches_are_loading(&state.clusters[&1], &pods));

    KubernetesResourcesReplacedInEveryNamespace {
        cluster_key: 1,
        api_resource: pods.clone(),
        resources_by_namespace: BTreeMap::from([
            ("apps".to_owned(), vec![in_namespace("api", "apps")]),
            ("jobs".to_owned(), vec![in_namespace("worker", "jobs")]),
        ]),
    }
    .apply(&mut state, &mut Vec::new());

    let cluster = &state.clusters[&1];
    assert!(cluster.namespaces.is_empty());
    assert!(!selected_watches_are_loading(cluster, &pods));
    assert_eq!(
        selected_resources(cluster, Some(&pods))
            .iter()
            .map(|resource| resource.name.as_str())
            .collect::<Vec<_>>(),
        ["api", "worker"]
    );
}

#[test]
fn a_namespace_emptied_by_a_relist_invalidates_the_prepared_table() {
    let (mut state, pods) = every_namespace_state();
    let configuration = resource_table_configuration(
        1_280.0,
        &pods,
        &[],
        true,
        &mut PersistedResourceTablePreferences::default(),
    );
    let relist = |state: &mut UiState, resources: Vec<MinimalResource>| {
        KubernetesResourcesReplacedInEveryNamespace {
            cluster_key: 1,
            api_resource: pods.clone(),
            resources_by_namespace: if resources.is_empty() {
                BTreeMap::new()
            } else {
                BTreeMap::from([("apps".to_owned(), resources)])
            },
        }
        .apply(state, &mut Vec::new());
    };
    relist(&mut state, vec![in_namespace("api", "apps")]);
    let cluster = state.clusters.get_mut(&1).unwrap();
    prepare_cluster_table(
        cluster,
        &pods,
        &ResourceSearchState::default(),
        &configuration,
    );
    assert_eq!(cluster.resource_table_cache.prepared().resource_count, 1);

    relist(&mut state, Vec::new());
    let cluster = state.clusters.get_mut(&1).unwrap();
    prepare_cluster_table(
        cluster,
        &pods,
        &ResourceSearchState::default(),
        &configuration,
    );

    assert_eq!(cluster.resource_table_cache.prepared().resource_count, 0);
}

#[test]
fn a_failed_every_namespace_watch_is_an_error_rather_than_loading() {
    let (mut state, pods) = every_namespace_state();

    KubernetesResourceWatchFailed {
        cluster_key: 1,
        api_resource: pods.clone(),
        namespace: None,
        error: "forbidden".to_owned(),
    }
    .apply(&mut state, &mut Vec::new());

    assert_eq!(
        selected_watch_error(&state.clusters[&1], &pods).as_deref(),
        Some("forbidden")
    );
}

#[test]
fn metrics_columns_wait_for_discovery_before_reporting_the_api_unavailable() {
    let pod = api_resource("Pod", "pods", true);
    let mut cluster = cluster_with_resources(&pod, [table_resource("api")]);
    cluster.pod_metrics_api_available = false;
    cluster.api_discovery.complete = false;
    let resource = cluster.resource_cache[&(pod.clone(), Some("default".to_owned()))]
        .resources
        .values()
        .next()
        .unwrap()
        .clone();

    let undecided = resolved_resource_cell(
        &resource,
        CPU_COLUMN,
        ResourceTableData::of(&cluster, &pod).metrics,
        &pod,
    );
    cluster.api_discovery.complete = true;
    let decided = resolved_resource_cell(
        &resource,
        CPU_COLUMN,
        ResourceTableData::of(&cluster, &pod).metrics,
        &pod,
    );

    assert_ne!(undecided, Some(CellValue::Unavailable));
    assert_eq!(decided, Some(CellValue::Unavailable));
}
