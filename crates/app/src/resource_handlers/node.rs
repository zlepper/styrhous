use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, cluster_typed_watcher, minimal_resource_from_typed,
};
use crate::minimal_resource::MinimalResource;
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

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_cluster_api_resource::<Node>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(STATUS_COLUMN, "Status", 108.0),
            column(ROLES_COLUMN, "Roles", 140.0),
            column(VERSION_COLUMN, "Version", 132.0),
        ],
    })
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
    minimal_resource_from_typed(
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
use crate::api_resource::ApiResource;
