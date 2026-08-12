use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::{MinimalResource, from_kubernetes_resource};
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    COMPLETIONS_COLUMN, CellValue, ResourceTableDefinition, STATUS_COLUMN, column, status_tone,
};
use k8s_openapi::api::batch::v1::Job;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<Job>(&context)
        .then(|| namespaced_typed_watcher::<Job>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<Job>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(COMPLETIONS_COLUMN, "Completions", 112.0),
            column(STATUS_COLUMN, "Status", 124.0),
        ],
    })
}

pub(crate) fn extract(resource: &Job) -> MinimalResource {
    let status = resource.status.as_ref();
    let succeeded = status.and_then(|status| status.succeeded).unwrap_or(0);
    let desired = resource
        .spec
        .as_ref()
        .and_then(|spec| spec.completions)
        .unwrap_or(1);
    let phase = if status.and_then(|status| status.failed).unwrap_or(0) > 0 {
        "Failed"
    } else if succeeded >= desired {
        "Succeeded"
    } else if status.and_then(|status| status.active).unwrap_or(0) > 0 {
        "Running"
    } else {
        "Pending"
    };
    from_kubernetes_resource(
        resource,
        BTreeMap::from([
            (
                COMPLETIONS_COLUMN.to_owned(),
                CellValue::Text(format!("{succeeded}/{desired}")),
            ),
            (
                STATUS_COLUMN.to_owned(),
                CellValue::Status {
                    label: phase.to_owned(),
                    tone: status_tone(phase),
                },
            ),
        ]),
    )
}
use crate::api_resource::ApiResource;
