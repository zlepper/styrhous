use crate::cluster_connection_manager::minimal_resource_from_typed;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    CURRENT_COLUMN, CellValue, DESIRED_COLUMN, READY_COLUMN, ResourceTableDefinition,
    UP_TO_DATE_COLUMN, column,
};
use k8s_openapi::api::apps::v1::DaemonSet;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<DaemonSet>(&context)
        .then(|| namespaced_typed_watcher::<DaemonSet>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<DaemonSet>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(DESIRED_COLUMN, "Desired", 90.0),
            column(CURRENT_COLUMN, "Current", 90.0),
            column(READY_COLUMN, "Ready", 90.0),
            column(UP_TO_DATE_COLUMN, "Up-to-date", 132.0),
        ],
    })
}

fn extract(resource: &DaemonSet) -> MinimalResource {
    let status = resource.status.as_ref();
    let desired = status
        .map(|status| status.desired_number_scheduled)
        .unwrap_or(0);
    let current = status
        .map(|status| status.current_number_scheduled)
        .unwrap_or(0);
    let ready = status.map(|status| status.number_ready).unwrap_or(0);
    let updated = status
        .and_then(|status| status.updated_number_scheduled)
        .unwrap_or(0);
    minimal_resource_from_typed(
        resource,
        BTreeMap::from([
            (
                DESIRED_COLUMN.to_owned(),
                CellValue::Number(i64::from(desired)),
            ),
            (
                CURRENT_COLUMN.to_owned(),
                CellValue::Number(i64::from(current)),
            ),
            (READY_COLUMN.to_owned(), CellValue::Number(i64::from(ready))),
            (
                UP_TO_DATE_COLUMN.to_owned(),
                CellValue::Number(i64::from(updated)),
            ),
        ]),
    )
}
use crate::api_resource::ApiResource;
