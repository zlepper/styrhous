use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, cluster_typed_watcher, minimal_resource_from_typed,
};
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers::{matches_cluster_api_resource, matches_cluster_resource};
use crate::resource_table::{
    BINDING_MODE_COLUMN, CellValue, PROVISIONER_COLUMN, RECLAIM_POLICY_COLUMN,
    ResourceTableDefinition, column,
};
use k8s_openapi::api::storage::v1::StorageClass;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_cluster_resource::<StorageClass>(&context)
        .then(|| cluster_typed_watcher::<StorageClass>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_cluster_api_resource::<StorageClass>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(PROVISIONER_COLUMN, "Provisioner", 176.0),
            column(RECLAIM_POLICY_COLUMN, "Reclaim policy", 136.0),
            column(BINDING_MODE_COLUMN, "Binding mode", 144.0),
        ],
    })
}

fn extract(resource: &StorageClass) -> MinimalResource {
    minimal_resource_from_typed(
        resource,
        BTreeMap::from([
            (
                PROVISIONER_COLUMN.to_owned(),
                CellValue::Text(resource.provisioner.clone()),
            ),
            (
                RECLAIM_POLICY_COLUMN.to_owned(),
                CellValue::Text(
                    resource
                        .reclaim_policy
                        .clone()
                        .unwrap_or_else(|| "Delete".to_owned()),
                ),
            ),
            (
                BINDING_MODE_COLUMN.to_owned(),
                CellValue::Text(
                    resource
                        .volume_binding_mode
                        .clone()
                        .unwrap_or_else(|| "Immediate".to_owned()),
                ),
            ),
        ]),
    )
}
use crate::api_resource::ApiResource;
