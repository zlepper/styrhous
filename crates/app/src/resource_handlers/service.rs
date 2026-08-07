use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, minimal_resource_from_typed, namespaced_typed_watcher,
};
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers::{matches_namespaced_api_resource, matches_namespaced_resource};
use crate::resource_table::{
    CLUSTER_IP_COLUMN, CellValue, PORTS_COLUMN, ResourceTableDefinition, TYPE_COLUMN, column,
};
use k8s_openapi::api::core::v1::Service;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    matches_namespaced_resource::<Service>(&context)
        .then(|| namespaced_typed_watcher::<Service>(context, extract))
}

pub(crate) fn table_definition(api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    matches_namespaced_api_resource::<Service>(api_resource).then(|| ResourceTableDefinition {
        columns: vec![
            column(TYPE_COLUMN, "Type", 104.0),
            column(CLUSTER_IP_COLUMN, "Cluster IP", 116.0),
            column(PORTS_COLUMN, "Ports", 128.0),
        ],
    })
}

fn extract(resource: &Service) -> MinimalResource {
    let spec = resource.spec.as_ref();
    let ports = spec
        .and_then(|spec| spec.ports.as_ref())
        .map(|ports| {
            ports
                .iter()
                .map(|port| {
                    format!(
                        "{}/{}",
                        port.port,
                        port.protocol.as_deref().unwrap_or("TCP")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    minimal_resource_from_typed(
        resource,
        BTreeMap::from([
            (
                TYPE_COLUMN.to_owned(),
                CellValue::Text(
                    spec.and_then(|spec| spec.type_.clone())
                        .unwrap_or_else(|| "ClusterIP".to_owned()),
                ),
            ),
            (
                CLUSTER_IP_COLUMN.to_owned(),
                CellValue::Text(
                    spec.and_then(|spec| spec.cluster_ip.clone())
                        .unwrap_or_else(|| "-".to_owned()),
                ),
            ),
            (PORTS_COLUMN.to_owned(), CellValue::List(ports)),
        ]),
    )
}
use crate::api_resource::ApiResource;
