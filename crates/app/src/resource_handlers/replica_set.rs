use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::{MinimalResource, from_kubernetes_resource};
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{CellValue, READY_COLUMN, ResourceTableDefinition, column};
use k8s_openapi::api::apps::v1::ReplicaSet;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<ReplicaSet>(&context)
        .then(|| namespaced_typed_watcher::<ReplicaSet>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<ReplicaSet>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![column(READY_COLUMN, "Ready", 90.0)],
    })
}

pub(crate) fn extract(resource: &ReplicaSet) -> MinimalResource {
    let status = resource.status.as_ref();
    let ready = status.and_then(|status| status.ready_replicas).unwrap_or(0);
    let desired = status.map(|status| status.replicas).unwrap_or(0);
    from_kubernetes_resource(
        resource,
        BTreeMap::from([(
            READY_COLUMN.to_owned(),
            CellValue::Text(format!("{ready}/{desired}")),
        )]),
    )
}
use crate::api_resource::ApiResource;
