use crate::cluster_connection_manager::minimal_resource_from_typed;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    AVAILABLE_COLUMN, CellValue, READY_COLUMN, ResourceTableDefinition, UP_TO_DATE_COLUMN, column,
};
use k8s_openapi::api::apps::v1::Deployment;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<Deployment>(&context)
        .then(|| namespaced_typed_watcher::<Deployment>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<Deployment>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(READY_COLUMN, "Ready", 90.0),
            column(UP_TO_DATE_COLUMN, "Up-to-date", 132.0),
            column(AVAILABLE_COLUMN, "Available", 116.0),
        ],
    })
}

pub(crate) fn extract(deployment: &Deployment) -> MinimalResource {
    let status = deployment.status.as_ref();
    let desired = status.and_then(|status| status.replicas).unwrap_or(0);
    let ready = status.and_then(|status| status.ready_replicas).unwrap_or(0);
    let updated = status
        .and_then(|status| status.updated_replicas)
        .unwrap_or(0);
    let available = status
        .and_then(|status| status.available_replicas)
        .unwrap_or(0);

    minimal_resource_from_typed(
        deployment,
        BTreeMap::from([
            (
                READY_COLUMN.to_owned(),
                CellValue::Text(format!("{ready}/{desired}")),
            ),
            (
                UP_TO_DATE_COLUMN.to_owned(),
                CellValue::Number(i64::from(updated)),
            ),
            (
                AVAILABLE_COLUMN.to_owned(),
                CellValue::Number(i64::from(available)),
            ),
        ]),
    )
}
use crate::api_resource::ApiResource;
