use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::{MinimalResource, PodLogContainer, from_kubernetes_resource};
use crate::pod_metrics::{parse_cpu_nanocores, parse_memory_bytes};
use crate::resource_detail::{
    PodConditionDetail, PodContainerDetail, PodDetail, PodEnvironmentVariableDetail,
    PodEnvironmentVariableSource, PodResourceThresholds, PodVolumeDetail, ResourceDetailPayload,
};
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    CONTAINERS_COLUMN, CPU_COLUMN, CellValue, ContainerIndicator, ContainerKind, MEMORY_COLUMN,
    NODE_COLUMN, READY_COLUMN, RESTARTS_COLUMN, ResourceTableDefinition, STATUS_COLUMN, StatusTone,
    column, status_tone,
};
use k8s_openapi::api::core::v1::{ContainerStatus, Pod};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<Pod>(&context)
        .then(|| namespaced_typed_watcher::<Pod>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<Pod>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(READY_COLUMN, "Ready", 55.0),
            column(CONTAINERS_COLUMN, "Containers", 110.0),
            column(STATUS_COLUMN, "Status", 85.0),
            column(CPU_COLUMN, "CPU", 65.0),
            column(MEMORY_COLUMN, "Memory", 75.0),
            column(RESTARTS_COLUMN, "Restarts", 70.0),
            column(NODE_COLUMN, "Node", 95.0),
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

    let mut resource = from_kubernetes_resource(
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
            (
                NODE_COLUMN.to_owned(),
                CellValue::Text(
                    pod.spec
                        .as_ref()
                        .and_then(|spec| spec.node_name.clone())
                        .unwrap_or_else(|| "-".to_owned()),
                ),
            ),
        ]),
    );
    resource.log_containers = pod_log_containers(pod);
    resource
}

mod detail_payload;
pub(crate) use detail_payload::detail_payload;

fn resource_thresholds(resources: Option<&BTreeMap<String, Quantity>>) -> PodResourceThresholds {
    let quantity = |name| {
        resources
            .and_then(|resources| resources.get(name))
            .map(|value| &value.0)
    };
    PodResourceThresholds {
        cpu_nanocores: quantity("cpu").and_then(|value| parse_cpu_nanocores(value).ok()),
        memory_bytes: quantity("memory").and_then(|value| parse_memory_bytes(value).ok()),
    }
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
                image: container.image.clone(),
            }),
    );
    containers.extend(spec.containers.iter().map(|container| PodLogContainer {
        name: container.name.clone(),
        kind: ContainerKind::App,
        image: container.image.clone(),
    }));
    containers.extend(
        spec.ephemeral_containers
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|container| PodLogContainer {
                name: container.name.clone(),
                kind: ContainerKind::Ephemeral,
                image: container.image.clone(),
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
mod tests;
