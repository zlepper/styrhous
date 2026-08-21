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
