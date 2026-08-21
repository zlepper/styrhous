//! Compact, copy-oriented diagnostic details for curated resource kinds.
//!
//! These details intentionally use the watched dynamic object rather than
//! growing a renderer and transport type for every Kubernetes kind.  The
//! fields are a small, stable troubleshooting subset; arbitrary resources
//! continue to receive metadata-only details.

use crate::api_resource::ApiResource;
use crate::resource_detail::{DiagnosticDetail, DiagnosticField, DiagnosticSection};
use k8s_openapi::serde_json::{self, Value};

mod payload;

pub(crate) use payload::detail_payload;

fn is_curated(api: &ApiResource) -> bool {
    matches!(
        (api.group.as_str(), api.kind.as_str()),
        (
            "apps",
            "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet"
        ) | (
            "core",
            "ReplicationController"
                | "Service"
                | "Endpoints"
                | "PersistentVolumeClaim"
                | "PersistentVolume"
                | "ResourceQuota"
                | "LimitRange"
                | "ServiceAccount"
                | "Event"
                | "Namespace"
        ) | ("batch", "Job" | "CronJob")
            | ("discovery.k8s.io", "EndpointSlice")
            | (
                "networking.k8s.io",
                "Ingress" | "IngressClass" | "NetworkPolicy"
            )
            | ("storage.k8s.io", "StorageClass")
            | ("autoscaling", "HorizontalPodAutoscaler")
            | ("policy", "PodDisruptionBudget")
            | ("coordination.k8s.io", "Lease")
            | (
                "rbac.authorization.k8s.io",
                "Role" | "ClusterRole" | "RoleBinding" | "ClusterRoleBinding"
            )
            | ("scheduling.k8s.io", "PriorityClass")
            | ("node.k8s.io", "RuntimeClass")
    )
}

fn pointer<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    value.pointer(path)
}

fn add(fields: &mut Vec<DiagnosticField>, label: &str, value: Option<&Value>) {
    add_value(fields, label, value.and_then(compact));
}

fn add_value(fields: &mut Vec<DiagnosticField>, label: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.is_empty() && value != "null") {
        fields.push(DiagnosticField {
            label: label.to_owned(),
            value,
        });
    }
}

fn compact(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Array(values) => Some(
            values
                .iter()
                .filter_map(compact)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Value::Object(_) => serde_json::to_string(value).ok(),
    }
}

fn container_images(value: &Value, path: &str) -> Option<String> {
    value
        .pointer(path)?
        .as_array()
        .map(|containers| {
            containers
                .iter()
                .filter_map(|container| {
                    Some(format!(
                        "{}={}",
                        container.get("name")?.as_str()?,
                        container.get("image")?.as_str()?
                    ))
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty())
}

fn conditions(value: &Value) -> Option<String> {
    value
        .pointer("/status/conditions")?
        .as_array()
        .map(|conditions| {
            conditions
                .iter()
                .filter_map(|condition| {
                    let type_ = condition.get("type")?.as_str()?;
                    let reason = condition
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("-");
                    let message = condition
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("-");
                    Some(format!("{type_}: {reason} — {message}"))
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|value| !value.is_empty())
}

fn service_ports(value: &Value) -> Option<String> {
    value
        .pointer("/spec/ports")?
        .as_array()
        .map(|ports| {
            ports
                .iter()
                .filter_map(|port| {
                    let port_number = port.get("port")?;
                    let protocol = port
                        .get("protocol")
                        .and_then(Value::as_str)
                        .unwrap_or("TCP");
                    let target = port
                        .get("targetPort")
                        .and_then(compact)
                        .unwrap_or_else(|| "-".to_owned());
                    Some(format!("{port_number}/{protocol} → {target}"))
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty())
}

fn endpoint_subsets(value: &Value) -> Option<String> {
    value
        .pointer("/subsets")?
        .as_array()
        .map(|subsets| {
            subsets
                .iter()
                .flat_map(|subset| {
                    let ports = subset
                        .get("ports")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|port| port.get("port").and_then(compact))
                        .collect::<Vec<_>>();
                    subset
                        .get("addresses")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|address| {
                            address
                                .get("ip")
                                .and_then(Value::as_str)
                                .map(|ip| format!("{ip}:{}", ports.join(",")))
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty())
}

fn slice_endpoints(value: &Value) -> Option<String> {
    value
        .pointer("/endpoints")?
        .as_array()
        .map(|endpoints| {
            endpoints
                .iter()
                .flat_map(|endpoint| {
                    endpoint
                        .get("addresses")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty())
}

fn ingress_rules(value: &Value) -> Option<String> {
    value
        .pointer("/spec/rules")?
        .as_array()
        .map(|rules| {
            rules
                .iter()
                .flat_map(|rule| {
                    let host = rule.get("host").and_then(Value::as_str).unwrap_or("*");
                    rule.pointer("/http/paths")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(move |path| {
                            let path_value =
                                path.get("path").and_then(Value::as_str).unwrap_or("/");
                            let service = path
                                .pointer("/backend/service/name")
                                .and_then(Value::as_str)?;
                            let port = path
                                .pointer("/backend/service/port")
                                .and_then(compact)
                                .unwrap_or_else(|| "-".to_owned());
                            Some(format!("{host}{path_value} → {service}:{port}"))
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|value| !value.is_empty())
}

fn persistent_volume_backend(value: &Value) -> Option<String> {
    let spec = value.pointer("/spec")?.as_object()?;
    for (kind, backend) in spec {
        if [
            "claimRef",
            "capacity",
            "accessModes",
            "storageClassName",
            "persistentVolumeReclaimPolicy",
            "volumeMode",
            "mountOptions",
            "nodeAffinity",
        ]
        .contains(&kind.as_str())
        {
            continue;
        }
        if backend.is_object() {
            return compact(backend).map(|backend| format!("{kind}: {backend}"));
        }
    }
    None
}

#[cfg(test)]
mod tests;
