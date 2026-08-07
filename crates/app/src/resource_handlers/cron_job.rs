use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, minimal_resource_from_typed, namespaced_typed_watcher,
};
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    ACTIVE_COLUMN, CellValue, ResourceTableDefinition, SCHEDULE_COLUMN, SUSPEND_COLUMN, column,
};
use k8s_openapi::api::batch::v1::CronJob;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<CronJob>(&context)
        .then(|| namespaced_typed_watcher::<CronJob>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<CronJob>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(SCHEDULE_COLUMN, "Schedule", 128.0),
            column(SUSPEND_COLUMN, "Suspend", 96.0),
            column(ACTIVE_COLUMN, "Active", 88.0),
        ],
    })
}

fn extract(resource: &CronJob) -> MinimalResource {
    let spec = resource.spec.as_ref();
    let active = resource
        .status
        .as_ref()
        .and_then(|status| status.active.as_ref())
        .map_or(0, Vec::len);
    minimal_resource_from_typed(
        resource,
        BTreeMap::from([
            (
                SCHEDULE_COLUMN.to_owned(),
                CellValue::Text(spec.map(|spec| spec.schedule.clone()).unwrap_or_default()),
            ),
            (
                SUSPEND_COLUMN.to_owned(),
                CellValue::Text(
                    spec.and_then(|spec| spec.suspend)
                        .map_or_else(|| "False".to_owned(), |suspend| suspend.to_string()),
                ),
            ),
            (ACTIVE_COLUMN.to_owned(), CellValue::Number(active as i64)),
        ]),
    )
}
use crate::api_resource::ApiResource;
