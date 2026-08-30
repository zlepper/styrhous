//! Kind connection, watch, secret, and metrics scenarios.

use super::*;

#[test]
fn test_secret_inspector_actions_integration() {
    let fixture = IntegrationNamespaceFixture::create(
        "resource-secret-actions",
        "secret-actions-anchor",
        "unused",
    );
    let test_secret_name = ACTIONS_SECRET_NAME.to_owned();
    let runtime = &fixture.runtime;
    let secrets = &fixture.secrets;
    runtime.block_on(async {
        secrets
            .create(
                &Default::default(),
                &Secret {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(test_secret_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    data: Some(BTreeMap::from([(
                        "password".to_owned(),
                        k8s_openapi::ByteString(b"original-secret".to_vec()),
                    )])),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create integration Secret");
    });

    let (mut harness, cluster_key) = support::connected_kind_harness();
    support::wait_for_cluster_data(&mut harness, cluster_key);
    support::select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let secrets_resource = support::select_resource(&mut harness, "Config", "Secrets");
    support::wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &secrets_resource,
        Some(&fixture.namespace),
    );
    let history_entry_id = support::open_resource_detail(
        &mut harness,
        cluster_key,
        &test_secret_name,
        Some(&fixture.namespace),
    );
    wait_for_data_editor(&mut harness, cluster_key, history_entry_id, "password");
    harness
        .state_mut()
        .ui_state
        .global_blades
        .navigator_mut()
        .and_then(|navigator| navigator.current_mut().resource_detail_mut())
        .filter(|entry| {
            entry.cluster_key == cluster_key && entry.history_entry_id == history_entry_id
        })
        .and_then(|entry| entry.data_editor.as_mut())
        .expect("Secret detail editor should be available")
        .draft_values
        .insert("password".to_owned(), "updated-secret".to_owned());
    harness.run_steps(1);
    harness.get_by_label("Save data").click_accesskit();
    harness.run_steps(1);
    support::wait_for_kubernetes_with_diagnostic(
        &mut harness,
        &format!("Secret {test_secret_name} to contain the saved data"),
        |remaining| {
            kubernetes_request(runtime, remaining, secrets.get(&test_secret_name)).map(|secret| {
                secret
                    .data
                    .as_ref()
                    .and_then(|data| data.get("password"))
                    .is_some_and(|value| value.0 == b"updated-secret")
                    .then_some(())
            })
        },
        |app| {
            current_resource_detail(&app.ui_state, cluster_key, history_entry_id)
                .and_then(|entry| entry.data_editor.as_ref())
                .and_then(|editor| editor.save_error.clone())
        },
        10_000,
    );
}

/// Verifies that the worker can connect to Kind and discover cluster data.

#[test]
fn test_real_cluster_connection() {
    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);

    let cluster = &harness.state().ui_state.clusters[&cluster_key];
    assert_eq!(cluster.name, "kind-kind");
    assert!(matches!(
        cluster.connection,
        ClusterConnectionState::Connected
    ));
    assert!(cluster.namespaces.contains_key(&SortedName::new("default")));
    assert!(
        !cluster.resource_navigation.curated_entries.is_empty()
            || !cluster.resource_navigation.other_api_groups.is_empty(),
        "Kind should advertise Kubernetes API resources"
    );
}

/// Integration test for a real resource watcher using accessibility interactions.

#[test]
fn test_resource_watcher_integration() {
    let fixture = IntegrationNamespaceFixture::create(
        "resource-watcher",
        WATCHER_CONFIGMAP_NAME,
        "watcher-value",
    );
    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    assert!(
        harness.state().ui_state.clusters[&cluster_key]
            .selected_namespaces
            .contains(&fixture.namespace)
    );

    let configmaps_resource = select_resource(&mut harness, "Config", "Config Maps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &configmaps_resource,
        Some(&fixture.namespace),
    );

    let resources = &harness.state().ui_state.clusters[&cluster_key].resource_cache
        [&(configmaps_resource, Some(fixture.namespace.clone()))]
        .resources;
    assert!(
        resources
            .values()
            .any(|resource| resource.name == fixture.name),
        "resource watcher should report the integration ConfigMap"
    );
}

/// Verifies that the namespace and detail metrics watches consume real Metrics API samples and
/// render a history instead of leaving a Pod's usage charts in the collecting state.

#[test]
fn test_pod_metrics_charts_integration() {
    let fixture = IntegrationNamespaceFixture::create("pod-metrics", "metrics-anchor", "unused");
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
                    name: Some(METRICS_LOAD_POD_NAME.to_owned()),
                    namespace: Some(fixture.namespace.clone()),
                    ..Default::default()
                },
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "load".to_owned(),
                        image: Some(
                            "docker.io/library/busybox@sha256:9db7b59979c38555a39def84a31fb98b5296952f9e3afd4f6f11f05b07adfab0"
                                .to_owned(),
                        ),
                        command: Some(vec!["sh".to_owned(), "-c".to_owned()]),
                        args: Some(vec!["while :; do :; done".to_owned()]),
                        resources: Some(ResourceRequirements {
                            limits: Some(BTreeMap::from([
                                ("cpu".to_owned(), Quantity("100m".to_owned())),
                                ("memory".to_owned(), Quantity("64Mi".to_owned())),
                            ])),
                            requests: Some(BTreeMap::from([
                                ("cpu".to_owned(), Quantity("100m".to_owned())),
                                ("memory".to_owned(), Quantity("64Mi".to_owned())),
                            ])),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }],
                    restart_policy: Some("Never".to_owned()),
                    termination_grace_period_seconds: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to create metrics load Pod");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_kubernetes(
        &mut harness,
        &format!("Pod {METRICS_LOAD_POD_NAME} to enter the Running phase"),
        |remaining| {
            kubernetes_request(&fixture.runtime, remaining, pods.get(METRICS_LOAD_POD_NAME)).map(
                |pod| {
                    (pod.status.and_then(|status| status.phase).as_deref() == Some("Running"))
                        .then_some(())
                },
            )
        },
        60_000,
    );
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let pods_resource = select_resource(&mut harness, "Apps & Containers", "Pods");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        &pods_resource,
        Some(&fixture.namespace),
    );

    wait_for_with_terminal_and_timeout_diagnostic(
        &mut harness,
        &format!("a non-zero namespace metrics sample for Pod {METRICS_LOAD_POD_NAME}"),
        |app| {
            app.ui_state
                .clusters
                .get(&cluster_key)
                .filter(|cluster| cluster.pod_metrics_api_available)
                .and_then(|cluster| cluster.pod_metrics.get(&fixture.namespace))
                .filter(|metrics| metrics.error.is_none())
                .and_then(|metrics| metrics.usages.get(METRICS_LOAD_POD_NAME))
                .filter(|usage| usage.cpu_nanocores > 0 && usage.memory_bytes > 0)
                .map(|_| ())
        },
        |app| {
            app.ui_state
                .clusters
                .get(&cluster_key)
                .filter(|cluster| !cluster.pod_metrics_api_available)
                .map(|_| "the cluster reported that the Pod Metrics API is unavailable".into())
        },
        |app| {
            let cluster = app.ui_state.clusters.get(&cluster_key)?;
            let Some(metrics) = cluster.pod_metrics.get(&fixture.namespace) else {
                return Some(format!(
                    "no metrics polling state exists for namespace {}",
                    fixture.namespace
                ));
            };
            metrics.error.clone().or_else(|| {
                Some(format!(
                    "{} Pod metrics samples are present, but none is a non-zero sample for {METRICS_LOAD_POD_NAME}",
                    metrics.usages.len()
                ))
            })
        },
        45_000,
    );

    let history_entry_id = open_resource_detail(
        &mut harness,
        cluster_key,
        METRICS_LOAD_POD_NAME,
        Some(&fixture.namespace),
    );
    let history = wait_for_with_terminal_and_timeout_diagnostic(
        &mut harness,
        &format!("two distinct inspector metrics samples for Pod {METRICS_LOAD_POD_NAME}"),
        |app| {
            current_resource_detail(&app.ui_state, cluster_key, history_entry_id)
                .filter(|entry| {
                    !entry.pod_metrics_api_unavailable
                        && !entry.pod_usage_missing
                        && entry.pod_usage_error.is_none()
                        && entry
                            .pod_usage
                            .as_ref()
                            .is_some_and(|usage| usage.cpu_nanocores > 0 && usage.memory_bytes > 0)
                })
                .and_then(|entry| {
                    (entry.pod_usage_history.len() >= 2).then_some(entry.pod_usage_history.clone())
                })
        },
        |app| {
            let Some(entry) = current_resource_detail(&app.ui_state, cluster_key, history_entry_id)
            else {
                return resource_detail_state(
                    &app.ui_state,
                    cluster_key,
                    METRICS_LOAD_POD_NAME,
                    Some(&fixture.namespace),
                    Some(history_entry_id),
                );
            };
            entry
                .pod_metrics_api_unavailable
                .then(|| "the cluster reported that the Pod Metrics API is unavailable".into())
        },
        |app| {
            let Some(entry) = current_resource_detail(&app.ui_state, cluster_key, history_entry_id)
            else {
                return resource_detail_state(
                    &app.ui_state,
                    cluster_key,
                    METRICS_LOAD_POD_NAME,
                    Some(&fixture.namespace),
                    Some(history_entry_id),
                );
            };
            if let Some(error) = &entry.pod_usage_error {
                return Some(format!("Pod detail metrics polling failed: {error}"));
            }
            Some(format!(
                "usage present={}, usage missing={}, history samples={}",
                entry.pod_usage.is_some(),
                entry.pod_usage_missing,
                entry.pod_usage_history.len()
            ))
        },
        45_000,
    );
    assert!(
        history
            .windows(2)
            .all(|samples| samples[0].timestamp < samples[1].timestamp),
        "metrics-server should return distinct samples: {history:#?}"
    );

    harness.run_steps(1);
    let rendered_history =
        current_resource_detail(&harness.state().ui_state, cluster_key, history_entry_id)
            .map(|entry| entry.pod_usage_history.clone())
            .expect("metrics load Pod inspector should remain open while rendering charts");
    let max_cpu = rendered_history
        .iter()
        .map(|usage| usage.cpu_nanocores)
        .max()
        .unwrap_or_default()
        .max(100_000_000);
    let max_memory = rendered_history
        .iter()
        .map(|usage| usage.memory_bytes)
        .max()
        .unwrap_or_default()
        .max(64 * 1024 * 1024);
    harness.get_by_label("Resource usage");
    harness.get_by_label(&format!(
        "Pod CPU usage chart; usage history available; 10-minute history; scale from 0 to {}; Request 100m, Limit 100m",
        format_cpu(max_cpu),
    ));
    harness.get_by_label(&format!(
        "Pod memory usage chart; usage history available; 10-minute history; scale from 0 to {}; Request 64Mi, Limit 64Mi",
        format_memory(max_memory),
    ));
}
