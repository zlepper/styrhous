//! Kind deployment completion and action scenarios.

use super::*;

#[test]
fn test_force_delete_resource_with_finalizer_integration() {
    let fixture =
        IntegrationNamespaceFixture::create("force-delete", "force-delete-stuck", "value");
    let resource_name = fixture.name.clone();
    let runtime = &fixture.runtime;
    let configmaps = &fixture.configmaps;
    runtime.block_on(async {
        configmaps
            .patch(
                &resource_name,
                &Default::default(),
                &Patch::Merge(&k8s_openapi::serde_json::json!({
                    "metadata": { "finalizers": [TEST_FINALIZER] }
                })),
            )
            .await
            .expect("ConfigMap finalizer should be added");
        configmaps
            .delete(&resource_name, &Default::default())
            .await
            .expect("ConfigMap deletion should be accepted");
        let configmap = configmaps
            .get(&resource_name)
            .await
            .expect("Finalizer should keep ConfigMap present");
        assert!(configmap.metadata.deletion_timestamp.is_some());
        assert!(
            configmap
                .metadata
                .finalizers
                .as_ref()
                .is_some_and(|finalizers| finalizers == &[TEST_FINALIZER])
        );
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let configmaps_resource = select_resource(&mut harness, "Config", "Config Maps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        configmaps_resource.clone(),
        &fixture.namespace,
    );
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key].resource_cache
                [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
                .resources
                .values()
                .find(|resource| resource.name == resource_name)
                .filter(|resource| resource.can_force_delete())
                .map(|_| ())
        },
        10_000,
    );

    harness
        .get_by_label(&format!("More actions for {resource_name}"))
        .click_accesskit();
    harness.run_steps(1);
    harness
        .get_by_label("Force delete (remove finalizers)")
        .click_accesskit();
    harness.run_steps(1);
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_force_delete
                .as_ref()
                .filter(|pending| pending.confirmation_available_at <= std::time::Instant::now())
                .map(|_| ())
        },
        5_000,
    );
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::TextInput,
            &format!("Type {resource_name} to acknowledge that you are bypassing cleanup:"),
        )
        .click();
    harness.run_steps(1);
    harness
        .input_mut()
        .events
        .push(egui::Event::Text(resource_name.clone()));
    harness.run_steps(1);
    harness.get_by_label("Remove finalizers").click_accesskit();

    wait_for_with_diagnostic(
        &mut harness,
        |_| {
            runtime
                .block_on(async { configmaps.get(&resource_name).await })
                .err()
                .filter(|error| matches!(error, kube::Error::Api(response) if response.code == 404))
                .map(|_| ())
        },
        |app| {
            app.ui_state.clusters[&cluster_key]
                .force_delete_error
                .clone()
        },
        10_000,
    );
    wait_for_with_diagnostic(
        &mut harness,
        |app| {
            (!app.ui_state.clusters[&cluster_key].resource_cache
                [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
                .resources
                .values()
                .any(|resource| resource.name == resource_name))
            .then_some(())
        },
        |app| {
            app.ui_state.clusters[&cluster_key].resource_cache
                [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
                .error
                .as_ref()
                .map(|error| {
                    format!("ConfigMap watcher failed while waiting for deletion: {error}")
                })
        },
        10_000,
    );
}

/// Fetches the live Deployment OpenAPI schema from Kind and verifies completion inside an
/// existing `spec.selector.matchLabels` key after it has been partially edited.

#[test]
fn test_deployment_match_labels_completion_integration() {
    let fixture = IntegrationNamespaceFixture::create("deployment-completion", "anchor", "unused");
    let deployment_name = "deployment-completion".to_owned();
    let client = fixture.runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let deployments: Api<Deployment> = Api::namespaced(client, &fixture.namespace);
    fixture.runtime.block_on(async {
        deployments
            .create(
                &Default::default(),
                &Deployment {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(deployment_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    spec: Some(DeploymentSpec {
                        replicas: Some(1),
                        selector: LabelSelector {
                            match_labels: Some(BTreeMap::from([(
                                "app".to_owned(),
                                deployment_name.clone(),
                            )])),
                            ..Default::default()
                        },
                        template: PodTemplateSpec {
                            metadata: Some(
                                k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                                    labels: Some(BTreeMap::from([(
                                        "app".to_owned(),
                                        deployment_name.clone(),
                                    )])),
                                    ..Default::default()
                                },
                            ),
                            spec: Some(PodSpec {
                                containers: vec![Container {
                                    name: "pause".to_owned(),
                                    image: Some("registry.k8s.io/pause:3.10".to_owned()),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }),
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create integration Deployment");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, "Apps & Containers", "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        deployments_resource,
        &fixture.namespace,
    );

    let actions_label = format!("More actions for {deployment_name}");
    harness.get_by_label(&actions_label).click();
    harness.run_steps(1);
    harness.get_by_label("Edit").click_accesskit();
    harness.run_steps(1);
    let (schema, yaml) = wait_for(
        &mut harness,
        |app| {
            app.ui_state
                .yaml_editors
                .values()
                .find(|editor| {
                    editor.resource_name == deployment_name
                        && !editor.loading
                        && editor.original_yaml.is_some()
                })
                .and_then(|editor| {
                    editor
                        .schema
                        .clone()
                        .map(|schema| (schema, editor.edited_yaml.clone()))
                })
        },
        10_000,
    );

    let key_start = yaml
        .find("matchLabels")
        .expect("live Deployment YAML includes spec.selector.matchLabels");
    let mut partial_yaml = yaml;
    partial_yaml.replace_range(key_start..key_start + "matchLabels".len(), "match");
    let cursor = partial_yaml[..key_start + "match".len()].chars().count();
    let suggestions = schema.completion_at(&partial_yaml, cursor).suggestions;

    assert_eq!(
        suggestions
            .first()
            .map(|suggestion| suggestion.label.as_str()),
        Some("matchLabels"),
        "suggestions: {suggestions:#?}\npartial YAML:\n{partial_yaml}"
    );

    let affinity_yaml = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: deployment-completion
spec:
  template:
    spec:
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
          - podAffinityTerm:
              labelSelector:
                matchExpressions:
                - key: k8s-app
                  operator: I"#;
    let suggestions = schema
        .completion_at(affinity_yaml, affinity_yaml.len())
        .suggestions;
    assert_eq!(
        suggestions
            .first()
            .map(|suggestion| suggestion.label.as_str()),
        Some("In"),
        "suggestions: {suggestions:#?}\nYAML:\n{affinity_yaml}"
    );
}

/// Opens the installed CoreDNS Deployment through the real editor and checks the completion
/// context at every mapping key in its live YAML.

#[test]
fn test_coredns_deployment_property_completion_integration() {
    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, "kube-system");
    let deployments_resource = select_resource(&mut harness, "Apps & Containers", "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        deployments_resource,
        "kube-system",
    );

    harness
        .get_by_label("More actions for coredns")
        .click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Edit").click_accesskit();
    harness.run_steps(1);
    let (schema, yaml) = wait_for(
        &mut harness,
        |app| {
            app.ui_state
                .yaml_editors
                .values()
                .find(|editor| {
                    editor.resource_name == "coredns"
                        && !editor.loading
                        && editor.original_yaml.is_some()
                })
                .and_then(|editor| {
                    editor
                        .schema
                        .clone()
                        .map(|schema| (schema, editor.edited_yaml.clone()))
                })
        },
        10_000,
    );

    let failures = yaml_mapping_key_positions(&yaml)
        .into_iter()
        .filter_map(|(line, key, cursor)| {
            let completion = schema.completion_at(&yaml, cursor);
            completion
                .context
                .is_none()
                .then_some((line, key, completion.suggestions))
        })
        .collect::<Vec<_>>();

    assert!(
        failures.is_empty(),
        "each CoreDNS mapping key should resolve to a schema completion context:\n{failures:#?}\nYAML:\n{yaml}"
    );
}

/// Verifies that the Deployment action patches the pod template annotation used
/// by `kubectl rollout restart` against a real Kubernetes API server.
#[test]
fn test_deployment_rollout_restart_integration() {
    let fixture =
        IntegrationNamespaceFixture::create("deployment-rollout-restart", "anchor", "unused");
    let deployment_name = "restartable-deployment".to_owned();
    let runtime = &fixture.runtime;
    let client = runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let deployments: Api<Deployment> = Api::namespaced(client, &fixture.namespace);
    runtime.block_on(async {
        deployments
            .create(
                &Default::default(),
                &Deployment {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(deployment_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    spec: Some(DeploymentSpec {
                        replicas: Some(1),
                        selector: LabelSelector {
                            match_labels: Some(BTreeMap::from([(
                                "app".to_owned(),
                                deployment_name.clone(),
                            )])),
                            ..Default::default()
                        },
                        template: PodTemplateSpec {
                            metadata: Some(
                                k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                                    labels: Some(BTreeMap::from([(
                                        "app".to_owned(),
                                        deployment_name.clone(),
                                    )])),
                                    ..Default::default()
                                },
                            ),
                            spec: Some(PodSpec {
                                containers: vec![Container {
                                    name: "pause".to_owned(),
                                    image: Some("registry.k8s.io/pause:3.10".to_owned()),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }),
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create Deployment");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, "Apps & Containers", "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        deployments_resource,
        &fixture.namespace,
    );
    for _ in 0..3 {
        harness.run_steps(1);
    }
    let actions_label = format!("More actions for {deployment_name}");
    harness.get_by_label(&actions_label).click();
    wait_for_harness(
        &mut harness,
        |harness| {
            harness
                .query_by_role_and_label(egui::accesskit::Role::Button, "Restart rollout")
                .map(|_| ())
        },
        5_000,
    );
    harness.get_by_label("Restart rollout").click();
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_deployment_restart
                .as_ref()
                .filter(|pending| pending.resource_name == deployment_name)
                .map(|_| ())
        },
        5_000,
    );
    wait_for_harness(
        &mut harness,
        |harness| {
            let mut buttons = harness
                .query_all_by_role_and_label(egui::accesskit::Role::Button, "Restart rollout");
            let button = buttons.next()?;
            (buttons.next().is_none() && !button.accesskit_node().is_disabled()).then_some(())
        },
        5_000,
    );
    harness.get_by_label("Restart rollout").click();

    wait_for_with_diagnostic(
        &mut harness,
        |_| {
            runtime
                .block_on(async { deployments.get(&deployment_name).await })
                .ok()
                .and_then(|deployment| {
                    deployment
                        .spec
                        .and_then(|spec| spec.template.metadata)
                        .and_then(|metadata| metadata.annotations)
                        .and_then(|annotations| {
                            annotations
                                .get("kubectl.kubernetes.io/restartedAt")
                                .cloned()
                        })
                })
                .filter(|timestamp| {
                    time::OffsetDateTime::parse(
                        timestamp,
                        &time::format_description::well_known::Rfc3339,
                    )
                    .is_ok()
                })
                .map(|_| ())
        },
        |app| {
            app.ui_state.clusters[&cluster_key]
                .deployment_restart_error
                .clone()
        },
        10_000,
    );
}
