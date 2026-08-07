use crate::cluster_connection_manager::minimal_resource_from_typed;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    CellValue, READY_COLUMN, RESTARTS_COLUMN, ResourceTableDefinition, STATUS_COLUMN, column,
    status_tone,
};
use k8s_openapi::api::core::v1::Pod;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<Pod>(&context)
        .then(|| namespaced_typed_watcher::<Pod>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<Pod>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(READY_COLUMN, "Ready", 90.0),
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

    minimal_resource_from_typed(
        pod,
        BTreeMap::from([
            (
                READY_COLUMN.to_owned(),
                CellValue::Text(format!("{ready}/{total}")),
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
    )
}
use crate::api_resource::ApiResource;
