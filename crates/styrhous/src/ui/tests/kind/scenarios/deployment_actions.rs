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
    let configmaps_resource =
        select_resource(&mut harness, Some("Config & secrets"), "Config Maps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &configmaps_resource,
        Some(&fixture.namespace),
    );
    wait_for_resource_watch(
        &mut harness,
        &format!("ConfigMap {resource_name} to become eligible for force deletion"),
        cluster_key,
        &configmaps_resource,
        Some(&fixture.namespace),
        |watch| {
            watch
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
        .click();
    harness.run_steps(1);
    harness
        .get_by_label("Force delete (remove finalizers)")
        .click();
    harness.run_steps(1);
    let acknowledgement_label = format!("Type {resource_name} to confirm");
    let mut previous_input_rect = None;
    wait_for_harness(
        &mut harness,
        "the force-delete acknowledgement input to settle after the countdown",
        |harness| {
            let pending = harness.state().ui_state.clusters[&cluster_key]
                .pending_force_delete
                .as_ref()?;
            if pending.confirmation_available_at > std::time::Instant::now() {
                return None;
            }
            // Removing the countdown changes the centered modal's layout. Observe
            // stable rendered coordinates before sending a real pointer click.
            let rect = harness
                .query_by_role_and_label(egui::accesskit::Role::TextInput, &acknowledgement_label)?
                .rect();
            (previous_input_rect.replace(rect) == Some(rect)).then_some(())
        },
        5_000,
    );
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, &acknowledgement_label)
        .click();
    wait_for_harness(
        &mut harness,
        "the force-delete acknowledgement input to receive pointer focus",
        |harness| {
            harness
                .query_by_role_and_label(egui::accesskit::Role::TextInput, &acknowledgement_label)?
                .is_focused()
                .then_some(())
        },
        5_000,
    );
    harness
        .input_mut()
        .events
        .push(egui::Event::Text(resource_name.clone()));
    wait_for_harness(
        &mut harness,
        "the acknowledged force-delete confirmation to become enabled",
        |harness| {
            let button = harness.query_by_label("Remove finalizers")?;
            (!button.accesskit_node().is_disabled()).then_some(())
        },
        5_000,
    );
    harness.get_by_label("Remove finalizers").click();
    wait_for(
        &mut harness,
        "the force-delete confirmation to dispatch and close",
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_force_delete
                .is_none()
                .then_some(())
        },
        5_000,
    );

    wait_for_kubernetes_with_diagnostic(
        &mut harness,
        &format!("ConfigMap {resource_name} to be deleted from Kubernetes"),
        |remaining| {
            kubernetes_object_absent(kubernetes_request(
                runtime,
                remaining,
                configmaps.get(&resource_name),
            ))
        },
        |app| latest_operation_failure(app, cluster_key),
        20_000,
    );
    wait_for_resource_watch(
        &mut harness,
        &format!("ConfigMap {resource_name} to disappear from the resource watch"),
        cluster_key,
        &configmaps_resource,
        Some(&fixture.namespace),
        |watch| {
            (!watch
                .resources
                .values()
                .any(|resource| resource.name == resource_name))
            .then_some(())
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
                &pause_deployment(&deployment_name, &fixture.namespace),
            )
            .await
            .expect("Failed to create integration Deployment");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, None, "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &deployments_resource,
        Some(&fixture.namespace),
    );

    let actions_label = format!("More actions for {deployment_name}");
    harness.get_by_label(&actions_label).click();
    harness.run_steps(1);
    wait_for_settled_clickable(&mut harness, "Edit YAML", 5_000);
    harness.get_by_label("Edit YAML").click();
    harness.run_steps(1);
    let (schema, yaml) = wait_for_yaml_editor_with_schema(&mut harness, &deployment_name, 10_000);

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
    let deployments_resource = select_resource(&mut harness, None, "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &deployments_resource,
        Some("kube-system"),
    );

    wait_for_settled_clickable(&mut harness, "More actions for coredns", 5_000);
    harness.get_by_label("More actions for coredns").click();
    harness.run_steps(1);
    wait_for_settled_clickable(&mut harness, "Edit YAML", 5_000);
    harness.get_by_label("Edit YAML").click();
    harness.run_steps(1);
    let (schema, yaml) = wait_for_yaml_editor_with_schema(&mut harness, "coredns", 10_000);

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
                &pause_deployment(&deployment_name, &fixture.namespace),
            )
            .await
            .expect("Failed to create Deployment");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, None, "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &deployments_resource,
        Some(&fixture.namespace),
    );
    for _ in 0..3 {
        harness.run_steps(1);
    }
    let actions_label = format!("More actions for {deployment_name}");
    harness.get_by_label(&actions_label).click();
    wait_for_harness(
        &mut harness,
        "the Restart rollout action to appear",
        |harness| {
            harness
                .query_by_role_and_label(egui::accesskit::Role::MenuItem, "Restart rollout")
                .map(|_| ())
        },
        5_000,
    );
    harness.get_by_label("Restart rollout").click();
    wait_for(
        &mut harness,
        "the restart rollout confirmation to open",
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
        "the restart rollout confirmation delay to elapse",
        |harness| {
            let mut buttons = harness
                .query_all_by_role_and_label(egui::accesskit::Role::Button, "Restart rollout");
            let button = buttons.next()?;
            (buttons.next().is_none() && !button.accesskit_node().is_disabled()).then_some(())
        },
        5_000,
    );
    harness.get_by_label("Restart rollout").click();

    wait_for_kubernetes_with_diagnostic(
        &mut harness,
        &format!("Deployment {deployment_name} to receive a restart annotation"),
        |remaining| {
            kubernetes_request(runtime, remaining, deployments.get(&deployment_name)).map(
                |deployment| {
                    deployment
                        .spec
                        .and_then(|spec| spec.template.metadata)
                        .and_then(|metadata| metadata.annotations)
                        .and_then(|annotations| {
                            annotations
                                .get("kubectl.kubernetes.io/restartedAt")
                                .cloned()
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
            )
        },
        |app| latest_operation_failure(app, cluster_key),
        10_000,
    );
}

/// Rolls a Deployment back to its first revision through "Roll back…", the
/// way `kubectl rollout undo` does, against a real Deployment controller.
#[test]
fn test_deployment_rollback_integration() {
    use crate::ui::state::DeploymentRevisionsLoad;
    use k8s_openapi::api::core::v1::EnvVar;

    let fixture = IntegrationNamespaceFixture::create("deployment-rollback", "anchor", "unused");
    let deployment_name = "rollback-deployment".to_owned();
    let runtime = &fixture.runtime;
    let client = runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let deployments: Api<Deployment> = Api::namespaced(client, &fixture.namespace);
    let with_greeting = |greeting: &str| {
        let mut deployment = pause_deployment(&deployment_name, &fixture.namespace);
        deployment
            .spec
            .as_mut()
            .and_then(|spec| spec.template.spec.as_mut())
            .expect("pause Deployment has a pod spec")
            .containers[0]
            .env = Some(vec![EnvVar {
            name: "GREETING".to_owned(),
            value: Some(greeting.to_owned()),
            ..Default::default()
        }]);
        deployment
    };
    let observed = |remaining| {
        kubernetes_request(runtime, remaining, deployments.get(&deployment_name)).map(
            |deployment| {
                let revision = deployment
                    .metadata
                    .annotations
                    .as_ref()
                    .and_then(|annotations| {
                        annotations
                            .get("deployment.kubernetes.io/revision")
                            .cloned()
                    });
                let greeting = deployment
                    .spec
                    .and_then(|spec| spec.template.spec)
                    .and_then(|spec| spec.containers.into_iter().next())
                    .and_then(|container| container.env)
                    .and_then(|env| env.into_iter().next())
                    .and_then(|env| env.value);
                (revision, greeting)
            },
        )
    };
    let (mut harness, cluster_key) = connected_kind_harness();
    runtime.block_on(async {
        deployments
            .create(&Default::default(), &with_greeting("first"))
            .await
            .expect("Failed to create Deployment");
    });
    wait_for_kubernetes(
        &mut harness,
        &format!("Deployment {deployment_name} revision 1"),
        |remaining| {
            observed(remaining)
                .map(|(revision, _)| (revision.as_deref() == Some("1")).then_some(()))
        },
        30_000,
    );
    runtime.block_on(async {
        deployments
            .replace(
                &deployment_name,
                &Default::default(),
                &with_greeting("second"),
            )
            .await
            .expect("Failed to update Deployment");
    });
    wait_for_kubernetes(
        &mut harness,
        &format!("Deployment {deployment_name} revision 2"),
        |remaining| {
            observed(remaining)
                .map(|(revision, _)| (revision.as_deref() == Some("2")).then_some(()))
        },
        30_000,
    );

    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, None, "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &deployments_resource,
        Some(&fixture.namespace),
    );
    let actions_label = format!("More actions for {deployment_name}");
    wait_for_settled_clickable(&mut harness, &actions_label, 5_000);
    harness.get_by_label(&actions_label).click();
    wait_for_settled_clickable(&mut harness, "Roll back…", 5_000);
    harness.get_by_label("Roll back…").click();
    let history = wait_for_with_terminal_and_timeout_diagnostic(
        &mut harness,
        "the rollback dialog to load revision 1 as the default target",
        |app| match &app.ui_state.clusters[&cluster_key]
            .pending_deployment_rollback
            .as_ref()?
            .revisions
        {
            DeploymentRevisionsLoad::Loaded {
                history,
                selected: Some(1),
            } => Some(history.clone()),
            _ => None,
        },
        |app| match &app.ui_state.clusters[&cluster_key]
            .pending_deployment_rollback
            .as_ref()?
            .revisions
        {
            DeploymentRevisionsLoad::Failed(error) => {
                Some(format!("loading revisions failed: {error}"))
            }
            _ => None,
        },
        |app| {
            Some(
                match &app.ui_state.clusters[&cluster_key].pending_deployment_rollback {
                    None => "the rollback dialog is not open".to_owned(),
                    Some(pending) => format!("the dialog shows {:#?}", pending.revisions),
                },
            )
        },
        // A GET and a LIST against an API server shared with the other
        // parallel Kind tests, so the same bound as the Kubernetes waits above.
        30_000,
    );
    // What the server really returns: defaulting must not make the
    // templates look different beyond the one change that was made.
    assert!(!history.paused);
    assert_eq!(
        history
            .revisions
            .iter()
            .map(|revision| (revision.revision, revision.is_current))
            .collect::<Vec<_>>(),
        vec![(2, true), (1, false)]
    );
    let previous = &history.revisions[1];
    assert_eq!(
        previous
            .changes
            .iter()
            .map(|change| (
                change.container.as_str(),
                change.field.as_str(),
                change.current.as_deref(),
                change.target.as_deref()
            ))
            .collect::<Vec<_>>(),
        vec![("pause", "env GREETING", Some("second"), Some("first"))]
    );
    assert!(!previous.other_changes);
    assert!(previous.created.is_some());
    wait_for_settled_clickable(&mut harness, "Roll back to revision 1", 5_000);
    harness.get_by_label("Roll back to revision 1").click();
    harness.run_steps(1);
    let cluster = &harness.state().ui_state.clusters[&cluster_key];
    assert!(
        cluster.pending_deployment_rollback.is_none()
            && !cluster.deployment_rollbacks.ids().is_empty(),
        "the rollback confirmation click was not applied"
    );

    wait_for_kubernetes_with_diagnostic(
        &mut harness,
        &format!("Deployment {deployment_name} to run revision 1's template as revision 3"),
        |remaining| {
            observed(remaining).map(|observed| {
                (observed == (Some("3".to_owned()), Some("first".to_owned()))).then_some(())
            })
        },
        |app| latest_operation_failure(app, cluster_key),
        20_000,
    );
    wait_for(
        &mut harness,
        "the rollback outcome to reach the notification history",
        |app| {
            app.ui_state.clusters[&cluster_key]
                .operation_history
                .iter()
                .any(|entry| entry.title == "Deployment rolled back")
                .then_some(())
        },
        10_000,
    );
}
