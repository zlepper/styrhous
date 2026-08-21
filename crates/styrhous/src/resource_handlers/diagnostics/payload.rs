use super::*;

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
