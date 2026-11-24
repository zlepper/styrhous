use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, cluster_typed_watcher,
};
use crate::minimal_resource::{MinimalResource, from_kubernetes_resource};
use crate::pod_metrics::{MissingResourceQuantity, resource_thresholds};
use crate::resource_detail::{NodeDetail, PodResourceThresholds, ResourceDetailPayload};
use crate::resource_handlers::{matches_cluster_api_resource, matches_cluster_resource};
use crate::resource_table::{
    CPU_COLUMN, CellValue, MEMORY_COLUMN, PODS_COLUMN, ROLES_COLUMN, ResourceTableDefinition,
    STATUS_COLUMN, Tone, VERSION_COLUMN, column, status_tone,
};
use k8s_openapi::api::core::v1::Node;
use std::collections::BTreeMap;

// Hidden cells: values the Nodes table and row menu read but show no column for.
const UNSCHEDULABLE_CELL: &str = "__node-unschedulable";
const ALLOCATABLE_CPU_CELL: &str = "__node-allocatable-cpu";
const ALLOCATABLE_MEMORY_CELL: &str = "__node-allocatable-memory";
/// The status of a ready Node that takes no new Pods.
pub(crate) const CORDONED_STATUS: &str = "Cordoned";

/// Records that the Node `resource` is cordoned, for `is_cordoned`.
pub(crate) fn mark_cordoned(resource: &mut MinimalResource) {
    resource.cells.insert(
        UNSCHEDULABLE_CELL.to_owned(),
        CellValue::Text("true".to_owned()),
    );
}

/// Whether the Node `resource` is cordoned (`spec.unschedulable`).
pub(crate) fn is_cordoned(resource: &MinimalResource) -> bool {
    matches!(resource.cells.get(UNSCHEDULABLE_CELL), Some(CellValue::Text(value)) if value == "true")
}

/// The Node `resource`'s allocatable CPU and memory, as `extract` recorded them.
pub(crate) fn allocatable(resource: &MinimalResource) -> PodResourceThresholds {
    let quantity = |cell| match resource.cells.get(cell) {
        Some(CellValue::Number(value)) => Some(*value),
        _ => None,
    };
    PodResourceThresholds {
        cpu_nanocores: quantity(ALLOCATABLE_CPU_CELL),
        memory_bytes: quantity(ALLOCATABLE_MEMORY_CELL),
    }
}

fn node_allocatable(node: &Node) -> PodResourceThresholds {
    resource_thresholds(
        node.status
            .as_ref()
            .and_then(|status| status.allocatable.as_ref()),
        MissingResourceQuantity::Unavailable,
    )
}

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_cluster_resource::<Node>(&context)
        .then(|| cluster_typed_watcher::<Node>(context, extract))
}

pub(crate) fn api_resource() -> ApiResource {
    ApiResource::of::<Node>()
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_cluster_api_resource::<Node>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(STATUS_COLUMN, "Status", 120.0),
            column(ROLES_COLUMN, "Roles", 140.0),
            column(VERSION_COLUMN, "Version", 100.0),
            // Wide enough for "used / allocatable" and its bar (A04).
            column(CPU_COLUMN, "CPU", 140.0).numeric(),
            column(MEMORY_COLUMN, "Memory", 140.0).numeric(),
            column(PODS_COLUMN, "Pods", 72.0).numeric(),
        ],
    })
}

pub(crate) fn detail_payload(object: &kube::api::DynamicObject) -> Option<ResourceDetailPayload> {
    let node = k8s_openapi::serde_json::from_value::<Node>(
        k8s_openapi::serde_json::to_value(object).ok()?,
    )
    .ok()?;
    let allocatable = node_allocatable(&node);
    let spec = node.spec.unwrap_or_default();
    let mut pod_cidrs = spec.pod_cidrs.unwrap_or_default();
    if pod_cidrs.is_empty() {
        pod_cidrs.extend(spec.pod_cidr);
    }
    let taints = spec
        .taints
        .unwrap_or_default()
        .into_iter()
        .map(|taint| match taint.value {
            Some(value) => format!("{}={value}:{}", taint.key, taint.effect),
            None => format!("{}:{}", taint.key, taint.effect),
        })
        .collect();
    Some(ResourceDetailPayload::Node(NodeDetail {
        pod_cidrs,
        provider_id: spec.provider_id,
        unschedulable: spec.unschedulable.unwrap_or(false),
        taints,
        allocatable,
    }))
}

pub(crate) fn extract(resource: &Node) -> MinimalResource {
    let ready = resource
        .status
        .as_ref()
        .and_then(|status| status.conditions.as_ref())
        .and_then(|conditions| {
            conditions
                .iter()
                .find(|condition| condition.type_ == "Ready")
        })
        .map(|condition| condition.status == "True")
        .unwrap_or(false);
    let roles = resource
        .metadata
        .labels
        .as_ref()
        .map(|labels| {
            labels
                .keys()
                .filter_map(|key| key.strip_prefix("node-role.kubernetes.io/"))
                .filter(|role| !role.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let version = resource
        .status
        .as_ref()
        .and_then(|status| status.node_info.as_ref())
        .map(|info| info.kubelet_version.clone())
        .unwrap_or_default();
    let unschedulable = resource
        .spec
        .as_ref()
        .and_then(|spec| spec.unschedulable)
        .unwrap_or(false);
    // A cordoned Node that is otherwise ready says so; not being ready is worse.
    let (label, tone) = match (ready, unschedulable) {
        (false, _) => ("NotReady", status_tone("NotReady")),
        (true, true) => (CORDONED_STATUS, Tone::Warning),
        (true, false) => ("Ready", status_tone("Ready")),
    };
    let status = CellValue::Status {
        label: label.to_owned(),
        tone,
    };
    let allocatable = node_allocatable(resource);
    let mut cells = BTreeMap::from([
        (STATUS_COLUMN.to_owned(), status),
        (ROLES_COLUMN.to_owned(), CellValue::List(roles)),
        (VERSION_COLUMN.to_owned(), CellValue::Text(version)),
    ]);
    for (cell, quantity) in [
        (ALLOCATABLE_CPU_CELL, allocatable.cpu_nanocores),
        (ALLOCATABLE_MEMORY_CELL, allocatable.memory_bytes),
    ] {
        if let Some(quantity) = quantity {
            cells.insert(cell.to_owned(), CellValue::Number(quantity));
        }
    }
    let mut node = from_kubernetes_resource(resource, cells);
    if unschedulable {
        mark_cordoned(&mut node);
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{NodeCondition, NodeSpec, NodeStatus, Taint};
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
    use kube::Resource;

    #[test]
    fn detail_payload_includes_node_spec_fields() {
        let node = Node {
            spec: Some(NodeSpec {
                pod_cidrs: Some(vec!["10.244.0.0/24".into(), "fd00:10:244::/64".into()]),
                provider_id: Some("kind://docker/kind/kind-control-plane".into()),
                unschedulable: Some(true),
                taints: Some(vec![Taint {
                    key: "node-role.kubernetes.io/control-plane".into(),
                    effect: "NoSchedule".into(),
                    value: None,
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let object = k8s_openapi::serde_json::from_value(
            k8s_openapi::serde_json::to_value(node).expect("Node serializes"),
        )
        .expect("Node deserializes as a dynamic object");

        let Some(ResourceDetailPayload::Node(detail)) = detail_payload(&object) else {
            panic!("Node detail payload should be present");
        };

        assert_eq!(detail.pod_cidrs, ["10.244.0.0/24", "fd00:10:244::/64"]);
        assert_eq!(
            detail.provider_id.as_deref(),
            Some("kind://docker/kind/kind-control-plane")
        );
        assert!(detail.unschedulable);
        assert_eq!(
            detail.taints,
            ["node-role.kubernetes.io/control-plane:NoSchedule"]
        );
        assert_eq!(detail.allocatable, PodResourceThresholds::default());
    }

    #[test]
    fn allocatable_thresholds_normalize_available_cpu_and_memory() {
        let thresholds = resource_thresholds(
            Some(&BTreeMap::from([
                ("cpu".to_owned(), Quantity("2".to_owned())),
                ("memory".to_owned(), Quantity("4Gi".to_owned())),
            ])),
            MissingResourceQuantity::Unavailable,
        );
        assert_eq!(
            thresholds,
            PodResourceThresholds {
                cpu_nanocores: Some(2_000_000_000),
                memory_bytes: Some(4 * 1024 * 1024 * 1024),
            }
        );

        let thresholds = resource_thresholds(
            Some(&BTreeMap::from([
                ("cpu".to_owned(), Quantity("invalid".to_owned())),
                ("memory".to_owned(), Quantity("128Mi".to_owned())),
            ])),
            MissingResourceQuantity::Unavailable,
        );
        assert_eq!(thresholds.cpu_nanocores, None);
        assert_eq!(thresholds.memory_bytes, Some(128 * 1024 * 1024));
    }

    fn node(ready: &str, unschedulable: Option<bool>) -> Node {
        Node {
            metadata: kube::api::ObjectMeta {
                name: Some("worker".into()),
                uid: Some("worker-uid".into()),
                ..Default::default()
            },
            spec: Some(NodeSpec {
                unschedulable,
                ..Default::default()
            }),
            status: Some(NodeStatus {
                conditions: Some(vec![NodeCondition {
                    type_: "Ready".into(),
                    status: ready.into(),
                    ..Default::default()
                }]),
                allocatable: Some(BTreeMap::from([
                    ("cpu".to_owned(), Quantity("3920m".to_owned())),
                    ("memory".to_owned(), Quantity("8Gi".to_owned())),
                ])),
                ..Default::default()
            }),
        }
    }

    fn status(resource: &MinimalResource) -> (&str, Tone) {
        match resource.cells.get(STATUS_COLUMN) {
            Some(CellValue::Status { label, tone }) => (label, *tone),
            other => panic!("a status cell, not {other:?}"),
        }
    }

    #[test]
    fn a_cordoned_ready_node_reads_cordoned_and_offers_its_allocatable_resources() {
        let schedulable = extract(&node("True", Some(false)));
        assert_eq!(status(&schedulable), ("Ready", Tone::Success));
        assert!(!is_cordoned(&schedulable));
        assert_eq!(
            allocatable(&schedulable),
            PodResourceThresholds {
                cpu_nanocores: Some(3_920_000_000),
                memory_bytes: Some(8 << 30),
            }
        );

        let cordoned = extract(&node("True", Some(true)));
        assert_eq!(status(&cordoned), (CORDONED_STATUS, Tone::Warning));
        assert!(is_cordoned(&cordoned));

        let not_ready = extract(&node("False", Some(true)));
        assert_eq!(
            status(&not_ready),
            ("NotReady", Tone::Danger),
            "not being ready outranks being cordoned"
        );
        assert!(is_cordoned(&not_ready));
        assert_eq!(
            allocatable(&MinimalResource {
                cells: BTreeMap::new(),
                ..not_ready
            }),
            PodResourceThresholds::default()
        );
    }

    #[test]
    fn api_resource_is_derived_from_the_kube_node_type() {
        let resource = api_resource();

        assert_eq!(resource.kind, Node::kind(&()).as_ref());
        assert_eq!(resource.name, Node::plural(&()).as_ref());
        assert!(!resource.namespaced);
    }
}
