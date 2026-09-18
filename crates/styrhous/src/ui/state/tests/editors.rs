use super::*;

#[test]
fn api_status_causes_become_editor_diagnostics_for_validation_and_apply() {
    let ctx = egui::Context::default();
    let api_resource = ApiResource {
        group: "apps".into(),
        version: "v1".into(),
        kind: "Deployment".into(),
        name: "deployments".into(),
        namespaced: true,
    };
    let yaml = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: api\nspec:\n  template:\n    spec:\n      containers:\n        - name: api\n          image: invalid";
    let api_error = ResourceApiError {
        status_code: 422,
        message: "Deployment.apps \"api\" is invalid".into(),
        causes: vec![crate::worker::ResourceApiErrorCause {
            field: "spec.template.spec.containers[0].image".into(),
            message: "Invalid value: \"invalid\"".into(),
            reason: "FieldValueInvalid".into(),
        }],
    };
    let mut state = UiState::default();
    let mut commands = Vec::new();
    state.open_yaml_editor(
        &ctx,
        7,
        api_resource.clone(),
        Some("default".into()),
        "api".into(),
        &mut commands,
    );
    let mut worker = MockWorker {
        results: VecDeque::from([
            Box::new(ResourceYamlFetched {
                editor_id: 1,
                cluster_key: 7,
                api_resource: api_resource.clone(),
                namespace: Some("default".into()),
                resource_name: "api".into(),
                yaml: yaml.into(),
                resource_version: "1".into(),
                resource_uid: "uid-1".into(),
            }) as WorkerResultBox,
            Box::new(ResourceYamlValidationFailed {
                editor_id: 1,
                revision: 0,
                cluster_key: 7,
                api_resource: api_resource.clone(),
                namespace: Some("default".into()),
                resource_name: "api".into(),
                error: api_error.clone(),
            }) as WorkerResultBox,
            Box::new(ResourceApplyFailed {
                editor_id: 1,
                cluster_key: 7,
                api_resource,
                namespace: Some("default".into()),
                resource_name: "api".into(),
                error: api_error,
            }) as WorkerResultBox,
        ]),
        commands: Vec::new(),
    };

    state.update(&mut worker);

    let editor = &state.yaml_editors[&1];
    assert_eq!(
        editor.server_validation,
        ValidationState::Failed("Deployment.apps \"api\" is invalid".into())
    );
    assert_eq!(
        editor.error.as_deref(),
        Some("Deployment.apps \"api\" is invalid")
    );
    assert_eq!(editor.diagnostics.len(), 1);
    assert_eq!(editor.diagnostics[0].line, Some(10));
    assert!(editor.diagnostics[0].range.is_some());
    assert_eq!(
        editor.diagnostics[0].message,
        "spec.template.spec.containers[0].image: Invalid value: \"invalid\""
    );
}

#[test]
fn conflict_errors_explain_how_to_recover_without_discarding_edits_automatically() {
    let error = ResourceApiError {
        status_code: 409,
        message: "the object has been modified".into(),
        causes: Vec::new(),
    };

    assert_eq!(
        api_error_message(&error),
        "This resource changed on the cluster. Discard your edits and reopen the editor before applying changes."
    );
}

#[test]
fn secret_mutation_failures_redact_notification_details() {
    let secret = ApiResource {
        group: "core".into(),
        version: "v1".into(),
        kind: "Secret".into(),
        name: "secrets".into(),
        namespaced: true,
    };
    let mut state = UiState::default();
    let mut cluster = ClusterState::for_test(7, "dev");
    cluster.connection = ClusterConnectionState::Connected;
    state.clusters.insert(7, cluster);
    let mut commands = Vec::new();

    ResourceApplyFailed {
        editor_id: 1,
        cluster_key: 7,
        api_resource: secret.clone(),
        namespace: Some("default".into()),
        resource_name: "credentials".into(),
        error: ResourceApiError {
            status_code: 403,
            message: "sentinel-apply-secret-value".into(),
            causes: Vec::new(),
        },
    }
    .apply(&mut state, &mut commands);
    ResourceDataUpdateFailed {
        cluster_key: 7,
        history_entry_id: 99,
        request_id: 1,
        api_resource: secret,
        namespace: "default".into(),
        resource_name: "credentials".into(),
        error: "sentinel-data-secret-value".into(),
    }
    .apply(&mut state, &mut commands);

    let details = state.clusters[&7]
        .operation_history
        .iter()
        .filter_map(|entry| entry.details.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(details.len(), 2);
    assert!(
        details
            .iter()
            .all(|detail| detail.contains("Details are hidden"))
    );
    assert!(
        details
            .iter()
            .all(|detail| !detail.contains("sentinel-apply-secret-value")
                && !detail.contains("sentinel-data-secret-value"))
    );
}
