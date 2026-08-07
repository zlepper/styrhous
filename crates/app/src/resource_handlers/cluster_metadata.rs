//! Typed handlers for cluster-scoped curated resources without custom columns yet.

use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, cluster_typed_watcher, minimal_resource_from_typed,
};
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers::matches_cluster_resource;
use crate::resource_table::ResourceTableDefinition;
use k8s_openapi::api::core::v1::Namespace;
use k8s_openapi::api::networking::v1::IngressClass;
use k8s_openapi::api::node::v1::RuntimeClass;
use k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding};
use k8s_openapi::api::scheduling::v1::PriorityClass;
use kube::Resource;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    macro_rules! typed_handler {
        ($type:ty) => {
            if matches_cluster_resource::<$type>(&context) {
                return Some(cluster_typed_watcher::<$type>(context, extract));
            }
        };
    }

    typed_handler!(Namespace);
    typed_handler!(PriorityClass);
    typed_handler!(RuntimeClass);
    typed_handler!(IngressClass);
    typed_handler!(ClusterRole);
    typed_handler!(ClusterRoleBinding);
    None
}

pub(crate) fn table_definition(_api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    None
}

fn extract<T: Resource>(resource: &T) -> MinimalResource {
    minimal_resource_from_typed(resource, BTreeMap::new())
}
