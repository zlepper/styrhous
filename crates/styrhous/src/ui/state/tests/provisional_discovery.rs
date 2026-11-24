//! The workspace before and while discovery answers: provisional resources, the restored
//! selection, the health check, and how discovery reconciles them.

use super::*;
use crate::api_resource::ApiResourceSet;

fn resource(group: &str, version: &str, kind: &str, name: &str) -> ApiResource {
    ApiResource {
        group: group.to_owned(),
        version: version.to_owned(),
        kind: kind.to_owned(),
        name: name.to_owned(),
        namespaced: true,
    }
}

fn widgets(version: &str) -> ApiResource {
    resource("example.com", version, "Widget", "widgets")
}

fn saved(group: &str, name: &str, namespaces: SelectedNamespaces) -> PersistedClusterSelections {
    PersistedClusterSelections {
        selections: BTreeMap::from([(
            "kind".to_owned(),
            PersistedClusterSelection {
                selected_namespaces: namespaces,
                selected_api_resource: Some(PersistedApiResource {
                    group: group.to_owned(),
                    name: name.to_owned(),
                }),
            },
        )]),
        last_selected_context: Some("kind".to_owned()),
    }
}

fn cache_of(api_resources: Vec<ApiResource>) -> PersistedApiDiscovery {
    PersistedApiDiscovery {
        contexts: BTreeMap::from([(
            "kind".to_owned(),
            ApiResourceSet {
                api_resources,
                scalable_api_resources: Default::default(),
            },
        )]),
    }
}

/// Loads the "kind" context with the given persisted state, which connects it.
fn connect(state: &mut UiState) -> Vec<WorkerCommandBox> {
    let mut commands = Vec::new();
    KubernetesClustersUpdated(vec![Cluster {
        name: "kind".to_owned(),
        is_current: true,
        presentation: Default::default(),
        server: None,
    }])
    .apply(state, &mut commands);
    commands
}

fn discovered(state: &mut UiState, api_resources: Vec<ApiResource>) -> Vec<WorkerCommandBox> {
    let mut commands = Vec::new();
    KubernetesApiResourcesDiscovered {
        cluster_key: 1,
        resources: ApiResourceSet {
            api_resources,
            scalable_api_resources: Default::default(),
        },
    }
    .apply(state, &mut commands);
    commands
}

fn completed(state: &mut UiState, failed: &[&str]) -> Vec<WorkerCommandBox> {
    let mut commands = Vec::new();
    KubernetesApiDiscoveryCompleted {
        cluster_key: 1,
        failed_api_groups: failed
            .iter()
            .map(|group_version| FailedApiGroup {
                group_version: (*group_version).to_owned(),
                error: "503".to_owned(),
            })
            .collect(),
        group_list_failed: false,
    }
    .apply(state, &mut commands);
    commands
}

fn watched_resources(commands: &[WorkerCommandBox]) -> Vec<(String, String, usize)> {
    commands_of::<ReconcileResourceWatches>(commands)
        .map(|command| {
            (
                command.api_resource.name.clone(),
                command.api_resource.version.clone(),
                command.sources.len(),
            )
        })
        .collect()
}

fn selected(state: &UiState) -> Option<(String, String)> {
    state.clusters[&1]
        .selected_api_resource
        .as_ref()
        .map(|resource| (resource.name.clone(), resource.version.clone()))
}

fn shown(state: &UiState, name: &str) -> bool {
    state.clusters[&1]
        .displayed_api_resources
        .api_resources
        .iter()
        .any(|resource| resource.name == name)
}

#[test]
fn a_first_connection_shows_the_built_in_resources_and_watches_the_saved_one_at_once() {
    let mut state = UiState {
        cluster_selections: saved("core", "pods", SelectedNamespaces::All),
        ..Default::default()
    };

    let commands = connect(&mut state);

    let cluster = &state.clusters[&1];
    assert!(!cluster.reachable(), "nothing has answered yet");
    assert!(!cluster.api_discovery.complete);
    assert!(shown(&state, "deployments") && shown(&state, "nodes"));
    assert!(
        !shown(&state, "gateways"),
        "CRDs are only shown once discovered"
    );
    assert_eq!(cluster.selected_namespaces, SelectedNamespaces::All);
    assert!(
        commands
            .first()
            .is_some_and(|command| command.as_ref().as_any().is::<ConnectToCluster>()),
        "the worker connects before it starts the watch"
    );
    assert!(
        commands_of::<ReconcileResourceWatches>(&commands).any(|command| {
            command.api_resource.name == "pods"
                && matches!(
                    command.sources.as_slice(),
                    [ResourceWatchSource::AllNamespaces(WatchedNamespaces::Every)]
                )
        })
    );
}

#[test]
fn the_last_discovery_of_a_context_is_shown_instead_of_the_built_in_resources() {
    let mut state = UiState {
        cluster_selections: saved(
            "example.com",
            "widgets",
            SelectedNamespaces::from(["default"]),
        ),
        api_discovery_cache: cache_of(vec![pods(), widgets("v1")]),
        ..Default::default()
    };

    let commands = connect(&mut state);

    assert!(shown(&state, "widgets"));
    assert!(
        !shown(&state, "deployments"),
        "the cache replaces the baseline"
    );
    assert_eq!(
        watched_resources(&commands),
        vec![("widgets".to_owned(), "v1".to_owned(), 1)],
        "the saved custom resource is watched in its saved namespace at once"
    );
}

#[test]
fn any_authenticated_answer_proves_the_cluster_reachable() {
    for answer in ["discovery", "namespaces", "resources"] {
        let mut state = UiState::default();
        connect(&mut state);
        let mut commands = Vec::new();
        match answer {
            "discovery" => {
                discovered(&mut state, vec![pods()]);
            }
            "namespaces" => KubernetesNamespacesReplaced {
                cluster_key: 1,
                namespaces: vec![MinimalNamespace::named("default")],
            }
            .apply(&mut state, &mut commands),
            _ => KubernetesResourcesReplaced {
                cluster_key: 1,
                api_resource: pods(),
                namespace: Some("default".to_owned()),
                resources: Vec::new(),
            }
            .apply(&mut state, &mut commands),
        }

        assert!(state.clusters[&1].reachable(), "{answer}");
    }
}

#[test]
fn a_connection_that_discovery_already_failed_stays_failed() {
    let mut state = UiState::default();
    connect(&mut state);
    let mut commands = Vec::new();

    ClusterConnectionFailed {
        cluster_key: 1,
        error: "Unauthorized".to_owned(),
    }
    .apply(&mut state, &mut commands);
    KubernetesClusterConnectionCreated { cluster_key: 1 }.apply(&mut state, &mut commands);

    assert!(matches!(
        state.clusters[&1].connection,
        ClusterConnectionState::Failed(_)
    ));
}

#[test]
fn a_saved_resource_in_a_late_group_is_restored_when_the_group_arrives() {
    let mut state = UiState {
        cluster_selections: saved("example.com", "widgets", SelectedNamespaces::All),
        ..Default::default()
    };
    connect(&mut state);
    assert_eq!(selected(&state), None);

    discovered(&mut state, vec![pods()]);
    assert_eq!(selected(&state), None, "kept until its group answers");
    discovered(&mut state, vec![widgets("v1")]);

    assert_eq!(
        selected(&state),
        Some(("widgets".to_owned(), "v1".to_owned()))
    );
}

#[test]
fn a_saved_resource_is_forgotten_only_once_discovery_completes_without_it() {
    let mut state = UiState {
        cluster_selections: saved("example.com", "widgets", SelectedNamespaces::All),
        ..Default::default()
    };
    connect(&mut state);
    discovered(&mut state, vec![pods()]);
    assert!(
        state.cluster_selections.selections["kind"]
            .selected_api_resource
            .is_some()
    );

    completed(&mut state, &[]);

    assert!(
        state.cluster_selections.selections["kind"]
            .selected_api_resource
            .is_none()
    );
}

#[test]
fn a_saved_resource_in_a_failed_group_is_kept_for_its_recovery() {
    let mut state = UiState {
        cluster_selections: saved("example.com", "widgets", SelectedNamespaces::All),
        ..Default::default()
    };
    connect(&mut state);
    discovered(&mut state, vec![pods()]);

    completed(&mut state, &["example.com/v1"]);

    assert!(
        state.cluster_selections.selections["kind"]
            .selected_api_resource
            .is_some()
    );
}

#[test]
fn a_resource_chosen_while_discovering_is_not_replaced_by_the_saved_one() {
    let mut state = UiState {
        cluster_selections: saved("example.com", "widgets", SelectedNamespaces::All),
        ..Default::default()
    };
    connect(&mut state);
    let deployments = state.clusters[&1]
        .displayed_api_resources
        .api_resources
        .iter()
        .find(|resource| resource.name == "deployments")
        .cloned()
        .unwrap();
    state.select_api_resource(1, deployments, &mut Vec::new());

    discovered(&mut state, vec![widgets("v1")]);

    assert_eq!(
        selected(&state).map(|(name, _)| name),
        Some("deployments".to_owned())
    );
}

#[test]
fn a_cached_resource_in_a_new_version_is_followed_and_its_old_watch_stopped() {
    let mut state = UiState {
        cluster_selections: saved("example.com", "widgets", SelectedNamespaces::All),
        api_discovery_cache: cache_of(vec![widgets("v1beta1")]),
        ..Default::default()
    };
    connect(&mut state);
    assert_eq!(
        selected(&state),
        Some(("widgets".to_owned(), "v1beta1".to_owned()))
    );

    KubernetesResourcesReplacedInEveryNamespace {
        cluster_key: 1,
        api_resource: widgets("v1beta1"),
        resources_by_namespace: BTreeMap::new(),
    }
    .apply(&mut state, &mut Vec::new());
    assert!(
        state.clusters[&1]
            .resource_cache
            .contains_key(&(widgets("v1beta1"), None))
    );

    let commands = discovered(&mut state, vec![widgets("v1")]);

    assert_eq!(
        selected(&state),
        Some(("widgets".to_owned(), "v1".to_owned()))
    );
    assert_eq!(
        watched_resources(&commands),
        vec![
            ("widgets".to_owned(), "v1beta1".to_owned(), 0),
            ("widgets".to_owned(), "v1".to_owned(), 1),
        ],
        "the old version's watch stops and the new one starts"
    );
    assert!(
        !state.clusters[&1]
            .resource_cache
            .keys()
            .any(|(resource, _)| resource.version == "v1beta1"),
        "the old version's rows are dropped"
    );
}

#[test]
fn a_cached_resource_that_no_longer_exists_is_dropped_when_discovery_completes() {
    let gadgets = resource("removed.example.com", "v1", "Gadget", "gadgets");
    let mut state = UiState {
        cluster_selections: saved("removed.example.com", "gadgets", SelectedNamespaces::All),
        api_discovery_cache: cache_of(vec![pods(), gadgets.clone(), widgets("v1")]),
        ..Default::default()
    };
    connect(&mut state);
    discovered(&mut state, vec![pods()]);
    assert!(
        shown(&state, "gadgets"),
        "kept while discovery is still running"
    );

    let commands = completed(&mut state, &["example.com/v1"]);

    assert!(!shown(&state, "gadgets"));
    assert!(
        shown(&state, "widgets"),
        "its group failed, so it may still exist"
    );
    assert_eq!(selected(&state), None);
    assert_eq!(
        watched_resources(&commands),
        vec![("gadgets".to_owned(), "v1".to_owned(), 0)]
    );
    assert!(
        state.cluster_selections.selections["kind"]
            .selected_api_resource
            .is_none()
    );
    let cached = state.api_discovery_cache.contexts["kind"]
        .api_resources
        .iter()
        .map(|resource| resource.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(cached, vec!["pods", "widgets"]);
}

#[test]
fn the_metrics_api_is_only_trusted_once_discovery_reports_it() {
    let pod_metrics = resource("metrics.k8s.io", "v1beta1", "PodMetrics", "pods");
    let mut state = UiState {
        api_discovery_cache: cache_of(vec![pods(), pod_metrics.clone()]),
        ..Default::default()
    };
    connect(&mut state);
    assert!(!state.clusters[&1].pod_metrics_api_available);

    discovered(&mut state, vec![pod_metrics]);
    assert!(state.clusters[&1].pod_metrics_api_available);
}

#[test]
fn contexts_removed_from_the_kubeconfig_lose_their_cached_discovery() {
    let mut state = UiState {
        api_discovery_cache: cache_of(vec![pods()]),
        ..Default::default()
    };
    state
        .api_discovery_cache
        .contexts
        .insert("deleted".to_owned(), ApiResourceSet::default());

    connect(&mut state);

    assert_eq!(
        state
            .api_discovery_cache
            .contexts
            .keys()
            .collect::<Vec<_>>(),
        vec!["kind"]
    );
}

fn group_list_failed(state: &mut UiState) -> Vec<WorkerCommandBox> {
    let mut commands = Vec::new();
    KubernetesApiDiscoveryCompleted {
        cluster_key: 1,
        failed_api_groups: vec![FailedApiGroup {
            group_version: "API group list".to_owned(),
            error: "timed out".to_owned(),
        }],
        group_list_failed: true,
    }
    .apply(state, &mut commands);
    commands
}

fn recovered(state: &mut UiState, api_resources: Vec<ApiResource>) -> Vec<WorkerCommandBox> {
    let mut commands = Vec::new();
    KubernetesApiGroupsRecovered {
        cluster_key: 1,
        group_versions: vec!["apps/v1".to_owned()],
        resources: ApiResourceSet {
            api_resources,
            scalable_api_resources: Default::default(),
        },
        still_failing_group_versions: Vec::new(),
    }
    .apply(state, &mut commands);
    commands
}

fn deployments(version: &str) -> ApiResource {
    resource("apps", version, "Deployment", "deployments")
}

#[test]
fn a_failed_group_list_keeps_every_assumed_group_until_it_is_listed() {
    let mut state = UiState {
        cluster_selections: saved("apps", "deployments", SelectedNamespaces::All),
        ..Default::default()
    };
    connect(&mut state);
    discovered(&mut state, vec![pods()]);

    group_list_failed(&mut state);

    assert!(shown(&state, "deployments"), "unknown, not missing");
    assert_eq!(
        state.clusters[&1].metrics_shown_as_available(),
        (true, true),
        "whether the Metrics API exists is still unknown"
    );
    assert_eq!(
        selected(&state).map(|(name, _)| name),
        Some("deployments".to_owned())
    );
    assert!(
        state.cluster_selections.selections["kind"]
            .selected_api_resource
            .is_some()
    );

    recovered(&mut state, vec![deployments("v1")]);

    let discovery = &state.clusters[&1].api_discovery;
    assert!(!discovery.group_list_failed);
    assert!(
        discovery.absence_is_final("batch"),
        "the listed groups are now known"
    );
    assert!(
        !shown(&state, "jobs"),
        "the assumed groups that were not listed go away"
    );
    assert_eq!(
        state.api_discovery_cache.contexts["kind"]
            .api_resources
            .iter()
            .map(|resource| resource.name.as_str())
            .collect::<Vec<_>>(),
        vec!["pods", "deployments"],
        "a recovery refreshes the cached discovery"
    );
}

#[test]
fn a_failed_connection_ends_its_session_and_ignores_late_answers() {
    let mut state = UiState::default();
    connect(&mut state);
    let mut commands = Vec::new();

    ClusterConnectionFailed {
        cluster_key: 1,
        error: "Unauthorized".to_owned(),
    }
    .apply(&mut state, &mut commands);
    KubernetesNamespacesReplaced {
        cluster_key: 1,
        namespaces: vec![MinimalNamespace::named("default")],
    }
    .apply(&mut state, &mut commands);

    assert!(
        commands_of::<DisconnectCluster>(&commands).any(|command| command.cluster_key == 1),
        "its namespace watcher and watches must not keep retrying"
    );
    assert!(!state.clusters[&1].reachable());
}

#[test]
fn retrying_after_a_namespace_failure_restarts_the_watches_without_reconnecting() {
    let mut state = UiState {
        cluster_selections: saved("core", "nodes", SelectedNamespaces::All),
        ..Default::default()
    };
    connect(&mut state);
    let mut commands = Vec::new();
    KubernetesClusterConnectionCreated { cluster_key: 1 }.apply(&mut state, &mut commands);
    KubernetesNamespacesReconnecting {
        cluster_key: 1,
        error: "forbidden".to_owned(),
    }
    .apply(&mut state, &mut commands);
    assert!(matches!(
        state.clusters[&1].namespaces_load,
        ClusterLoadState::Failed(_)
    ));
    // The table's watch failed for good as well, so retrying has something to restart.
    KubernetesResourceWatchFailed {
        cluster_key: 1,
        api_resource: state.clusters[&1].selected_api_resource.clone().unwrap(),
        namespace: None,
        error: "forbidden".to_owned(),
    }
    .apply(&mut state, &mut commands);

    let mut commands = Vec::new();
    state.retry_selected_load(1, &mut commands);

    assert!(commands_of::<ConnectToCluster>(&commands).next().is_none());
    assert!(
        commands_of::<ReconcileResourceWatches>(&commands)
            .any(|command| command.api_resource.name == "nodes")
    );
}

#[test]
fn a_reconnect_shows_the_discovery_of_the_previous_session() {
    let mut state = UiState::default();
    connect(&mut state);
    discovered(&mut state, vec![pods(), widgets("v1")]);
    completed(&mut state, &[]);
    let mut commands = Vec::new();
    ClusterConnectionFailed {
        cluster_key: 1,
        error: "lost".to_owned(),
    }
    .apply(&mut state, &mut commands);

    state.retry_selected_load(1, &mut commands);

    assert!(
        shown(&state, "widgets"),
        "from the cache, not the built-in resources"
    );
    assert!(!shown(&state, "deployments"));
}

#[test]
fn connection_progress_times_each_step_once() {
    let mut state = UiState {
        cluster_selections: saved("core", "pods", SelectedNamespaces::All),
        now: 10.0,
        ..Default::default()
    };
    connect(&mut state);
    let cluster = state.clusters.get_mut(&1).unwrap();
    assert_eq!(cluster.connection_progress.started, 10.0);

    cluster.mark_authenticated(10.5);
    cluster.mark_authenticated(11.0);
    cluster.mark_rows_received(&deployments("v1"), 11.5);
    cluster.mark_reachable(12.0);
    cluster.mark_rows_received(&pods(), 12.5);
    cluster.mark_rows_received(&pods(), 13.0);

    assert_eq!(
        cluster.connection_progress,
        ConnectionProgress {
            started: 10.0,
            authenticated: Some(10.5),
            reachable: Some(12.0),
            first_rows: Some(12.5),
        }
    );
}

#[test]
fn an_inspector_opened_while_discovering_waits_for_the_metrics_api_to_be_decided() {
    let mut state = UiState {
        cluster_selections: saved("core", "pods", SelectedNamespaces::All),
        ..Default::default()
    };
    connect(&mut state);
    let mut commands = Vec::new();
    state.open_resource_detail(
        1,
        pods(),
        "api".to_owned(),
        Some("default".to_owned()),
        "uid".to_owned(),
        &mut commands,
    );
    let inspector = |state: &UiState| {
        state
            .global_blades
            .navigator()
            .unwrap()
            .entries()
            .find_map(|entry| entry.resource_detail())
            .map(|entry| entry.pod_metrics_api_unavailable)
            .unwrap()
    };
    assert!(!inspector(&state), "not yet known to be missing");
    assert!(
        commands_of::<StartResourceDetailWatch>(&commands)
            .all(|command| !command.pod_metrics_api_available)
    );

    discovered(&mut state, vec![pods()]);
    completed(&mut state, &[]);

    assert!(
        inspector(&state),
        "discovery completed without the Metrics API"
    );
}

#[test]
fn an_inspector_opened_while_discovering_collects_usage_once_the_metrics_api_is_found() {
    let pod_metrics = resource("metrics.k8s.io", "v1beta1", "PodMetrics", "pods");
    let mut state = UiState {
        cluster_selections: saved("core", "pods", SelectedNamespaces::All),
        ..Default::default()
    };
    connect(&mut state);
    state.open_resource_detail(
        1,
        pods(),
        "api".to_owned(),
        Some("default".to_owned()),
        "uid".to_owned(),
        &mut Vec::new(),
    );

    let commands = discovered(&mut state, vec![pods(), pod_metrics]);

    assert!(
        commands_of::<StartResourceDetailWatch>(&commands)
            .any(|command| command.pod_metrics_api_available),
        "the inspector's watch restarts with the Metrics API"
    );
    let entry = state
        .global_blades
        .navigator()
        .unwrap()
        .entries()
        .find_map(|entry| entry.resource_detail())
        .unwrap();
    assert!(entry.pod_metrics_watched && !entry.pod_metrics_api_unavailable);
}

#[test]
fn a_clusters_server_reaches_its_state_and_is_replaced_by_the_next_update() {
    let mut state = UiState::default();
    let update = |server: Option<&str>| {
        KubernetesClustersUpdated(vec![Cluster {
            name: "kind".to_owned(),
            is_current: true,
            presentation: Default::default(),
            server: server.map(str::to_owned),
        }])
    };
    update(Some("https://127.0.0.1:6443")).apply(&mut state, &mut Vec::new());
    assert_eq!(
        state
            .clusters
            .values()
            .next()
            .and_then(|c| c.server.as_deref()),
        Some("https://127.0.0.1:6443")
    );
    update(None).apply(&mut state, &mut Vec::new());
    assert_eq!(
        state
            .clusters
            .values()
            .next()
            .and_then(|c| c.server.as_deref()),
        None
    );
}
