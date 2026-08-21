use super::*;
use k8s_openapi::api::core::v1::{
    Container, ContainerState, ContainerStateRunning, ContainerStateTerminated,
    ContainerStateWaiting, EphemeralContainer, PodSpec, PodStatus,
};

fn container_status(
    name: &str,
    state: ContainerState,
    ready: bool,
    restart_count: i32,
) -> ContainerStatus {
    ContainerStatus {
        name: name.to_owned(),
        state: Some(state),
        ready,
        restart_count,
        ..Default::default()
    }
}

#[test]
fn extract_includes_all_container_categories_with_state_aware_tones() {
    let pod = Pod {
        spec: Some(PodSpec {
            init_containers: Some(vec![Container {
                name: "setup".to_owned(),
                image: Some("registry.example/setup:v1".to_owned()),
                ..Default::default()
            }]),
            containers: vec![
                Container {
                    name: "api".to_owned(),
                    image: Some("registry.example/api:v1".to_owned()),
                    ..Default::default()
                },
                Container {
                    name: "worker".to_owned(),
                    image: Some("registry.example/worker:v1".to_owned()),
                    ..Default::default()
                },
                Container {
                    name: "sidecar".to_owned(),
                    image: Some("registry.example/api:v1".to_owned()),
                    ..Default::default()
                },
            ],
            ephemeral_containers: Some(vec![EphemeralContainer {
                name: "debugger".to_owned(),
                image: Some("registry.example/debugger:v1".to_owned()),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        status: Some(PodStatus {
            init_container_statuses: Some(vec![container_status(
                "setup",
                ContainerState {
                    terminated: Some(ContainerStateTerminated {
                        exit_code: 0,
                        reason: Some("Completed".to_owned()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                false,
                0,
            )]),
            container_statuses: Some(vec![
                container_status(
                    "api",
                    ContainerState {
                        running: Some(ContainerStateRunning::default()),
                        ..Default::default()
                    },
                    true,
                    2,
                ),
                container_status(
                    "worker",
                    ContainerState {
                        running: Some(ContainerStateRunning::default()),
                        ..Default::default()
                    },
                    false,
                    0,
                ),
                container_status(
                    "sidecar",
                    ContainerState {
                        waiting: Some(ContainerStateWaiting {
                            reason: Some("ContainerCreating".to_owned()),
                            message: Some("Waiting for volume mount".to_owned()),
                        }),
                        ..Default::default()
                    },
                    false,
                    3,
                ),
            ]),
            ephemeral_container_statuses: Some(vec![container_status(
                "debugger",
                ContainerState {
                    terminated: Some(ContainerStateTerminated {
                        exit_code: 1,
                        reason: Some("Error".to_owned()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                false,
                1,
            )]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resource = extract(&pod);
    let CellValue::ContainerIndicators(indicators) = resource
        .cells
        .get(CONTAINERS_COLUMN)
        .expect("containers cell should exist")
    else {
        panic!("containers cell should contain indicators");
    };

    assert_eq!(
        indicators
            .iter()
            .map(|indicator| (indicator.name.as_str(), indicator.kind, indicator.tone))
            .collect::<Vec<_>>(),
        vec![
            ("setup", ContainerKind::Init, StatusTone::Success),
            ("api", ContainerKind::App, StatusTone::Success),
            ("worker", ContainerKind::App, StatusTone::Warning),
            ("sidecar", ContainerKind::App, StatusTone::Warning),
            ("debugger", ContainerKind::Ephemeral, StatusTone::Danger),
        ]
    );
    assert_eq!(
        resource.cells.get(READY_COLUMN),
        Some(&CellValue::Text("1/3".to_owned()))
    );
    assert_eq!(
        resource.cells.get(RESTARTS_COLUMN),
        Some(&CellValue::Number(5))
    );
    assert_eq!(indicators[3].reason.as_deref(), Some("ContainerCreating"));
    assert_eq!(
        indicators[3].message.as_deref(),
        Some("Waiting for volume mount")
    );
    assert_eq!(
        resource
            .log_containers
            .iter()
            .map(|container| (
                container.name.as_str(),
                container.kind,
                container.image.as_deref(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "setup",
                ContainerKind::Init,
                Some("registry.example/setup:v1")
            ),
            ("api", ContainerKind::App, Some("registry.example/api:v1")),
            (
                "worker",
                ContainerKind::App,
                Some("registry.example/worker:v1")
            ),
            (
                "sidecar",
                ContainerKind::App,
                Some("registry.example/api:v1")
            ),
            (
                "debugger",
                ContainerKind::Ephemeral,
                Some("registry.example/debugger:v1")
            ),
        ]
    );
}

#[test]
fn container_without_a_reported_state_is_neutral() {
    let indicator = container_indicator(
        &ContainerStatus {
            name: "api".to_owned(),
            ..Default::default()
        },
        ContainerKind::App,
    );

    assert_eq!(indicator.state, "Unknown");
    assert_eq!(indicator.tone, StatusTone::Neutral);
}

#[test]
fn detail_payload_preserves_operational_pod_fields() {
    let object = kube::api::DynamicObject {
        types: None,
        metadata: kube::api::ObjectMeta {
            name: Some("api-pod".to_owned()),
            ..Default::default()
        },
        data: k8s_openapi::serde_json::json!({
            "spec": {
                "nodeName": "kind-control-plane",
                "restartPolicy": "Always",
                "serviceAccountName": "coredns",
                "dnsPolicy": "ClusterFirst",
                "containers": [{
                    "name": "api",
                    "image": "example/api:v1",
                    "resources": {
                        "requests": {"cpu": "25m", "memory": "32Mi"},
                        "limits": {"cpu": "100m", "memory": "128Mi"}
                    },
                    "env": [
                        {"name": "LOG_LEVEL", "value": "info"},
                        {"name": "CONFIG_VALUE", "valueFrom": {"configMapKeyRef": {"name": "settings", "key": "mode", "optional": true}}},
                        {"name": "SECRET_VALUE", "valueFrom": {"secretKeyRef": {"name": "credentials", "key": "token"}}},
                        {"name": "POD_NAME", "valueFrom": {"fieldRef": {"fieldPath": "metadata.name"}}},
                        {"name": "CPU_LIMIT", "valueFrom": {"resourceFieldRef": {"resource": "limits.cpu", "containerName": "api"}}}
                    ],
                    "envFrom": [
                        {"configMapRef": {"name": "defaults"}, "prefix": "APP_"},
                        {"secretRef": {"name": "shared-secrets", "optional": true}}
                    ]
                }],
                "volumes": [{"name": "config"}]
            },
            "status": {
                "phase": "Running",
                "podIP": "10.244.0.3",
                "qosClass": "Burstable",
                "conditions": [{"type": "Ready", "status": "True"}],
                "containerStatuses": [{
                    "name": "api",
                    "ready": true,
                    "restartCount": 2,
                    "state": {"running": {}}
                }]
            }
        }),
    };

    let Some(ResourceDetailPayload::Pod(detail)) = detail_payload(&object) else {
        panic!("pod dynamic object should produce a pod detail payload");
    };

    assert_eq!(detail.phase, "Running");
    assert_eq!(detail.node_name.as_deref(), Some("kind-control-plane"));
    assert_eq!(detail.conditions[0].type_, "Ready");
    assert_eq!(detail.containers[0].image, "example/api:v1");
    assert_eq!(detail.containers[0].restart_count, 2);
    assert_eq!(
        detail.containers[0].resource_requests,
        PodResourceThresholds {
            cpu_nanocores: Some(25_000_000),
            memory_bytes: Some(32 * 1024 * 1024),
        }
    );
    assert_eq!(
        detail.containers[0].resource_limits,
        PodResourceThresholds {
            cpu_nanocores: Some(100_000_000),
            memory_bytes: Some(128 * 1024 * 1024),
        }
    );
    assert_eq!(
        detail.containers[0]
            .environment_variables
            .iter()
            .map(|variable| (variable.name.as_str(), variable.value.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            ("Import ConfigMap defaults", None),
            ("Import Secret shared-secrets", None),
            ("LOG_LEVEL", Some("info")),
            ("CONFIG_VALUE", None),
            ("SECRET_VALUE", None),
            ("POD_NAME", Some("api-pod")),
            ("CPU_LIMIT", None),
        ]
    );
    assert!(matches!(
        detail.containers[0].environment_variables[0].source,
        PodEnvironmentVariableSource::ConfigMapImport { .. }
    ));
    assert!(matches!(
        detail.containers[0].environment_variables[3].source,
        PodEnvironmentVariableSource::ConfigMapKey { .. }
    ));
    assert!(matches!(
        detail.containers[0].environment_variables[4].source,
        PodEnvironmentVariableSource::SecretKey { .. }
    ));
    assert_eq!(detail.volumes[0].name, "config");
    assert_eq!(detail.restart_policy.as_deref(), Some("Always"));
}

#[test]
fn resource_thresholds_normalize_supported_cpu_and_memory_quantities() {
    let thresholds = resource_thresholds(Some(&BTreeMap::from([
        ("cpu".to_owned(), Quantity("1.5".to_owned())),
        ("memory".to_owned(), Quantity("48Mi".to_owned())),
        ("ephemeral-storage".to_owned(), Quantity("1Gi".to_owned())),
    ])));
    assert_eq!(
        thresholds,
        PodResourceThresholds {
            cpu_nanocores: Some(1_500_000_000),
            memory_bytes: Some(48 * 1024 * 1024),
        }
    );

    let thresholds = resource_thresholds(Some(&BTreeMap::from([(
        "cpu".to_owned(),
        Quantity("not-a-quantity".to_owned()),
    )])));
    assert_eq!(thresholds, PodResourceThresholds::default());
}
