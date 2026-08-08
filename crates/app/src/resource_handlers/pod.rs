use crate::cluster_connection_manager::minimal_resource_from_typed;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::{MinimalResource, PodLogContainer};
use crate::resource_detail::{
    PodConditionDetail, PodContainerDetail, PodDetail, PodEnvironmentVariableDetail,
    PodEnvironmentVariableSource, PodVolumeDetail, ResourceDetailPayload,
};
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    CONTAINERS_COLUMN, CellValue, ContainerIndicator, ContainerKind, READY_COLUMN, RESTARTS_COLUMN,
    ResourceTableDefinition, STATUS_COLUMN, StatusTone, column, status_tone,
};
use k8s_openapi::api::core::v1::{ContainerStatus, Pod};
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<Pod>(&context)
        .then(|| namespaced_typed_watcher::<Pod>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<Pod>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(READY_COLUMN, "Ready", 90.0),
            column(CONTAINERS_COLUMN, "Containers", 150.0),
            column(STATUS_COLUMN, "Status", 128.0),
            column(RESTARTS_COLUMN, "Restarts", 120.0),
        ],
    })
}

pub(crate) fn extract(pod: &Pod) -> MinimalResource {
    let status = pod.status.as_ref();
    let containers = status.and_then(|status| status.container_statuses.as_ref());
    let total = containers.map_or(0, Vec::len);
    let ready = containers
        .map(|containers| {
            containers
                .iter()
                .filter(|container| container.ready)
                .count()
        })
        .unwrap_or(0);
    let restarts = containers
        .map(|containers| {
            containers
                .iter()
                .map(|container| i64::from(container.restart_count))
                .sum()
        })
        .unwrap_or(0);
    let phase = status
        .and_then(|status| status.phase.as_deref())
        .unwrap_or("Unknown");
    let indicators = status.map(container_indicators).unwrap_or_default();

    let mut resource = minimal_resource_from_typed(
        pod,
        BTreeMap::from([
            (
                READY_COLUMN.to_owned(),
                CellValue::Text(format!("{ready}/{total}")),
            ),
            (
                CONTAINERS_COLUMN.to_owned(),
                CellValue::ContainerIndicators(indicators),
            ),
            (
                STATUS_COLUMN.to_owned(),
                CellValue::Status {
                    label: phase.to_owned(),
                    tone: status_tone(phase),
                },
            ),
            (RESTARTS_COLUMN.to_owned(), CellValue::Number(restarts)),
        ]),
    );
    resource.log_containers = pod_log_containers(pod);
    resource
}

pub(crate) fn detail_payload(object: &kube::api::DynamicObject) -> Option<ResourceDetailPayload> {
    let pod =
        k8s_openapi::serde_json::from_value::<Pod>(k8s_openapi::serde_json::to_value(object).ok()?)
            .ok()?;
    let status = pod.status.as_ref();
    let containers = pod
        .spec
        .as_ref()
        .map(|spec| &spec.containers)
        .map(|containers| {
            containers
                .iter()
                .map(|container| {
                    let status = status
                        .and_then(|status| status.container_statuses.as_ref())
                        .and_then(|statuses| {
                            statuses.iter().find(|entry| entry.name == container.name)
                        });
                    let (state, reason, message) = status
                        .map(container_detail_state)
                        .unwrap_or_else(|| ("Unknown".to_owned(), None, None));
                    PodContainerDetail {
                        name: container.name.clone(),
                        image: container.image.clone().unwrap_or_else(|| "-".to_owned()),
                        ready: status.is_some_and(|status| status.ready),
                        restart_count: status.map_or(0, |status| status.restart_count),
                        state,
                        reason,
                        message,
                        command: container.command.clone().unwrap_or_default(),
                        args: container.args.clone().unwrap_or_default(),
                        ports: container
                            .ports
                            .as_deref()
                            .unwrap_or_default()
                            .iter()
                            .map(|port| {
                                format!(
                                    "{}/{}",
                                    port.container_port,
                                    port.protocol.as_deref().unwrap_or("TCP")
                                )
                            })
                            .collect(),
                        environment_variables: pod_environment_variables(container, &pod),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let conditions = status
        .and_then(|status| status.conditions.as_ref())
        .map(|conditions| {
            conditions
                .iter()
                .map(|condition| PodConditionDetail {
                    type_: condition.type_.clone(),
                    status: condition.status.clone(),
                    reason: condition.reason.clone(),
                    message: condition.message.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let volumes = pod
        .spec
        .as_ref()
        .and_then(|spec| spec.volumes.as_ref())
        .map_or_else(Vec::new, |volumes| {
            volumes
                .iter()
                .map(|volume| pod_volume_detail(volume, &pod))
                .collect()
        });

    Some(ResourceDetailPayload::Pod(PodDetail {
        phase: status
            .and_then(|status| status.phase.clone())
            .unwrap_or_else(|| "Unknown".to_owned()),
        conditions,
        node_name: pod.spec.as_ref().and_then(|spec| spec.node_name.clone()),
        pod_ip: status.and_then(|status| status.pod_ip.clone()),
        host_ip: status.and_then(|status| status.host_ip.clone()),
        qos_class: status.and_then(|status| status.qos_class.clone()),
        restart_policy: pod
            .spec
            .as_ref()
            .and_then(|spec| spec.restart_policy.clone()),
        service_account_name: pod
            .spec
            .as_ref()
            .and_then(|spec| spec.service_account_name.clone()),
        dns_policy: pod.spec.as_ref().and_then(|spec| spec.dns_policy.clone()),
        containers,
        log_containers: pod_log_containers(&pod),
        volumes,
    }))
}

fn pod_log_containers(pod: &Pod) -> Vec<PodLogContainer> {
    let Some(spec) = &pod.spec else {
        return Vec::new();
    };
    let mut containers = Vec::new();
    containers.extend(
        spec.init_containers
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|container| PodLogContainer {
                name: container.name.clone(),
                kind: ContainerKind::Init,
            }),
    );
    containers.extend(spec.containers.iter().map(|container| PodLogContainer {
        name: container.name.clone(),
        kind: ContainerKind::App,
    }));
    containers.extend(
        spec.ephemeral_containers
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|container| PodLogContainer {
                name: container.name.clone(),
                kind: ContainerKind::Ephemeral,
            }),
    );
    containers
}

fn pod_environment_variables(
    container: &k8s_openapi::api::core::v1::Container,
    pod: &Pod,
) -> Vec<PodEnvironmentVariableDetail> {
    let variables = container
        .env
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|variable| {
            let (value, source) = if let Some(value) = &variable.value {
                (Some(value.clone()), PodEnvironmentVariableSource::Literal)
            } else if let Some(source) = &variable.value_from {
                environment_variable_source(source, pod)
            } else {
                (None, PodEnvironmentVariableSource::Unspecified)
            };
            PodEnvironmentVariableDetail {
                name: variable.name.clone(),
                value,
                source,
            }
        });
    let imports = container
        .env_from
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|source| {
            if let Some(config_map) = &source.config_map_ref {
                PodEnvironmentVariableDetail {
                    name: format!("Import ConfigMap {}", config_map.name),
                    value: None,
                    source: PodEnvironmentVariableSource::ConfigMapImport {
                        name: config_map.name.clone(),
                        prefix: source.prefix.clone().unwrap_or_default(),
                        optional: config_map.optional.unwrap_or(false),
                    },
                }
            } else if let Some(secret) = &source.secret_ref {
                PodEnvironmentVariableDetail {
                    name: format!("Import Secret {}", secret.name),
                    value: None,
                    source: PodEnvironmentVariableSource::SecretImport {
                        name: secret.name.clone(),
                        prefix: source.prefix.clone().unwrap_or_default(),
                        optional: secret.optional.unwrap_or(false),
                    },
                }
            } else {
                PodEnvironmentVariableDetail {
                    name: "Import".to_owned(),
                    value: None,
                    source: PodEnvironmentVariableSource::Unspecified,
                }
            }
        });

    imports.chain(variables).collect()
}

fn environment_variable_source(
    source: &k8s_openapi::api::core::v1::EnvVarSource,
    pod: &Pod,
) -> (Option<String>, PodEnvironmentVariableSource) {
    if let Some(config_map) = &source.config_map_key_ref {
        (
            None,
            PodEnvironmentVariableSource::ConfigMapKey {
                name: config_map.name.clone(),
                key: config_map.key.clone(),
                optional: config_map.optional.unwrap_or(false),
            },
        )
    } else if let Some(secret) = &source.secret_key_ref {
        (
            None,
            PodEnvironmentVariableSource::SecretKey {
                name: secret.name.clone(),
                key: secret.key.clone(),
                optional: secret.optional.unwrap_or(false),
            },
        )
    } else if let Some(field) = &source.field_ref {
        (
            pod_field_value(pod, &field.field_path),
            PodEnvironmentVariableSource::Field {
                path: field.field_path.clone(),
            },
        )
    } else if let Some(resource) = &source.resource_field_ref {
        (
            None,
            PodEnvironmentVariableSource::ResourceField {
                resource: resource.resource.clone(),
                container_name: resource.container_name.clone(),
            },
        )
    } else {
        (None, PodEnvironmentVariableSource::Unspecified)
    }
}

fn pod_field_value(pod: &Pod, path: &str) -> Option<String> {
    match path {
        "metadata.name" => pod.metadata.name.clone(),
        "metadata.namespace" => pod.metadata.namespace.clone(),
        "metadata.uid" => pod.metadata.uid.clone(),
        "spec.nodeName" => pod.spec.as_ref().and_then(|spec| spec.node_name.clone()),
        "spec.serviceAccountName" => pod
            .spec
            .as_ref()
            .and_then(|spec| spec.service_account_name.clone()),
        "status.hostIP" => pod
            .status
            .as_ref()
            .and_then(|status| status.host_ip.clone()),
        "status.podIP" => pod.status.as_ref().and_then(|status| status.pod_ip.clone()),
        "status.podIPs" => pod.status.as_ref().and_then(|status| {
            status.pod_ips.as_ref().map(|ips| {
                ips.iter()
                    .map(|ip| ip.ip.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            })
        }),
        _ => pod_metadata_map_value(path, "metadata.labels", pod.metadata.labels.as_ref()).or_else(
            || {
                pod_metadata_map_value(
                    path,
                    "metadata.annotations",
                    pod.metadata.annotations.as_ref(),
                )
            },
        ),
    }
}

fn pod_metadata_map_value(
    path: &str,
    field: &str,
    values: Option<&std::collections::BTreeMap<String, String>>,
) -> Option<String> {
    let key = path.strip_prefix(&format!("{field}["))?.strip_suffix(']')?;
    let key = key
        .strip_prefix('\'')
        .and_then(|key| key.strip_suffix('\''))
        .or_else(|| key.strip_prefix('"').and_then(|key| key.strip_suffix('"')))?;
    values.and_then(|values| values.get(key)).cloned()
}

fn pod_volume_detail(volume: &k8s_openapi::api::core::v1::Volume, pod: &Pod) -> PodVolumeDetail {
    let (kind, source) = if let Some(source) = &volume.config_map {
        ("ConfigMap", source.name.clone())
    } else if volume.projected.is_some() {
        ("Projected", volume.name.clone())
    } else if let Some(source) = &volume.secret {
        (
            "Secret",
            source
                .secret_name
                .clone()
                .unwrap_or_else(|| volume.name.clone()),
        )
    } else if let Some(source) = &volume.persistent_volume_claim {
        ("PersistentVolumeClaim", source.claim_name.clone())
    } else if volume.empty_dir.is_some() {
        ("EmptyDir", volume.name.clone())
    } else if let Some(source) = &volume.host_path {
        ("HostPath", source.path.clone())
    } else {
        ("Volume", volume.name.clone())
    };
    let mount = pod
        .spec
        .as_ref()
        .map(|spec| &spec.containers)
        .into_iter()
        .flatten()
        .filter_map(|container| container.volume_mounts.as_ref())
        .flatten()
        .find(|mount| mount.name == volume.name);
    PodVolumeDetail {
        name: volume.name.clone(),
        kind: kind.to_owned(),
        source,
        mount_path: mount.map(|mount| mount.mount_path.clone()),
        read_only: mount.is_some_and(|mount| mount.read_only.unwrap_or(false)),
    }
}

fn container_detail_state(status: &ContainerStatus) -> (String, Option<String>, Option<String>) {
    match status.state.as_ref() {
        Some(state) if state.running.is_some() => ("Running".to_owned(), None, None),
        Some(state) if state.waiting.is_some() => {
            let waiting = state.waiting.as_ref().expect("checked waiting state");
            (
                "Waiting".to_owned(),
                waiting.reason.clone(),
                waiting.message.clone(),
            )
        }
        Some(state) if state.terminated.is_some() => {
            let terminated = state.terminated.as_ref().expect("checked terminated state");
            (
                "Terminated".to_owned(),
                terminated.reason.clone(),
                terminated.message.clone(),
            )
        }
        _ => ("Unknown".to_owned(), None, None),
    }
}

fn container_indicators(status: &k8s_openapi::api::core::v1::PodStatus) -> Vec<ContainerIndicator> {
    let mut indicators = Vec::new();
    append_container_indicators(
        &mut indicators,
        status.init_container_statuses.as_deref(),
        ContainerKind::Init,
    );
    append_container_indicators(
        &mut indicators,
        status.container_statuses.as_deref(),
        ContainerKind::App,
    );
    append_container_indicators(
        &mut indicators,
        status.ephemeral_container_statuses.as_deref(),
        ContainerKind::Ephemeral,
    );
    indicators
}

fn append_container_indicators(
    indicators: &mut Vec<ContainerIndicator>,
    statuses: Option<&[ContainerStatus]>,
    kind: ContainerKind,
) {
    indicators.extend(
        statuses
            .unwrap_or_default()
            .iter()
            .map(|status| container_indicator(status, kind)),
    );
}

fn container_indicator(status: &ContainerStatus, kind: ContainerKind) -> ContainerIndicator {
    let (state, reason, message, tone) = match status.state.as_ref() {
        Some(state) if state.running.is_some() => (
            "Running".to_owned(),
            None,
            None,
            if status.ready {
                StatusTone::Success
            } else {
                StatusTone::Warning
            },
        ),
        Some(state) if state.waiting.is_some() => {
            let waiting = state.waiting.as_ref().expect("checked waiting state");
            (
                "Waiting".to_owned(),
                waiting.reason.clone(),
                waiting.message.clone(),
                StatusTone::Warning,
            )
        }
        Some(state) if state.terminated.is_some() => {
            let terminated = state.terminated.as_ref().expect("checked terminated state");
            (
                "Terminated".to_owned(),
                terminated.reason.clone(),
                terminated.message.clone(),
                if terminated.exit_code == 0 {
                    StatusTone::Success
                } else {
                    StatusTone::Danger
                },
            )
        }
        _ => ("Unknown".to_owned(), None, None, StatusTone::Neutral),
    };

    ContainerIndicator {
        name: status.name.clone(),
        kind,
        state,
        reason,
        message,
        ready: status.ready,
        restart_count: status.restart_count,
        tone,
    }
}

#[cfg(test)]
mod tests {
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
                    ..Default::default()
                }]),
                containers: vec![
                    Container {
                        name: "api".to_owned(),
                        ..Default::default()
                    },
                    Container {
                        name: "worker".to_owned(),
                        ..Default::default()
                    },
                    Container {
                        name: "sidecar".to_owned(),
                        ..Default::default()
                    },
                ],
                ephemeral_containers: Some(vec![EphemeralContainer {
                    name: "debugger".to_owned(),
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
                .map(|container| (container.name.as_str(), container.kind))
                .collect::<Vec<_>>(),
            vec![
                ("setup", ContainerKind::Init),
                ("api", ContainerKind::App),
                ("worker", ContainerKind::App),
                ("sidecar", ContainerKind::App),
                ("debugger", ContainerKind::Ephemeral),
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
}
use crate::api_resource::ApiResource;
