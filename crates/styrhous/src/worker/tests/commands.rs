use super::*;

#[test]
fn sensitive_commands_omit_resource_data_values_from_debug_output() {
    let command = UpdateResourceData {
        cluster_key: 7,
        history_entry_id: 12,
        request_id: 34,
        api_resource: pod_resource(),
        namespace: "default".to_owned(),
        resource_name: "credentials".to_owned(),
        update: ResourceDataUpdate {
            expected_resource_version: "42".to_owned(),
            expected_values: BTreeMap::from([("token".to_owned(), "old-secret".to_owned())]),
            updated_values: BTreeMap::from([("token".to_owned(), "new-secret".to_owned())]),
        },
    };
    assert!(!format!("{command:?}").contains("secret"));
}

#[test]
fn yaml_commands_omit_document_text_from_debug_output() {
    let secret_yaml = "data:\n  token: definitely-secret".to_owned();
    let apply = ApplyResourceYaml {
        editor_id: 9,
        cluster_key: 7,
        api_resource: pod_resource(),
        namespace: Some("default".to_owned()),
        resource_name: "credentials".to_owned(),
        yaml: secret_yaml.clone(),
    };
    let validation = ValidateResourceYaml {
        editor_id: 9,
        revision: 4,
        cluster_key: 7,
        api_resource: pod_resource(),
        namespace: Some("default".to_owned()),
        resource_name: "credentials".to_owned(),
        yaml: secret_yaml,
    };
    assert!(!format!("{apply:?}{validation:?}").contains("definitely-secret"));
}

#[test]
fn cluster_lifecycle_commands_are_serialized_and_watch_reconciliation_is_scoped() {
    let load_clusters: WorkerCommandBox = Box::new(LoadClusters);
    assert!(load_clusters.serializes_session_lifecycle());
    assert_eq!(load_clusters.cluster_key(), None);
    let stop_logs = StopPodLogStream {
        cluster_key: 1,
        log_window_id: 1,
    };
    let stop_logs: WorkerCommandBox = Box::new(stop_logs);
    assert!(!stop_logs.serializes_session_lifecycle());
    assert_eq!(stop_logs.cluster_key(), Some(1));
    let reconcile: WorkerCommandBox = Box::new(ReconcileResourceWatches {
        cluster_key: 1,
        api_resource: pod_resource(),
        sources: vec![ResourceWatchSource::Namespace("default".to_owned())],
    });
    assert!(!reconcile.serializes_session_lifecycle());
    assert_eq!(reconcile.cluster_key(), Some(1));
    let get_yaml = GetResourceYaml {
        editor_id: 1,
        cluster_key: 1,
        api_resource: pod_resource(),
        namespace: Some("default".to_owned()),
        resource_name: "pod".to_owned(),
    };
    let get_yaml: WorkerCommandBox = Box::new(get_yaml);
    assert!(!get_yaml.serializes_session_lifecycle());
    assert_eq!(get_yaml.cluster_key(), Some(1));
}

#[test]
fn watch_initialization_slots_are_limited_per_cluster() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime initializes");
    runtime.block_on(async {
        let state = worker_state();
        let first = state.watch_initialization_slot(1).await;
        let second = state.watch_initialization_slot(2).await;
        let permits = (0..16)
            .map(|_| {
                first
                    .clone()
                    .try_acquire_owned()
                    .expect("slot is available")
            })
            .collect::<Vec<_>>();
        assert!(first.try_acquire().is_err());
        assert!(second.try_acquire().is_ok());
        drop(permits);
        assert!(first.try_acquire().is_ok());
    });
}
