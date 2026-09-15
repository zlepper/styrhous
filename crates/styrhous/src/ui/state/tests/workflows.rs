use super::*;

#[test]
fn pod_log_windows_route_each_stream_by_its_window_id() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "api".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );
    state.open_pod_log_window(
        7,
        "api-pod".into(),
        Some("default".into()),
        PodLogContainer {
            name: "sidecar".into(),
            kind: ContainerKind::App,
            image: None,
        },
        &mut commands,
    );

    assert_eq!(commands.len(), 2);
    assert_eq!(
        commands[0]
            .as_ref()
            .as_any()
            .downcast_ref::<StartPodLogStream>()
            .map(|command| command.log_window_id),
        Some(1)
    );
    assert_eq!(
        commands[1]
            .as_ref()
            .as_any()
            .downcast_ref::<StartPodLogStream>()
            .map(|command| command.log_window_id),
        Some(2)
    );

    let mut worker = MockWorker {
        results: VecDeque::from([
            Box::new(PodLogStreamStarted { log_window_id: 1 }) as WorkerResultBox,
            Box::new(PodLogStreamStarted { log_window_id: 2 }) as WorkerResultBox,
            Box::new(PodLogStreamEnded { log_window_id: 1 }) as WorkerResultBox,
        ]),
        commands: Vec::new(),
    };
    let _ = state.update(&mut worker);
    state.apply_log_store_result(LogStoreResult::Updated {
        window_id: 2,
        total_lines: 1,
        completed_search: None,
        appended_rows: Vec::new(),
        backfill_lines: None,
    });
    state.apply_log_store_result(LogStoreResult::Updated {
        window_id: 1,
        total_lines: 2,
        completed_search: None,
        appended_rows: Vec::new(),
        backfill_lines: None,
    });

    assert_eq!(state.log_windows[&1].total_lines, 2);
    assert_eq!(state.log_windows[&1].status, PodLogStatus::Finished);
    assert_eq!(state.log_windows[&2].total_lines, 1);
    assert_eq!(state.log_windows[&2].status, PodLogStatus::Following);
}

#[test]
fn pod_log_window_routes_all_selected_sources_through_one_worker_command() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.request_pod_log_window(
        7,
        vec![
            PodLogStreamTarget {
                namespace: "payments".into(),
                pod_name: "api-0".into(),
                container: "server".into(),
            },
            PodLogStreamTarget {
                namespace: "payments".into(),
                pod_name: "worker-0".into(),
                container: "worker".into(),
            },
        ],
        &mut commands,
    );

    assert_eq!(state.log_windows.len(), 1);
    assert!(state.log_windows[&1].has_multiple_sources());
    assert!(state.log_windows[&1].show_source_labels);
    let command = commands[0]
        .as_ref()
        .as_any()
        .downcast_ref::<StartPodLogStream>()
        .expect("selected sources start a Pod log stream");
    assert_eq!(command.log_window_id, 1);
    assert_eq!(command.targets.len(), 2);
    assert_eq!(
        command.targets[1].display_name(),
        "payments/worker-0 · worker"
    );
}

#[test]
fn pod_log_window_requires_confirmation_above_ten_sources() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.request_pod_log_window(
        7,
        (0..11)
            .map(|index| PodLogStreamTarget {
                namespace: "payments".into(),
                pod_name: format!("worker-{index}"),
                container: "worker".into(),
            })
            .collect(),
        &mut commands,
    );

    assert!(commands.is_empty());
    assert!(state.log_windows.is_empty());
    assert_eq!(
        state
            .pending_log_sources
            .as_ref()
            .map(|pending| pending.targets.len()),
        Some(11)
    );
}

#[test]
fn pod_log_source_failures_are_grouped_by_source() {
    let mut state = UiState::default();
    let mut commands = Vec::new();
    let target = PodLogStreamTarget {
        namespace: "payments".into(),
        pod_name: "api-0".into(),
        container: "server".into(),
    };
    state.request_pod_log_window(7, vec![target.clone()], &mut commands);
    assert!(!state.log_windows[&1].show_source_labels);

    PodLogSourceFailed {
        log_window_id: 1,
        target: target.clone(),
        error: "follow request was forbidden".into(),
    }
    .apply(&mut state, &mut commands);
    PodLogSourceFailed {
        log_window_id: 1,
        target,
        error: "history request was forbidden".into(),
    }
    .apply(&mut state, &mut commands);

    assert_eq!(state.log_windows[&1].source_failures.len(), 1);
    assert_eq!(
        state.log_windows[&1]
            .source_failures
            .values()
            .next()
            .expect("failure is retained"),
        "follow request was forbidden\nhistory request was forbidden"
    );
}

#[test]
fn cluster_reload_ignores_resource_events_from_the_retired_cluster_key() {
    let api_resource = ApiResource {
        group: "core".into(),
        version: "v1".into(),
        kind: "Pod".into(),
        name: "pods".into(),
        namespaced: true,
    };
    let stale_resource = MinimalResource {
        uid: "stale".into(),
        name: "stale-pod".into(),
        namespace: Some("default".into()),
        creation_timestamp: None,
        controller_owner: None,
        labels: BTreeMap::new(),
        annotations: BTreeMap::new(),
        cells: BTreeMap::new(),
        log_containers: Vec::new(),
    };
    let mut state = UiState::default();
    let mut worker = MockWorker {
        results: VecDeque::from([
            Box::new(KubernetesClustersUpdated(vec![Cluster {
                name: "old".into(),
                is_current: true,
            }])) as WorkerResultBox,
            Box::new(KubernetesClustersUpdated(vec![Cluster {
                name: "new".into(),
                is_current: true,
            }])) as WorkerResultBox,
            Box::new(KubernetesResourceAdded {
                cluster_key: 1,
                api_resource,
                namespace: Some("default".into()),
                resource: stale_resource,
            }) as WorkerResultBox,
        ]),
        commands: Vec::new(),
    };

    let _ = state.update(&mut worker);

    assert_eq!(state.selected_cluster, Some(2));
    assert_eq!(state.clusters.len(), 1);
    assert_eq!(state.clusters[&2].name, "new");
    assert!(state.clusters[&2].resource_cache.is_empty());
}

#[test]
fn yaml_editors_are_deduplicated_and_route_results_by_editor_id() {
    let ctx = egui::Context::default();
    let api_resource = ApiResource {
        group: "core".into(),
        version: "v1".into(),
        kind: "ConfigMap".into(),
        name: "configmaps".into(),
        namespaced: true,
    };
    let mut state = UiState::default();
    let mut commands = Vec::new();

    state.open_yaml_editor(
        &ctx,
        7,
        api_resource.clone(),
        Some("default".into()),
        "settings".into(),
        &mut commands,
    );
    state.open_yaml_editor(
        &ctx,
        7,
        api_resource.clone(),
        Some("default".into()),
        "settings".into(),
        &mut commands,
    );
    state.open_yaml_editor(
        &ctx,
        7,
        api_resource.clone(),
        Some("default".into()),
        "other-settings".into(),
        &mut commands,
    );

    assert_eq!(commands.len(), 4);
    assert_eq!(
        commands[0]
            .as_ref()
            .as_any()
            .downcast_ref::<GetResourceYaml>()
            .map(|command| command.editor_id),
        Some(1)
    );
    assert_eq!(
        commands[1]
            .as_ref()
            .as_any()
            .downcast_ref::<LoadResourceSchema>()
            .map(|command| command.editor_id),
        Some(1)
    );
    assert_eq!(
        commands[2]
            .as_ref()
            .as_any()
            .downcast_ref::<GetResourceYaml>()
            .map(|command| command.editor_id),
        Some(2)
    );
    assert_eq!(
        commands[3]
            .as_ref()
            .as_any()
            .downcast_ref::<LoadResourceSchema>()
            .map(|command| command.editor_id),
        Some(2)
    );
    assert!(state.yaml_editors[&1].focus_requested);

    let mut worker = MockWorker {
        results: VecDeque::from([
            Box::new(ResourceYamlFetched {
                editor_id: 2,
                cluster_key: 7,
                api_resource: api_resource.clone(),
                namespace: Some("default".into()),
                resource_name: "other-settings".into(),
                yaml: "kind: ConfigMap\nmetadata:\n  name: other-settings".into(),
            }) as WorkerResultBox,
            Box::new(ResourceYamlFetched {
                editor_id: 1,
                cluster_key: 7,
                api_resource: api_resource.clone(),
                namespace: Some("default".into()),
                resource_name: "settings".into(),
                yaml: "kind: ConfigMap\nmetadata:\n  name: settings".into(),
            }) as WorkerResultBox,
        ]),
        commands: Vec::new(),
    };
    state.update(&mut worker);

    assert_eq!(state.yaml_editors[&1].resource_name, "settings");
    assert_eq!(state.yaml_editors[&2].resource_name, "other-settings");
    assert!(
        state
            .yaml_editors
            .values()
            .all(|editor| !editor.loading && editor.original_yaml.is_some())
    );
}

#[test]
fn resource_data_completion_updates_only_the_initiating_history_entry() {
    let config_maps = ApiResource {
        group: "core".into(),
        version: "v1".into(),
        kind: "ConfigMap".into(),
        name: "configmaps".into(),
        namespaced: true,
    };
    let secrets = ApiResource {
        group: "core".into(),
        version: "v1".into(),
        kind: "Secret".into(),
        name: "secrets".into(),
        namespaced: true,
    };
    let mut state = UiState::default();
    let mut commands = Vec::new();
    let mut setup_worker = MockWorker {
        results: VecDeque::from([Box::new(KubernetesClustersUpdated(vec![Cluster {
            name: "kind".into(),
            is_current: true,
        }])) as WorkerResultBox]),
        commands: Vec::new(),
    };
    state.update(&mut setup_worker);
    state.open_resource_detail(
        1,
        config_maps.clone(),
        "settings".into(),
        Some("default".into()),
        "config-map-uid".into(),
        &mut commands,
    );
    state.navigate_resource_detail(
        1,
        secrets,
        "settings".into(),
        Some("default".into()),
        "secret-uid".into(),
        &mut commands,
    );
    let navigator = state
        .global_blades
        .navigator_mut()
        .expect("detail panel is open");
    let config_map_history_entry_id = navigator
        .entries()
        .filter_map(|entry| entry.resource_detail())
        .find(|entry| entry.api_resource == config_maps)
        .expect("ConfigMap history entry exists")
        .history_entry_id;
    for entry in navigator.entries_mut() {
        let entry = entry
            .resource_detail_mut()
            .expect("the test only creates resource detail content");
        entry.data_editor = Some(ResourceDataEditorState {
            saving: true,
            pending_save_request_id: Some(2),
            ..ResourceDataEditorState::new(BTreeMap::new(), "1".into())
        });
    }

    let mut worker = MockWorker {
        results: VecDeque::from([
            Box::new(ResourceDataUpdateCompleted {
                cluster_key: 1,
                history_entry_id: config_map_history_entry_id,
                request_id: 999,
            }) as WorkerResultBox,
            Box::new(ResourceDataUpdateCompleted {
                cluster_key: 1,
                history_entry_id: config_map_history_entry_id,
                request_id: 2,
            }) as WorkerResultBox,
        ]),
        commands: Vec::new(),
    };
    state.update(&mut worker);

    let navigator = state
        .global_blades
        .navigator()
        .expect("detail panel is open");
    let config_map_editor = navigator
        .entries()
        .filter_map(|entry| entry.resource_detail())
        .find(|entry| entry.history_entry_id == config_map_history_entry_id)
        .and_then(|entry| entry.data_editor.as_ref())
        .expect("config map editor exists");
    assert!(!config_map_editor.saving);
    assert!(
        navigator
            .current()
            .resource_detail()
            .and_then(|entry| entry.data_editor.as_ref())
            .expect("secret editor exists")
            .saving
    );

    for entry in state
        .global_blades
        .navigator_mut()
        .expect("detail panel is open")
        .entries_mut()
    {
        let editor = entry
            .resource_detail_mut()
            .expect("the test only creates resource detail content")
            .data_editor
            .as_mut()
            .expect("editor exists");
        editor.saving = true;
        editor.pending_save_request_id = Some(3);
        editor.save_error = None;
    }
    let mut worker = MockWorker {
        results: VecDeque::from([Box::new(ResourceDataUpdateFailed {
            cluster_key: 1,
            history_entry_id: config_map_history_entry_id,
            request_id: 3,
            error: "stale update failed".into(),
        }) as WorkerResultBox]),
        commands: Vec::new(),
    };
    state.update(&mut worker);

    assert_eq!(
        state
            .global_blades
            .navigator()
            .expect("detail panel is open")
            .entries()
            .filter_map(|entry| entry.resource_detail())
            .find(|entry| entry.history_entry_id == config_map_history_entry_id)
            .and_then(|entry| entry.data_editor.as_ref())
            .and_then(|editor| editor.save_error.as_deref()),
        Some("stale update failed")
    );
    assert_eq!(
        state
            .global_blades
            .navigator()
            .expect("detail panel is open")
            .current()
            .resource_detail()
            .and_then(|entry| entry.data_editor.as_ref())
            .expect("secret editor exists")
            .save_error,
        None
    );
}
