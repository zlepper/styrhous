use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, api_resource_for, cluster_typed_watcher,
};
use crate::minimal_resource::{MinimalResource, from_kubernetes_resource};
use crate::resource_detail::{NodeDetail, ResourceDetailPayload};
use crate::resource_handlers::{matches_cluster_api_resource, matches_cluster_resource};
use crate::resource_table::{
    CellValue, ROLES_COLUMN, ResourceTableDefinition, STATUS_COLUMN, VERSION_COLUMN, column,
    status_tone,
};
use k8s_openapi::api::core::v1::Node;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_cluster_resource::<Node>(&context)
        .then(|| cluster_typed_watcher::<Node>(context, extract))
}

pub(crate) fn api_resource() -> ApiResource {
    api_resource_for::<Node>()
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_cluster_api_resource::<Node>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(STATUS_COLUMN, "Status", 108.0),
            column(ROLES_COLUMN, "Roles", 140.0),
            column(VERSION_COLUMN, "Version", 132.0),
        ],
    })
}

pub(crate) fn detail_payload(object: &kube::api::DynamicObject) -> Option<ResourceDetailPayload> {
    let node = k8s_openapi::serde_json::from_value::<Node>(
        k8s_openapi::serde_json::to_value(object).ok()?,
    )
    .ok()?;
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
    }))
}

fn extract(resource: &Node) -> MinimalResource {
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
    let label = if ready { "Ready" } else { "NotReady" };
    from_kubernetes_resource(
        resource,
        BTreeMap::from([
            (
                STATUS_COLUMN.to_owned(),
                CellValue::Status {
                    label: label.to_owned(),
                    tone: status_tone(label),
                },
            ),
            (ROLES_COLUMN.to_owned(), CellValue::List(roles)),
            (VERSION_COLUMN.to_owned(), CellValue::Text(version)),
        ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{NodeSpec, Taint};
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
    }

    #[test]
    fn api_resource_is_derived_from_the_kube_node_type() {
        let resource = api_resource();

        assert_eq!(resource.kind, Node::kind(&()).as_ref());
        assert_eq!(resource.name, Node::plural(&()).as_ref());
        assert!(!resource.namespaced);
    }
}
