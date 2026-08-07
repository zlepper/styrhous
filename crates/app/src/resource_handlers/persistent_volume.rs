use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, cluster_typed_watcher, minimal_resource_from_typed,
};
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers::{matches_cluster_api_resource, matches_cluster_resource};
use crate::resource_table::{
    ACCESS_MODES_COLUMN, CAPACITY_COLUMN, CellValue, RECLAIM_POLICY_COLUMN,
    ResourceTableDefinition, STATUS_COLUMN, column, status_tone,
};
use k8s_openapi::api::core::v1::PersistentVolume;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_cluster_resource::<PersistentVolume>(&context)
        .then(|| cluster_typed_watcher::<PersistentVolume>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_cluster_api_resource::<PersistentVolume>(api_resource).then(|| {
        ResourceTableDefinition {
            columns: vec![
                column(CAPACITY_COLUMN, "Capacity", 104.0),
                column(ACCESS_MODES_COLUMN, "Access modes", 136.0),
                column(STATUS_COLUMN, "Status", 96.0),
                column(RECLAIM_POLICY_COLUMN, "Reclaim policy", 136.0),
            ],
        }
    })
}

fn extract(resource: &PersistentVolume) -> MinimalResource {
    let spec = resource.spec.as_ref();
    let capacity = spec
        .and_then(|spec| spec.capacity.as_ref())
        .and_then(|capacity| capacity.get("storage"))
        .map(|capacity| capacity.0.clone())
        .unwrap_or_default();
    let access_modes = spec
        .and_then(|spec| spec.access_modes.clone())
        .unwrap_or_default();
    let phase = resource
        .status
        .as_ref()
        .and_then(|status| status.phase.as_deref())
        .unwrap_or("Unknown");
    minimal_resource_from_typed(
        resource,
        BTreeMap::from([
            (CAPACITY_COLUMN.to_owned(), CellValue::Text(capacity)),
            (
                ACCESS_MODES_COLUMN.to_owned(),
                CellValue::List(access_modes),
            ),
            (
                STATUS_COLUMN.to_owned(),
                CellValue::Status {
                    label: phase.to_owned(),
                    tone: status_tone(phase),
                },
            ),
            (
                RECLAIM_POLICY_COLUMN.to_owned(),
                CellValue::Text(
                    spec.and_then(|spec| spec.persistent_volume_reclaim_policy.clone())
                        .unwrap_or_else(|| "Retain".to_owned()),
                ),
            ),
        ]),
    )
}
use crate::api_resource::ApiResource;
