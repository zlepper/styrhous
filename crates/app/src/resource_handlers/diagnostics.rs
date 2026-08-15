//! Compact, copy-oriented diagnostic details for curated resource kinds.
//!
//! These details intentionally use the watched dynamic object rather than
//! growing a renderer and transport type for every Kubernetes kind.  The
//! fields are a small, stable troubleshooting subset; arbitrary resources
//! continue to receive metadata-only details.

use crate::api_resource::ApiResource;
use crate::resource_detail::{DiagnosticDetail, DiagnosticField, DiagnosticSection};
use k8s_openapi::serde_json::{self, Value};

pub(crate) fn detail_payload(
    api_resource: &ApiResource,
    object: &kube::api::DynamicObject,
) -> Option<DiagnosticDetail> {
    if !is_curated(api_resource) {
        return None;
    }
    let value = serde_json::to_value(object).ok()?;
    let mut fields = Vec::new();
    match api_resource.kind.as_str() {
        "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "ReplicationController" => {
            add(&mut fields, "Selector", pointer(&value, "/spec/selector"));
            add(
                &mut fields,
                "Service account",
                pointer(&value, "/spec/template/spec/serviceAccountName"),
            );
            add_value(
                &mut fields,
                "Pod images",
                container_images(&value, "/spec/template/spec/containers"),
            );
            add_value(&mut fields, "Status conditions", conditions(&value));
        }
        "Job" => {
            add(
                &mut fields,
                "Completions",
                pointer(&value, "/spec/completions"),
            );
            add(
                &mut fields,
                "Parallelism",
                pointer(&value, "/spec/parallelism"),
            );
            add(
                &mut fields,
                "Backoff limit",
                pointer(&value, "/spec/backoffLimit"),
            );
            add_value(
                &mut fields,
                "Pod images",
                container_images(&value, "/spec/template/spec/containers"),
            );
            add_value(&mut fields, "Status conditions", conditions(&value));
        }
        "CronJob" => {
            add(&mut fields, "Schedule", pointer(&value, "/spec/schedule"));
            add(&mut fields, "Time zone", pointer(&value, "/spec/timeZone"));
            add(
                &mut fields,
                "Concurrency policy",
                pointer(&value, "/spec/concurrencyPolicy"),
            );
            add(&mut fields, "Suspend", pointer(&value, "/spec/suspend"));
            add_value(
                &mut fields,
                "Pod images",
                container_images(&value, "/spec/jobTemplate/spec/template/spec/containers"),
            );
        }
        "Service" => {
            add(&mut fields, "Type", pointer(&value, "/spec/type"));
            add(
                &mut fields,
                "Cluster IP",
                pointer(&value, "/spec/clusterIP"),
            );
            add(
                &mut fields,
                "External IPs",
                pointer(&value, "/spec/externalIPs"),
            );
            add(&mut fields, "Selector", pointer(&value, "/spec/selector"));
            add_value(&mut fields, "Ports", service_ports(&value));
            add(
                &mut fields,
                "Load balancer ingress",
                pointer(&value, "/status/loadBalancer/ingress"),
            );
        }
        "Endpoints" => add_value(&mut fields, "Endpoints", endpoint_subsets(&value)),
        "EndpointSlice" => {
            add(&mut fields, "Address type", pointer(&value, "/addressType"));
            add(
                &mut fields,
                "Service",
                pointer(&value, "/metadata/labels/kubernetes.io~1service-name"),
            );
            add_value(&mut fields, "Endpoints", slice_endpoints(&value));
            add(&mut fields, "Ports", pointer(&value, "/ports"));
        }
        "Ingress" => {
            add(
                &mut fields,
                "Ingress class",
                pointer(&value, "/spec/ingressClassName"),
            );
            add_value(&mut fields, "Rules", ingress_rules(&value));
            add(&mut fields, "TLS", pointer(&value, "/spec/tls"));
        }
        "IngressClass" => {
            add(
                &mut fields,
                "Controller",
                pointer(&value, "/spec/controller"),
            );
            add(
                &mut fields,
                "Parameters",
                pointer(&value, "/spec/parameters"),
            );
            add(
                &mut fields,
                "Default class",
                pointer(
                    &value,
                    "/metadata/annotations/ingressclass.kubernetes.io~1is-default-class",
                ),
            );
        }
        "NetworkPolicy" => {
            add(
                &mut fields,
                "Pod selector",
                pointer(&value, "/spec/podSelector"),
            );
            add(
                &mut fields,
                "Policy types",
                pointer(&value, "/spec/policyTypes"),
            );
            add(
                &mut fields,
                "Ingress rules",
                pointer(&value, "/spec/ingress"),
            );
            add(&mut fields, "Egress rules", pointer(&value, "/spec/egress"));
        }
        "PersistentVolumeClaim" => {
            add(&mut fields, "Volume", pointer(&value, "/spec/volumeName"));
            add(
                &mut fields,
                "Storage class",
                pointer(&value, "/spec/storageClassName"),
            );
            add(
                &mut fields,
                "Access modes",
                pointer(&value, "/spec/accessModes"),
            );
            add(
                &mut fields,
                "Volume mode",
                pointer(&value, "/spec/volumeMode"),
            );
            add(&mut fields, "Selector", pointer(&value, "/spec/selector"));
        }
        "PersistentVolume" => {
            add(
                &mut fields,
                "Claim reference",
                pointer(&value, "/spec/claimRef"),
            );
            add(
                &mut fields,
                "Storage class",
                pointer(&value, "/spec/storageClassName"),
            );
            add(
                &mut fields,
                "Access modes",
                pointer(&value, "/spec/accessModes"),
            );
            add(
                &mut fields,
                "Volume mode",
                pointer(&value, "/spec/volumeMode"),
            );
            add_value(&mut fields, "Backend", persistent_volume_backend(&value));
        }
        "StorageClass" => {
            add(&mut fields, "Provisioner", pointer(&value, "/provisioner"));
            add(&mut fields, "Parameters", pointer(&value, "/parameters"));
            add(
                &mut fields,
                "Reclaim policy",
                pointer(&value, "/reclaimPolicy"),
            );
            add(
                &mut fields,
                "Binding mode",
                pointer(&value, "/volumeBindingMode"),
            );
        }
        "HorizontalPodAutoscaler" => {
            add(
                &mut fields,
                "Scale target",
                pointer(&value, "/spec/scaleTargetRef"),
            );
            add(
                &mut fields,
                "Minimum replicas",
                pointer(&value, "/spec/minReplicas"),
            );
            add(
                &mut fields,
                "Maximum replicas",
                pointer(&value, "/spec/maxReplicas"),
            );
            add(&mut fields, "Metrics", pointer(&value, "/spec/metrics"));
            add_value(&mut fields, "Status conditions", conditions(&value));
        }
        "PodDisruptionBudget" => {
            add(&mut fields, "Selector", pointer(&value, "/spec/selector"));
            add(
                &mut fields,
                "Minimum available",
                pointer(&value, "/spec/minAvailable"),
            );
            add(
                &mut fields,
                "Maximum unavailable",
                pointer(&value, "/spec/maxUnavailable"),
            );
            add(
                &mut fields,
                "Unhealthy eviction policy",
                pointer(&value, "/spec/unhealthyPodEvictionPolicy"),
            );
        }
        "ResourceQuota" => {
            add(&mut fields, "Hard limits", pointer(&value, "/status/hard"));
            add(&mut fields, "Used", pointer(&value, "/status/used"));
            add(&mut fields, "Scopes", pointer(&value, "/spec/scopes"));
        }
        "LimitRange" => add(&mut fields, "Limits", pointer(&value, "/spec/limits")),
        "Lease" => {
            add(
                &mut fields,
                "Holder identity",
                pointer(&value, "/spec/holderIdentity"),
            );
            add(
                &mut fields,
                "Renew time",
                pointer(&value, "/spec/renewTime"),
            );
            add(
                &mut fields,
                "Lease duration",
                pointer(&value, "/spec/leaseDurationSeconds"),
            );
        }
        "ServiceAccount" => {
            add(
                &mut fields,
                "Image pull secrets",
                pointer(&value, "/imagePullSecrets"),
            );
            add(&mut fields, "Secrets", pointer(&value, "/secrets"));
        }
        "Role" | "ClusterRole" => {
            add(&mut fields, "Rules", pointer(&value, "/rules"));
            add(
                &mut fields,
                "Aggregation rule",
                pointer(&value, "/aggregationRule"),
            );
        }
        "RoleBinding" | "ClusterRoleBinding" => {
            add(&mut fields, "Role reference", pointer(&value, "/roleRef"));
            add(&mut fields, "Subjects", pointer(&value, "/subjects"));
        }
        "Event" => {
            add(
                &mut fields,
                "Involved object",
                pointer(&value, "/involvedObject"),
            );
            add(&mut fields, "Reason", pointer(&value, "/reason"));
            add(&mut fields, "Message", pointer(&value, "/message"));
            add(&mut fields, "Source", pointer(&value, "/source"));
            add(&mut fields, "Count", pointer(&value, "/count"));
        }
        "Namespace" => add(&mut fields, "Phase", pointer(&value, "/status/phase")),
        "PriorityClass" => {
            add(&mut fields, "Value", pointer(&value, "/value"));
            add(
                &mut fields,
                "Global default",
                pointer(&value, "/globalDefault"),
            );
            add(
                &mut fields,
                "Preemption policy",
                pointer(&value, "/preemptionPolicy"),
            );
        }
        "RuntimeClass" => {
            add(&mut fields, "Handler", pointer(&value, "/handler"));
            add(&mut fields, "Overhead", pointer(&value, "/overhead"));
            add(&mut fields, "Scheduling", pointer(&value, "/scheduling"));
        }
        _ => return None,
    }
    (!fields.is_empty()).then_some(DiagnosticDetail {
        sections: vec![DiagnosticSection {
            title: "Troubleshooting".to_owned(),
            fields,
        }],
    })
}

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
mod tests {
    use super::*;

    fn api_resource(kind: &str) -> ApiResource {
        ApiResource {
            group: "core".to_owned(),
            version: "v1".to_owned(),
            kind: kind.to_owned(),
            name: format!("{}s", kind.to_ascii_lowercase()),
            namespaced: true,
        }
    }

    #[test]
    fn extracts_copyable_service_diagnostics() {
        let object = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": {"name": "api"},
            "spec": {
                "type": "LoadBalancer",
                "clusterIP": "10.96.0.10",
                "selector": {"app": "api"},
                "ports": [{"port": 443, "protocol": "TCP", "targetPort": "https"}]
            },
            "status": {"loadBalancer": {"ingress": [{"hostname": "api.example.test"}]}}
        }))
        .unwrap();

        let detail = detail_payload(&api_resource("Service"), &object).unwrap();
        let fields = &detail.sections[0].fields;
        assert!(
            fields
                .iter()
                .any(|field| field.label == "Cluster IP" && field.value == "10.96.0.10")
        );
        assert!(
            fields
                .iter()
                .any(|field| field.label == "Ports" && field.value.contains("443/TCP → https"))
        );
        assert!(
            fields
                .iter()
                .any(|field| field.label == "Load balancer ingress"
                    && field.value.contains("api.example.test"))
        );
    }

    #[test]
    fn only_curated_resource_kinds_receive_payload_diagnostics() {
        let object = serde_json::from_value(serde_json::json!({
            "apiVersion": "example.test/v1", "kind": "Widget", "metadata": {"name": "one"},
            "spec": {"endpoint": "https://example.test"}
        }))
        .unwrap();
        assert!(detail_payload(&api_resource("Widget"), &object).is_none());
    }

    #[test]
    fn custom_resources_with_a_curated_kind_name_remain_metadata_only() {
        let object = serde_json::from_value(serde_json::json!({
            "apiVersion": "example.test/v1", "kind": "Service", "metadata": {"name": "one"},
            "spec": {"credentials": "must-not-be-rendered"}
        }))
        .unwrap();
        let mut resource = api_resource("Service");
        resource.group = "example.test".to_owned();
        assert!(detail_payload(&resource, &object).is_none());
    }
}
