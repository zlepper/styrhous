use super::*;
use crate::resource_table::{ROLES_COLUMN, STATUS_COLUMN, Tone};
use crate::ui::resource_table_cache::PreparedGroup;
use crate::ui::workspace::resource_table::{problem_rows, step_to_problem};
use crate::ui::workspace::toolbar::ProblemStep;
use std::collections::HashSet;

fn pod_in(namespace: &str, name: &str, status: Option<Tone>) -> MinimalResource {
    let mut resource = table_resource(name);
    resource.namespace = Some(namespace.to_owned());
    resource.uid = format!("uid-{namespace}-{name}");
    if let Some(tone) = status {
        resource.cells.insert(
            STATUS_COLUMN.to_owned(),
            CellValue::Status {
                label: "Status".to_owned(),
                tone,
            },
        );
    }
    resource
}

/// Pods of `a` and `b`: `a` has one failed and one pending Pod, `b` is healthy.
fn grouped_cluster(pod: &crate::api_resource::ApiResource) -> ClusterState {
    let mut cluster = ClusterState::for_test(1, "test");
    cluster.selected_namespaces = SelectedNamespaces::from(["a", "b"]);
    for (namespace, resources) in [
        (
            "b",
            vec![
                pod_in("b", "web", Some(Tone::Success)),
                pod_in("b", "api", Some(Tone::Success)),
            ],
        ),
        (
            "a",
            vec![
                pod_in("a", "job", Some(Tone::Danger)),
                pod_in("a", "cache", Some(Tone::Success)),
                pod_in("a", "queue", Some(Tone::Warning)),
            ],
        ),
    ] {
        cluster.resource_cache.insert(
            (pod.clone(), Some(namespace.to_owned())),
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
    }
    cluster
}

fn pod_configuration(pod: &crate::api_resource::ApiResource) -> ResourceTableConfiguration {
    resource_table_configuration(
        1_280.0,
        pod,
        &[],
        true,
        false,
        &mut PersistedResourceTablePreferences::default(),
    )
}

fn group_summary(groups: &[PreparedGroup]) -> Vec<(Option<&str>, usize, usize, usize)> {
    groups
        .iter()
        .map(|group| {
            (
                group.name.as_deref(),
                group.total,
                group.failing,
                group.rows.len(),
            )
        })
        .collect()
}

#[test]
fn pods_group_by_namespace_with_their_problems_counted() {
    let pod = api_resource("Pod", "pods", true);
    let cluster = grouped_cluster(&pod);
    let configuration = pod_configuration(&pod);
    assert_eq!(configuration.grouping, Grouping::Namespace);
    let mut cache = ResourceTableCache::default();

    let names = prepared_names(
        &mut cache,
        &cluster,
        &pod,
        &ResourceSearchState::default(),
        &configuration,
    );
    assert_eq!(names, ["cache", "job", "queue", "api", "web"]);
    let prepared = cache.prepared();
    assert_eq!(
        group_summary(&prepared.groups),
        [(Some("a"), 3, 2, 3), (Some("b"), 2, 0, 2)]
    );
    assert_eq!(prepared.problem_count, 2);
    assert_eq!(prepared.shown_count, 5);
    assert_eq!(prepared.groups[1].rows, 3..5);
}

#[test]
fn problems_only_keeps_the_totals_and_drops_groups_without_problems() {
    let pod = api_resource("Pod", "pods", true);
    let cluster = grouped_cluster(&pod);
    let configuration = pod_configuration(&pod);
    let mut cache = ResourceTableCache::default();
    let problems_only = ResourceSearchState {
        problems_only: true,
        ..Default::default()
    };

    let names = prepared_names(&mut cache, &cluster, &pod, &problems_only, &configuration);
    assert_eq!(names, ["job", "queue"]);
    let prepared = cache.prepared();
    assert_eq!(
        group_summary(&prepared.groups),
        [(Some("a"), 3, 2, 2)],
        "the group still says it has three Pods, two failing"
    );
    assert_eq!(prepared.problem_count, 2, "the chip's count stays live");
    assert_eq!(prepared.shown_count, 2);
    assert_eq!(prepared.visible_resource_count, 5);

    let search = ResourceSearchState {
        query: "web".to_owned(),
        problems_only: true,
        ..Default::default()
    };
    assert!(prepared_names(&mut cache, &cluster, &pod, &search, &configuration).is_empty());
    assert_eq!(
        cache.prepared().problem_count,
        0,
        "problems of the search only"
    );
}

#[test]
fn ungrouped_rows_form_one_group_without_a_header() {
    let pod = api_resource("Pod", "pods", true);
    let cluster = grouped_cluster(&pod);
    let mut configuration = pod_configuration(&pod);
    configuration.grouping = Grouping::None;
    let mut cache = ResourceTableCache::default();

    prepared_names(
        &mut cache,
        &cluster,
        &pod,
        &ResourceSearchState::default(),
        &configuration,
    );
    assert_eq!(group_summary(&cache.prepared().groups), [(None, 5, 2, 5)]);
}

#[test]
fn rows_the_search_hides_are_counted_in_a_group_of_their_own() {
    let pod = api_resource("Pod", "pods", true);
    let cluster = grouped_cluster(&pod);
    let configuration = pod_configuration(&pod);
    let mut cache = ResourceTableCache::default();
    let search = ResourceSearchState {
        query: "a".to_owned(),
        ..Default::default()
    };

    prepared_names(&mut cache, &cluster, &pod, &search, &configuration);
    let prepared = cache.prepared();
    let last = prepared.groups.last().expect("a group");
    assert_eq!(last.name, None);
    assert!(matches!(
        prepared.rows[last.rows.start],
        PreparedResourceTableRow::HiddenBySearch(hidden) if hidden > 0
    ));
}

#[test]
fn a_nodes_rows_share_the_cluster_group() {
    let node = api_resource("Node", "nodes", false);
    let mut cluster = ClusterState::for_test(1, "test");
    let mut worker = table_resource("worker");
    worker.namespace = None;
    worker
        .cells
        .insert(ROLES_COLUMN.to_owned(), CellValue::List(Vec::new()));
    let mut control_plane = table_resource("control-plane");
    control_plane.namespace = None;
    cluster.resource_cache.insert(
        (node.clone(), None),
        ResourceWatchState {
            resources: [worker, control_plane]
                .into_iter()
                .map(|resource| (resource.uid.clone(), resource))
                .collect(),
            is_synced: true,
            revision: 1,
            ..Default::default()
        },
    );
    let configuration = resource_table_configuration(
        1_280.0,
        &node,
        &[],
        false,
        false,
        &mut PersistedResourceTablePreferences::default(),
    );
    let mut cache = ResourceTableCache::default();

    prepared_names(
        &mut cache,
        &cluster,
        &node,
        &ResourceSearchState::default(),
        &configuration,
    );
    assert_eq!(
        group_summary(&cache.prepared().groups),
        [(Some("cluster"), 2, 0, 2)]
    );
}

#[test]
fn stepping_goes_on_from_the_last_problem_wraps_and_expands_its_group() {
    let pod = api_resource("Pod", "pods", true);
    let cluster = grouped_cluster(&pod);
    let configuration = pod_configuration(&pod);
    let mut cache = ResourceTableCache::default();
    let mut search = ResourceSearchState::default();
    prepared_names(&mut cache, &cluster, &pod, &search, &configuration);
    let prepared = cache.prepared();
    let step = |search: &mut ResourceSearchState, step| {
        step_to_problem(prepared, search, step);
        search.problem_cursor.clone().expect("a problem was found")
    };

    assert_eq!(step(&mut search, ProblemStep::Next), "uid-a-job");
    assert_eq!(search.scroll_to.as_deref(), Some("uid-a-job"));
    assert_eq!(step(&mut search, ProblemStep::Next), "uid-a-queue");
    assert_eq!(step(&mut search, ProblemStep::Next), "uid-a-job", "wraps");
    assert_eq!(
        step(&mut search, ProblemStep::Previous),
        "uid-a-queue",
        "wraps back"
    );

    search.collapsed_groups = HashSet::from(["a".to_owned()]);
    assert!(
        problem_rows(prepared, &search.collapsed_groups, false).is_empty(),
        "ticks mark the rows that show"
    );
    assert_eq!(step(&mut search, ProblemStep::Previous), "uid-a-job");
    assert!(
        search.collapsed_groups.is_empty(),
        "the group of the problem expands"
    );
}

#[test]
fn a_list_without_problems_has_nothing_to_step_to() {
    let pod = api_resource("Pod", "pods", true);
    let mut cluster = ClusterState::for_test(1, "test");
    cluster.selected_namespaces = SelectedNamespaces::from(["default"]);
    cluster.resource_cache.insert(
        (pod.clone(), Some("default".to_owned())),
        ResourceWatchState {
            resources: [table_resource("web")]
                .into_iter()
                .map(|resource| (resource.uid.clone(), resource))
                .collect(),
            is_synced: true,
            revision: 1,
            ..Default::default()
        },
    );
    let configuration = pod_configuration(&pod);
    let mut cache = ResourceTableCache::default();
    let mut search = ResourceSearchState::default();
    prepared_names(&mut cache, &cluster, &pod, &search, &configuration);
    step_to_problem(cache.prepared(), &mut search, ProblemStep::Next);
    assert_eq!(search.problem_cursor, None);
    assert_eq!(search.scroll_to, None);
}
