//! Kind inspector and resource-delete scenarios.

use super::*;

#[test]
fn test_managed_resource_inspector_integration() {
    let fixture =
        IntegrationNamespaceFixture::create("managed-resource-inspector", "anchor", "unused");
    let deployment_name = "managed-resource-inspector".to_owned();
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
        &deployments_resource,
        Some(&fixture.namespace),
    );
    let history_entry_id = open_resource_detail(
        &mut harness,
        cluster_key,
        &deployment_name,
        Some(&fixture.namespace),
    );

    wait_for_with_diagnostic(
        &mut harness,
        "the Deployment inspector to load its ReplicaSet and Pod",
        |app| {
            current_resource_detail(&app.ui_state, cluster_key, history_entry_id)
                .filter(|panel| {
                    let has_replica_set = panel
                        .managed_resources
                        .iter()
                        .any(|resource| resource.api_resource.kind == "ReplicaSet");
                    let has_pod_with_managed_parent =
                        panel.managed_resources.iter().any(|resource| {
                            resource.api_resource.kind == "Pod"
                                && panel.managed_resources.iter().any(|parent| {
                                    matches!(
                                    &resource.association,
                                    crate::resource_detail::ManagedResourceAssociation::ControllerOwnerUid(owner_uid)
                                        if parent.uid == *owner_uid
                                    )
                                })
                        });
                    has_replica_set && has_pod_with_managed_parent
                })
                .map(|_| ())
        },
        |app| {
            current_resource_detail(&app.ui_state, cluster_key, history_entry_id).and_then(
                |panel| {
                    panel
                        .detail_error
                        .as_ref()
                        .map(|error| format!("Deployment details failed to load: {error}"))
                        .or_else(|| {
                            panel.managed_resources_error.as_ref().map(|error| {
                                format!("managed Deployment resources failed to load: {error}")
                            })
                        })
                },
            )
        },
        15_000,
    );
    let panel = current_resource_detail(&harness.state().ui_state, cluster_key, history_entry_id)
        .expect("Deployment inspector should remain open");
    let replica_set = panel
        .managed_resources
        .iter()
        .find(|resource| resource.api_resource.kind == "ReplicaSet")
        .expect("managed ReplicaSet should be present");
    assert!(
        replica_set.cells.contains_key(READY_COLUMN),
        "managed ReplicaSet should include the Ready table value"
    );
    let pod = panel
        .managed_resources
        .iter()
        .find(|resource| resource.api_resource.kind == "Pod")
        .expect("managed Pod should be present");
    assert!(
        pod.cells.contains_key(STATUS_COLUMN),
        "managed Pod should include the Status table value"
    );
}

/// Verifies that a Node inspector watches Pods cluster-wide and shows the Pods
/// scheduled to the selected Node through the shared inspector table path.

#[test]
fn test_node_inspector_lists_scheduled_pods_integration() {
    let fixture = IntegrationNamespaceFixture::create("node-inspector", "anchor", "unused");
    let pod_name = "node-inspector-pod".to_owned();
    let client = fixture.runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let pods: Api<Pod> = Api::namespaced(client, &fixture.namespace);
    fixture.runtime.block_on(async {
        pods.create(
            &Default::default(),
            &Pod {
                metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                    name: Some(pod_name.clone()),
                    namespace: Some(fixture.namespace.clone()),
                    ..Default::default()
                },
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "pause".to_owned(),
                        image: Some("registry.k8s.io/pause:3.10".to_owned()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to create integration Pod");
    });
    let (mut harness, cluster_key) = connected_kind_harness();
    let node_name = wait_for_kubernetes(
        &mut harness,
        &format!("Kubernetes to assign Pod {pod_name} to a Node"),
        |remaining| {
            kubernetes_request(&fixture.runtime, remaining, pods.get(&pod_name))
                .map(|pod| pod.spec.and_then(|spec| spec.node_name))
        },
        10_000,
    );
    wait_for_cluster_data(&mut harness, cluster_key);
    harness.get_by_label("Nodes").click_accesskit();
    let node_resource = crate::resource_handlers::node::api_resource();
    wait_for_resource_sync(&mut harness, cluster_key, &node_resource, None);
    let history_entry_id = open_resource_detail(&mut harness, cluster_key, &node_name, None);

    wait_for_with_diagnostic(
        &mut harness,
        &format!("the Node inspector to list Pod {pod_name}"),
        |app| {
            current_resource_detail(&app.ui_state, cluster_key, history_entry_id).and_then(
                |panel| {
                    panel
                        .managed_resources
                        .iter()
                        .find(|resource| resource.name == pod_name)
                        .filter(|resource| {
                            resource.namespace.as_deref() == Some(&fixture.namespace)
                        })
                        .map(|_| ())
                },
            )
        },
        |app| {
            current_resource_detail(&app.ui_state, cluster_key, history_entry_id).and_then(
                |panel| {
                    panel
                        .detail_error
                        .as_ref()
                        .map(|error| format!("Node details failed to load: {error}"))
                        .or_else(|| {
                            panel.managed_resources_error.as_ref().map(|error| {
                                format!("Pods scheduled to the Node failed to load: {error}")
                            })
                        })
                },
            )
        },
        15_000,
    );
}

/// Creates a ConfigMap, edits it through the UI, and then deletes it through the UI.

#[test]
fn test_resource_actions_integration() {
    let fixture = IntegrationNamespaceFixture::create(
        "resource-actions",
        ACTIONS_CONFIGMAP_NAME,
        "original-value",
    );
    let test_configmap_name = fixture.name.clone();
    let runtime = &fixture.runtime;
    let configmaps = &fixture.configmaps;

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let configmaps_resource = select_resource(&mut harness, "Config", "Config Maps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &configmaps_resource,
        Some(&fixture.namespace),
    );
    assert!(
        harness.state().ui_state.clusters[&cluster_key].resource_cache
            [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
            .resources
            .values()
            .any(|resource| resource.name == test_configmap_name)
    );

    for _ in 0..3 {
        harness.run_steps(1);
    }
    let actions_label = format!("More actions for {test_configmap_name}");
    harness.get_by_label(&actions_label).click();
    harness.run_steps(1);
    harness.get_by_label("Edit").click();
    harness.run_steps(1);
    let yaml_editor_id = wait_for_yaml_editor(&mut harness, &test_configmap_name, 5_000);

    let yaml_editor = harness
        .state_mut()
        .ui_state
        .yaml_editors
        .get_mut(&yaml_editor_id)
        .expect("YAML editor should be open");
    yaml_editor.edited_yaml = yaml_editor
        .edited_yaml
        .replace("original-value", "edited-value");
    harness.run_steps(1);
    harness.get_by_label("Apply changes").click();
    harness.run_steps(1);
    wait_for_yaml_editor_saved(&mut harness, &test_configmap_name, 5_000);

    let configmap = runtime.block_on(async {
        configmaps
            .get(&test_configmap_name)
            .await
            .expect("ConfigMap should be updated")
    });
    assert_eq!(
        configmap
            .data
            .as_ref()
            .and_then(|data| data.get("key1"))
            .map(String::as_str),
        Some("edited-value")
    );

    for _ in 0..5 {
        harness.run_steps(1);
    }
    harness.get_by_label(&actions_label).click();
    harness.run_steps(1);
    harness.get_by_label("Delete").click();
    harness.run_steps(1);
    assert!(
        harness.state().ui_state.clusters[&cluster_key]
            .pending_delete
            .as_ref()
            .is_some_and(|pending| pending.resource_name == test_configmap_name)
    );

    wait_for(
        &mut harness,
        "the resource-delete confirmation delay to elapse",
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_delete
                .as_ref()
                .filter(|pending| pending.confirmation_available_at <= std::time::Instant::now())
                .map(|_| ())
        },
        5_000,
    );

    let confirm_delete_label = format!("Delete {test_configmap_name}");
    harness.get_by_label(&confirm_delete_label).click();
    harness.run_steps(1);
    wait_for_resource_watch(
        &mut harness,
        &format!("ConfigMap {test_configmap_name} to disappear from the resource watch"),
        cluster_key,
        &configmaps_resource,
        Some(&fixture.namespace),
        |watch| {
            (!watch
                .resources
                .values()
                .any(|resource| resource.name == test_configmap_name))
            .then_some(())
        },
        10_000,
    );
    wait_for_kubernetes(
        &mut harness,
        &format!("ConfigMap {test_configmap_name} to be deleted from Kubernetes"),
        |remaining| {
            kubernetes_object_absent(kubernetes_request(
                runtime,
                remaining,
                configmaps.get(&test_configmap_name),
            ))
        },
        10_000,
    );
}

/// Deletes two independently selected ConfigMaps through the bulk action.

#[test]
fn test_bulk_resource_delete_integration() {
    let fixture = IntegrationNamespaceFixture::create("bulk-delete", "bulk-delete-a", "first");
    let second_name = "bulk-delete-b".to_owned();
    let runtime = &fixture.runtime;
    let configmaps = &fixture.configmaps;
    runtime.block_on(async {
        configmaps
            .create(
                &Default::default(),
                &ConfigMap {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(second_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    data: Some(BTreeMap::from([(
                        String::from("key1"),
                        String::from("second"),
                    )])),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create second integration ConfigMap");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let configmaps_resource = select_resource(&mut harness, "Config", "Config Maps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &configmaps_resource,
        Some(&fixture.namespace),
    );
    wait_for_resource_watch(
        &mut harness,
        "both ConfigMaps to appear in the resource watch",
        cluster_key,
        &configmaps_resource,
        Some(&fixture.namespace),
        |watch| {
            (watch
                .resources
                .values()
                .any(|resource| resource.name == fixture.name)
                && watch
                    .resources
                    .values()
                    .any(|resource| resource.name == second_name))
            .then_some(())
        },
        10_000,
    );

    harness.get_by_label("Select row 1").click();
    harness.run_steps(1);
    harness.get_by_label("Select row 2").click();
    harness.run_steps(1);
    harness.get_by_label("Delete selected").click();
    harness.run_steps(1);
    wait_for_harness(
        &mut harness,
        "the bulk-delete confirmation delay to elapse",
        |harness| {
            harness
                .query_by_role_and_label(egui::accesskit::Role::Button, "Delete 2 resources")
                .filter(|button| !button.accesskit_node().is_disabled())
                .map(|_| ())
        },
        5_000,
    );
    harness.get_by_label("Delete 2 resources").click();
    harness.run_steps(1);
    wait_for_with_diagnostic(
        &mut harness,
        "the bulk delete to finish and clear the resource selection",
        |app| {
            let cluster = &app.ui_state.clusters[&cluster_key];
            (cluster.bulk_delete_progress.is_none()
                && cluster
                    .resource_selections
                    .get(&configmaps_resource)
                    .is_none_or(|selection| selection.is_empty()))
            .then_some(())
        },
        |app| {
            app.ui_state.clusters[&cluster_key]
                .bulk_delete_error
                .clone()
        },
        10_000,
    );
    assert!(matches!(
        runtime.block_on(async { configmaps.get(&fixture.name).await }),
        Err(kube::Error::Api(response)) if response.code == 404
    ));
    assert!(matches!(
        runtime.block_on(async { configmaps.get(&second_name).await }),
        Err(kube::Error::Api(response)) if response.code == 404
    ));
}
