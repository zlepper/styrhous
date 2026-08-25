use super::*;

#[test]
fn metrics_availability_requires_the_matching_served_resource() {
    let resources = vec![ApiResource {
        group: "metrics.k8s.io".into(),
        version: "v1beta1".into(),
        kind: "PodMetrics".into(),
        name: "pods".into(),
        namespaced: true,
    }];
    assert!(pod_metrics_api_available(&resources));
    assert!(!node_metrics_api_available(&resources));

    let missing_metrics = vec![ApiResource {
        group: "metrics.k8s.io".into(),
        version: "v1beta1".into(),
        kind: "NodeMetrics".into(),
        name: "nodes".into(),
        namespaced: false,
    }];
    assert!(!pod_metrics_api_available(&missing_metrics));
    assert!(node_metrics_api_available(&missing_metrics));
}

#[test]
fn metrics_api_404_is_terminal() {
    let error = anyhow::Error::new(kube::Error::Api(
        kube::core::Status::failure(
            "the server could not find the requested resource",
            "NotFound",
        )
        .with_code(404)
        .boxed(),
    ));

    assert!(is_metrics_api_not_found(&error));
}

#[test]
fn missing_pod_metric_sample_is_not_an_unavailable_metrics_api() {
    let response = kube::core::Status {
        code: 404,
        details: Some(kube::core::response::StatusDetails {
            group: "metrics.k8s.io".into(),
            name: "api".into(),
            kind: "PodMetrics".into(),
            uid: String::new(),
            causes: Vec::new(),
            retry_after_seconds: 0,
        }),
        ..Default::default()
    };

    assert!(is_metric_sample_missing(&response, "api"));
    assert!(!is_metrics_api_not_found(&anyhow::Error::new(
        kube::Error::Api(response.boxed(),)
    )));
}

#[test]
fn missing_node_metric_sample_is_not_an_unavailable_metrics_api() {
    let response = kube::core::Status {
        code: 404,
        details: Some(kube::core::response::StatusDetails {
            group: "metrics.k8s.io".into(),
            name: "worker-a".into(),
            // Kubernetes status details use the resource name, not the GVK kind.
            kind: "nodes".into(),
            uid: String::new(),
            causes: Vec::new(),
            retry_after_seconds: 0,
        }),
        ..Default::default()
    };

    assert!(is_metric_sample_missing(&response, "worker-a"));
    assert!(!is_metric_sample_missing(&response, "worker-b"));
    assert!(!is_metrics_api_not_found(&anyhow::Error::new(
        kube::Error::Api(response.boxed(),)
    )));
}

#[test]
fn resource_owners_preserve_all_references_and_identify_the_controller() {
    let metadata = ObjectMeta {
        owner_references: Some(vec![
            OwnerReference {
                api_version: "example.dev/v1".into(),
                kind: "Backup".into(),
                name: "api-backup".into(),
                uid: "backup-uid".into(),
                controller: Some(false),
                block_owner_deletion: None,
            },
            OwnerReference {
                api_version: "apps/v1".into(),
                kind: "ReplicaSet".into(),
                name: "api-7b948f".into(),
                uid: "replicaset-uid".into(),
                controller: Some(true),
                block_owner_deletion: None,
            },
        ]),
        ..Default::default()
    };

    let owners = resource_owners(&metadata);
    let dynamic_resource = extract_minimal_resource(
        &DynamicObject {
            types: None,
            metadata: metadata.clone(),
            data: k8s_openapi::serde_json::json!({}),
        },
        &[],
    );
    let typed_resource = crate::minimal_resource::from_kubernetes_resource(
        &Pod {
            metadata: metadata.clone(),
            ..Default::default()
        },
        BTreeMap::new(),
    );

    assert_eq!(owners.len(), 2);
    assert_eq!(owners[0].label(), "Backup / api-backup");
    assert_eq!(owners[1].uid, "replicaset-uid");
    assert_eq!(
        controller_owner(&metadata).map(|owner| owner.name),
        Some("api-7b948f".into())
    );
    assert_eq!(
        dynamic_resource.controller_owner.map(|owner| owner.name),
        Some("api-7b948f".into())
    );
    assert_eq!(
        typed_resource.controller_owner.map(|owner| owner.name),
        Some("api-7b948f".into())
    );
    assert_eq!(
        owners
            .into_iter()
            .find(|owner| owner.controller)
            .map(|owner| owner.name),
        Some("api-7b948f".into())
    );
}

#[test]
fn scale_capability_requires_get_and_patch_on_the_parent_subresource() {
    let resources = vec![
        APIResource {
            name: "deployments/scale".into(),
            verbs: vec!["get".into(), "patch".into()],
            ..Default::default()
        },
        APIResource {
            name: "deployments/status".into(),
            verbs: vec!["get".into(), "patch".into()],
            ..Default::default()
        },
        APIResource {
            name: "statefulsets/scale".into(),
            verbs: vec!["get".into()],
            ..Default::default()
        },
    ];

    assert!(supports_scale_subresource(&resources, "deployments"));
    assert!(!supports_scale_subresource(&resources, "statefulsets"));
    assert!(!supports_scale_subresource(&resources, "replicasets"));
}

#[test]
fn environment_variable_expansion_uses_earlier_values_and_preserves_unknown_references() {
    let mut variables = vec![
        PodEnvironmentVariableDetail {
            name: "HOST".to_owned(),
            value: Some("api".to_owned()),
            source: PodEnvironmentVariableSource::Literal,
        },
        PodEnvironmentVariableDetail {
            name: "URL".to_owned(),
            value: Some("https://$(HOST)/$(SERVICE_PORT)".to_owned()),
            source: PodEnvironmentVariableSource::Literal,
        },
        PodEnvironmentVariableDetail {
            name: "ESCAPED".to_owned(),
            value: Some("$$(HOST)".to_owned()),
            source: PodEnvironmentVariableSource::Literal,
        },
        PodEnvironmentVariableDetail {
            name: "SHELL_STYLE".to_owned(),
            value: Some("$HOST".to_owned()),
            source: PodEnvironmentVariableSource::Literal,
        },
    ];

    expand_environment_variable_references(&mut variables);

    assert_eq!(
        variables[1].value.as_deref(),
        Some("https://api/$(SERVICE_PORT)")
    );
    assert_eq!(variables[2].value.as_deref(), Some("$(HOST)"));
    assert_eq!(variables[3].value.as_deref(), Some("$HOST"));
}

#[test]
fn environment_variable_resolution_expands_config_map_and_secret_imports() {
    let config_maps = BTreeMap::from([(
        "settings".to_owned(),
        ConfigMap {
            data: Some(BTreeMap::from([("HOST".to_owned(), "api".to_owned())])),
            ..Default::default()
        },
    )]);
    let secrets = BTreeMap::from([(
        "credentials".to_owned(),
        Secret {
            data: Some(BTreeMap::from([(
                "token".to_owned(),
                k8s_openapi::ByteString(b"secret-value".to_vec()),
            )])),
            ..Default::default()
        },
    )]);
    let variables = [
        PodEnvironmentVariableDetail {
            name: "CONFIG".to_owned(),
            value: None,
            source: PodEnvironmentVariableSource::ConfigMapKey {
                name: "settings".to_owned(),
                key: "HOST".to_owned(),
                optional: false,
            },
        },
        PodEnvironmentVariableDetail {
            name: "Import Secret credentials".to_owned(),
            value: None,
            source: PodEnvironmentVariableSource::SecretImport {
                name: "credentials".to_owned(),
                prefix: "APP_".to_owned(),
                optional: false,
            },
        },
    ];

    let resolved = variables
        .into_iter()
        .flat_map(|variable| resolve_environment_variable(variable, &config_maps, &secrets))
        .collect::<Vec<_>>();

    assert_eq!(resolved[0].value.as_deref(), Some("api"));
    assert_eq!(resolved[1].name, "APP_token");
    assert_eq!(resolved[1].value.as_deref(), Some("secret-value"));
    assert!(matches!(
        resolved[1].source,
        PodEnvironmentVariableSource::SecretKey { .. }
    ));
}

#[test]
fn environment_variable_resolution_applies_kubernetes_precedence() {
    let config_maps = BTreeMap::from([
        (
            "defaults".to_owned(),
            ConfigMap {
                data: Some(BTreeMap::from([
                    ("LOG_LEVEL".to_owned(), "info".to_owned()),
                    ("PORT".to_owned(), "3000".to_owned()),
                ])),
                ..Default::default()
            },
        ),
        (
            "overrides".to_owned(),
            ConfigMap {
                data: Some(BTreeMap::from([("PORT".to_owned(), "4000".to_owned())])),
                ..Default::default()
            },
        ),
    ]);
    let variables = vec![
        PodEnvironmentVariableDetail {
            name: "Import ConfigMap defaults".to_owned(),
            value: None,
            source: PodEnvironmentVariableSource::ConfigMapImport {
                name: "defaults".to_owned(),
                prefix: String::new(),
                optional: false,
            },
        },
        PodEnvironmentVariableDetail {
            name: "Import ConfigMap overrides".to_owned(),
            value: None,
            source: PodEnvironmentVariableSource::ConfigMapImport {
                name: "overrides".to_owned(),
                prefix: String::new(),
                optional: false,
            },
        },
        PodEnvironmentVariableDetail {
            name: "URL".to_owned(),
            value: Some("http://$(PORT)".to_owned()),
            source: PodEnvironmentVariableSource::Literal,
        },
        PodEnvironmentVariableDetail {
            name: "PORT".to_owned(),
            value: Some("8080".to_owned()),
            source: PodEnvironmentVariableSource::Literal,
        },
    ];

    let resolved = resolve_environment_variables(variables, &config_maps, &BTreeMap::new());

    assert_eq!(
        resolved
            .iter()
            .filter(|variable| variable.name == "PORT")
            .map(|variable| variable.value.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("8080")]
    );
    assert!(resolved.iter().any(|variable| {
        variable.name == "LOG_LEVEL" && variable.value.as_deref() == Some("info")
    }));
    assert!(resolved.iter().any(|variable| {
        variable.name == "URL" && variable.value.as_deref() == Some("http://4000")
    }));
}
