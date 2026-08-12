pub(crate) mod cluster_metadata;
pub(crate) mod config_map;
pub(crate) mod cron_job;
pub(crate) mod daemon_set;
pub(crate) mod deployment;
pub(crate) mod job;
pub(crate) mod metadata;
pub(crate) mod node;
pub(crate) mod persistent_volume;
pub(crate) mod pod;
pub(crate) mod replica_set;
pub(crate) mod replication_controller;
pub(crate) mod secret;
pub(crate) mod service;
pub(crate) mod stateful_set;
pub(crate) mod storage_class;

use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{ResourceWatcher, TypedWatcherContext};
use crate::resource_detail::ResourceDetailPayload;
use crate::resource_table::{
    CustomResourceColumn, ResourceTableDefinition, custom_table_definition,
};
use k8s_openapi::{ClusterResourceScope, NamespaceResourceScope};
use kube::Resource;

pub(crate) fn matches_namespaced_resource<T>(context: &TypedWatcherContext) -> bool
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>,
{
    matches_api_resource::<T>(&context.api_resource, true)
}

pub(crate) fn matches_cluster_resource<T>(context: &TypedWatcherContext) -> bool
where
    T: Resource<DynamicType = (), Scope = ClusterResourceScope>,
{
    matches_api_resource::<T>(&context.api_resource, false)
}

pub(crate) fn matches_namespaced_api_resource<T>(api_resource: &ApiResource) -> bool
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>,
{
    matches_api_resource::<T>(api_resource, true)
}

pub(crate) fn matches_cluster_api_resource<T>(api_resource: &ApiResource) -> bool
where
    T: Resource<DynamicType = (), Scope = ClusterResourceScope>,
{
    matches_api_resource::<T>(api_resource, false)
}

fn matches_api_resource<T>(api_resource: &ApiResource, namespaced: bool) -> bool
where
    T: Resource<DynamicType = ()>,
{
    let group = T::group(&());
    let group = if group.is_empty() {
        "core"
    } else {
        group.as_ref()
    };
    api_resource.group == group
        && api_resource.version == T::version(&())
        && api_resource.kind == T::kind(&())
        && api_resource.name == T::plural(&())
        && api_resource.namespaced == namespaced
}

#[derive(Clone, Copy)]
struct HandlerDefinition {
    watcher: fn(TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>>,
    table_definition: fn(&ApiResource) -> Option<ResourceTableDefinition>,
}

struct DetailHandler {
    matches: fn(&ApiResource) -> bool,
    detail_payload: fn(&kube::api::DynamicObject) -> Option<ResourceDetailPayload>,
}

macro_rules! handler {
    ($module:ident) => {
        HandlerDefinition {
            watcher: $module::watcher,
            table_definition: $module::table_definition,
        }
    };
}

const HANDLERS: [HandlerDefinition; 14] = [
    handler!(pod),
    handler!(deployment),
    handler!(stateful_set),
    handler!(daemon_set),
    handler!(replica_set),
    handler!(replication_controller),
    handler!(job),
    handler!(cron_job),
    handler!(service),
    handler!(node),
    handler!(persistent_volume),
    handler!(storage_class),
    handler!(metadata),
    handler!(cluster_metadata),
];

const DETAIL_HANDLERS: [DetailHandler; 4] = [
    DetailHandler {
        matches: matches_namespaced_api_resource::<k8s_openapi::api::core::v1::Pod>,
        detail_payload: pod::detail_payload,
    },
    DetailHandler {
        matches: matches_cluster_api_resource::<k8s_openapi::api::core::v1::Node>,
        detail_payload: node::detail_payload,
    },
    DetailHandler {
        matches: matches_namespaced_api_resource::<k8s_openapi::api::core::v1::ConfigMap>,
        detail_payload: config_map::detail_payload,
    },
    DetailHandler {
        matches: matches_namespaced_api_resource::<k8s_openapi::api::core::v1::Secret>,
        detail_payload: secret::detail_payload,
    },
];

pub(crate) fn table_definition(
    api_resource: &ApiResource,
    custom_columns: &[CustomResourceColumn],
) -> ResourceTableDefinition {
    if !custom_columns.is_empty() {
        return custom_table_definition(custom_columns);
    }
    HANDLERS
        .iter()
        .find_map(|handler| (handler.table_definition)(api_resource))
        .unwrap_or_default()
}

pub(crate) fn watcher_for(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    HANDLERS
        .iter()
        .find_map(|handler| (handler.watcher)(context.clone()))
}

/// Builds the resource-specific portion of a detail response. The generic metadata
/// is always present, even when no handler recognises the resource.
pub(crate) fn detail_payload(
    api_resource: &ApiResource,
    object: &kube::api::DynamicObject,
) -> ResourceDetailPayload {
    DETAIL_HANDLERS
        .iter()
        .find(|handler| (handler.matches)(api_resource))
        .and_then(|handler| (handler.detail_payload)(object))
        .unwrap_or(ResourceDetailPayload::Generic)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_resource(kind: &str, name: &str) -> ApiResource {
        ApiResource {
            group: "core".to_owned(),
            version: "v1".to_owned(),
            kind: kind.to_owned(),
            name: name.to_owned(),
            namespaced: true,
        }
    }

    #[test]
    fn registry_routes_config_map_detail_payloads() {
        let object = k8s_openapi::serde_json::from_value(k8s_openapi::serde_json::json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {"name": "settings"},
            "data": {"theme": "dark"}
        }))
        .unwrap();

        assert!(matches!(
            detail_payload(&api_resource("ConfigMap", "configmaps"), &object),
            ResourceDetailPayload::ConfigMap(_)
        ));
    }

    #[test]
    fn registry_routes_secret_and_cluster_scoped_node_detail_payloads() {
        let secret = k8s_openapi::serde_json::from_value(k8s_openapi::serde_json::json!({
            "apiVersion": "v1", "kind": "Secret", "metadata": {"name": "credentials"}
        }))
        .unwrap();
        assert!(matches!(
            detail_payload(&api_resource("Secret", "secrets"), &secret),
            ResourceDetailPayload::Secret(_)
        ));

        let node = k8s_openapi::serde_json::from_value(k8s_openapi::serde_json::json!({
            "apiVersion": "v1", "kind": "Node", "metadata": {"name": "node-1"}
        }))
        .unwrap();
        let node_resource = ApiResource {
            namespaced: false,
            ..api_resource("Node", "nodes")
        };
        assert!(matches!(
            detail_payload(&node_resource, &node),
            ResourceDetailPayload::Node(_)
        ));
    }

    #[test]
    fn registry_selects_representative_typed_table_definitions() {
        assert!(
            !table_definition(&api_resource("Pod", "pods"), &[])
                .columns
                .is_empty()
        );
        let node_resource = ApiResource {
            namespaced: false,
            ..api_resource("Node", "nodes")
        };
        assert!(!table_definition(&node_resource, &[]).columns.is_empty());
        assert!(
            table_definition(
                &ApiResource {
                    group: "example.dev".into(),
                    version: "v1".into(),
                    kind: "Backup".into(),
                    name: "backups".into(),
                    namespaced: true,
                },
                &[],
            )
            .columns
            .is_empty()
        );
    }

    #[test]
    fn registry_keeps_unknown_resource_payloads_generic() {
        let object = k8s_openapi::serde_json::from_value(k8s_openapi::serde_json::json!({
            "apiVersion": "example.dev/v1",
            "kind": "Backup",
            "metadata": {"name": "nightly"}
        }))
        .unwrap();
        let resource = ApiResource {
            group: "example.dev".to_owned(),
            version: "v1".to_owned(),
            kind: "Backup".to_owned(),
            name: "backups".to_owned(),
            namespaced: true,
        };

        assert!(matches!(
            detail_payload(&resource, &object),
            ResourceDetailPayload::Generic
        ));
    }
}
