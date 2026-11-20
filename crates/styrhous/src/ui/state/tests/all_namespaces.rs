use super::*;

/// A connected cluster with namespaces `a` and `b`, showing `api_resource`.
fn state_showing(api_resource: ApiResource) -> UiState {
    let mut state = connected_state();
    let mut commands = Vec::new();
    state
        .clusters
        .get_mut(&1)
        .unwrap()
        .pod_metrics_api_available = true;
    replace_namespaces(&mut state, ["a", "b"]);
    state.select_api_resource(1, api_resource, &mut commands);
    state
}

fn add_namespace(state: &mut UiState, name: &str) -> Vec<WorkerCommandBox> {
    let mut commands = Vec::new();
    KubernetesNamespacesAdded {
        cluster_key: 1,
        namespace: MinimalNamespace::named(name),
    }
    .apply(state, &mut commands);
    commands
}

fn delete_namespace(state: &mut UiState, name: &str) -> Vec<WorkerCommandBox> {
    let mut commands = Vec::new();
    KubernetesNamespacesDeleted {
        cluster_key: 1,
        namespace_name: name.to_owned(),
    }
    .apply(state, &mut commands);
    commands
}

fn replace_namespaces<const N: usize>(
    state: &mut UiState,
    names: [&str; N],
) -> Vec<WorkerCommandBox> {
    let mut commands = Vec::new();
    KubernetesNamespacesReplaced {
        cluster_key: 1,
        namespaces: names.map(MinimalNamespace::named).into(),
    }
    .apply(state, &mut commands);
    commands
}

/// Deliver the first list of `api_resource` in `namespace`.
fn list_resources(state: &mut UiState, api_resource: ApiResource, namespace: &str) {
    KubernetesResourcesReplaced {
        cluster_key: 1,
        api_resource,
        namespace: Some(namespace.to_owned()),
        resources: Vec::new(),
    }
    .apply(state, &mut Vec::new());
}

/// Every namespace named by the watch sources the commands request.
fn watched_namespaces(commands: &[WorkerCommandBox]) -> BTreeSet<String> {
    commands_of::<ReconcileResourceWatches>(commands)
        .flat_map(|command| command.sources.iter())
        .flat_map(|source| match source {
            ResourceWatchSource::AllNamespaces(namespaces) => namespaces.clone(),
            ResourceWatchSource::Namespace(namespace) => BTreeSet::from([namespace.clone()]),
            ResourceWatchSource::Cluster => BTreeSet::new(),
        })
        .collect()
}

/// The namespaces of the single all-namespaces source the commands request.
fn all_namespaces_source(commands: &[WorkerCommandBox]) -> Option<&BTreeSet<String>> {
    commands_of::<ReconcileResourceWatches>(commands).find_map(|command| {
        match command.sources.as_slice() {
            [ResourceWatchSource::AllNamespaces(namespaces)] => Some(namespaces),
            _ => None,
        }
    })
}

fn started_pod_metrics(commands: &[WorkerCommandBox]) -> Vec<&str> {
    commands_of::<StartPodMetricsWatch>(commands)
        .map(|command| command.namespace.as_str())
        .collect()
}

fn stopped_pod_metrics(commands: &[WorkerCommandBox]) -> Vec<&str> {
    commands_of::<StopPodMetricsWatch>(commands)
        .map(|command| command.namespace.as_str())
        .collect()
}

fn names<const N: usize>(names: [&str; N]) -> BTreeSet<String> {
    names.map(str::to_owned).into()
}

#[test]
fn all_namespaces_selection_watches_namespaces_created_later() {
    let mut state = state_showing(pods());
    state.select_all_namespaces(1, &mut Vec::new());

    let commands = add_namespace(&mut state, "c");

    let cluster = &state.clusters[&1];
    assert_eq!(cluster.selected_namespaces, SelectedNamespaces::All);
    assert_eq!(
        all_namespaces_source(&commands),
        Some(&names(["a", "b", "c"]))
    );
    assert_eq!(started_pod_metrics(&commands), ["c"]);
    assert!(cluster.namespace_scope().contains("c"));
    assert_eq!(
        state.cluster_selections.selections["kind"].selected_namespaces,
        SelectedNamespaces::All
    );
}

#[test]
fn all_namespaces_watcher_keeps_its_rows_while_it_restarts_for_a_new_namespace() {
    let mut state = state_showing(pods());
    state.select_all_namespaces(1, &mut Vec::new());
    list_resources(&mut state, pods(), "a");

    add_namespace(&mut state, "c");

    let cluster = &state.clusters[&1];
    assert!(cluster.resource_cache[&(pods(), Some("a".to_owned()))].is_synced);
    assert!(cluster.active_watchers.contains(&(pods(), None)));
    assert!(cluster.is_namespace_watched(&pods(), "c"));
}

#[test]
fn updating_a_namespace_does_not_restart_the_all_namespaces_watcher() {
    let mut state = state_showing(pods());
    state.select_all_namespaces(1, &mut Vec::new());

    let mut commands = Vec::new();
    KubernetesNamespacesAdded {
        cluster_key: 1,
        namespace: MinimalNamespace {
            labels: BTreeMap::from([("team".to_owned(), "platform".to_owned())]),
            ..MinimalNamespace::named("a")
        },
    }
    .apply(&mut state, &mut commands);
    commands.extend(replace_namespaces(&mut state, ["a", "b"]));

    assert_eq!(
        commands_of::<ReconcileResourceWatches>(&commands).count(),
        0
    );
}

#[test]
fn namespace_relist_in_all_namespaces_mode_follows_the_new_set() {
    let mut state = state_showing(pods());
    state.select_all_namespaces(1, &mut Vec::new());
    list_resources(&mut state, pods(), "a");
    list_resources(&mut state, pods(), "b");

    let commands = replace_namespaces(&mut state, ["b", "c"]);

    let cluster = &state.clusters[&1];
    assert_eq!(cluster.selected_namespaces, SelectedNamespaces::All);
    assert_eq!(all_namespaces_source(&commands), Some(&names(["b", "c"])));
    assert_eq!(stopped_pod_metrics(&commands), ["a"]);
    assert_eq!(started_pod_metrics(&commands), ["c"]);
    assert!(
        !cluster
            .resource_cache
            .contains_key(&(pods(), Some("a".to_owned())))
    );
    assert!(cluster.resource_cache[&(pods(), Some("b".to_owned()))].is_synced);
    assert_eq!(
        state.cluster_selections.selections["kind"].selected_namespaces,
        SelectedNamespaces::All
    );
}

#[test]
fn all_namespaces_mode_survives_a_deleted_namespace() {
    let mut state = state_showing(pods());
    state.select_all_namespaces(1, &mut Vec::new());

    let commands = delete_namespace(&mut state, "a");
    assert_eq!(
        state.clusters[&1].selected_namespaces,
        SelectedNamespaces::All
    );
    assert_eq!(stopped_pod_metrics(&commands), ["a"]);
    assert_eq!(
        state.cluster_selections.selections["kind"].selected_namespaces,
        SelectedNamespaces::All
    );

    let commands = add_namespace(&mut state, "c");

    assert_eq!(all_namespaces_source(&commands), Some(&names(["b", "c"])));
}

#[test]
fn selection_completed_by_hand_does_not_grow_with_new_namespaces() {
    let mut state = state_showing(pods());
    state.replace_selected_namespaces(1, ["a".to_owned()], &mut Vec::new());
    state.toggle_namespace(1, "b".to_owned(), &mut Vec::new());
    assert!(state.clusters[&1].active_watchers.contains(&(pods(), None)));
    list_resources(&mut state, pods(), "a");
    list_resources(&mut state, pods(), "b");

    let commands = add_namespace(&mut state, "c");

    let cluster = &state.clusters[&1];
    assert_eq!(
        cluster.selected_namespaces,
        SelectedNamespaces::from(["a", "b"])
    );
    // The all-namespaces watcher is replaced by one per selected namespace, so
    // both must be listed again rather than kept from the replaced watcher.
    assert_eq!(watched_namespaces(&commands), names(["a", "b"]));
    assert!(
        !cluster
            .resource_cache
            .contains_key(&(pods(), Some("a".to_owned())))
    );
    assert!(!cluster.is_namespace_watched(&pods(), "c"));
    assert!(started_pod_metrics(&commands).is_empty());
}

#[test]
fn deselecting_one_namespace_leaves_all_namespaces_mode() {
    let mut state = state_showing(pods());
    state.select_all_namespaces(1, &mut Vec::new());
    state.toggle_namespace(1, "a".to_owned(), &mut Vec::new());
    assert_eq!(
        state.clusters[&1].selected_namespaces,
        SelectedNamespaces::from(["b"])
    );

    let commands = add_namespace(&mut state, "c");

    assert_eq!(
        state.clusters[&1].selected_namespaces,
        SelectedNamespaces::from(["b"])
    );
    assert!(!watched_namespaces(&commands).contains("c"));
}

#[test]
fn helm_releases_in_all_namespaces_mode_watch_namespaces_created_later() {
    let mut state = state_showing(ApiResource::helm_releases());
    state.select_all_namespaces(1, &mut Vec::new());

    let commands = add_namespace(&mut state, "c");

    assert!(watched_namespaces(&commands).contains("c"));
    assert!(
        state.clusters[&1]
            .active_watchers
            .contains(&(ApiResource::helm_releases(), Some("c".to_owned())))
    );
}

#[test]
fn toggling_out_of_all_namespaces_keeps_the_other_discovered_namespaces() {
    let discovered = ["a", "b", "c"]
        .map(|name| (SortedName::new(name), MinimalNamespace::named(name)))
        .into();
    let mut selection = SelectedNamespaces::All;

    selection.toggle("b", &discovered);
    assert_eq!(selection, SelectedNamespaces::from(["a", "c"]));
    selection.toggle("b", &discovered);
    assert_eq!(selection, SelectedNamespaces::from(["a", "b", "c"]));
    assert!(
        NamespaceScope::new(&selection, &discovered).covers_all_discovered(),
        "an explicit set may cover everything without becoming All"
    );
}

#[test]
fn all_namespaces_with_nothing_discovered_selects_nothing() {
    let discovered = BTreeMap::new();
    let scope = NamespaceScope::new(&SelectedNamespaces::All, &discovered);

    assert!(scope.is_empty());
    assert!(!scope.covers_all_discovered());
}
