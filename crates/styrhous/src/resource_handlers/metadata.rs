//! Typed handlers for curated resources whose first useful table is metadata-only.
//!
//! A resource gets its own module once it gains type-specific columns; keeping this
//! shared handler prevents empty per-kind modules from obscuring those implementations.

use crate::api_resource::ApiResource;
use crate::cluster_connection_manager::{
    ResourceWatcher, TypedWatcherContext, namespaced_typed_watcher,
};
use crate::minimal_resource::{MinimalResource, from_kubernetes_resource};
use crate::resource_handlers::matches_namespaced_resource;
use crate::resource_table::ResourceTableDefinition;
use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
use k8s_openapi::api::coordination::v1::Lease;
use k8s_openapi::api::core::v1::{
    ConfigMap, Endpoints, Event, LimitRange, PersistentVolumeClaim, ResourceQuota, Secret,
    ServiceAccount,
};
use k8s_openapi::api::discovery::v1::EndpointSlice;
use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
use k8s_openapi::api::policy::v1::PodDisruptionBudget;
use k8s_openapi::api::rbac::v1::{Role, RoleBinding};
use kube::Resource;
use std::collections::BTreeMap;

pub(crate) fn watcher(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    macro_rules! typed_handler {
        ($type:ty) => {
            if matches_namespaced_resource::<$type>(&context) {
                return Some(namespaced_typed_watcher::<$type>(context, extract));
            }
        };
    }

    typed_handler!(ConfigMap);
    typed_handler!(Secret);
    typed_handler!(ResourceQuota);
    typed_handler!(LimitRange);
    typed_handler!(PersistentVolumeClaim);
    typed_handler!(ServiceAccount);
    typed_handler!(Endpoints);
    typed_handler!(HorizontalPodAutoscaler);
    typed_handler!(PodDisruptionBudget);
    typed_handler!(Lease);
    typed_handler!(EndpointSlice);
    typed_handler!(Ingress);
    typed_handler!(NetworkPolicy);
    typed_handler!(Role);
    typed_handler!(RoleBinding);
    typed_handler!(Event);
    None
}

pub(crate) fn table_definition(_api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
    None
}

fn extract<T: Resource>(resource: &T) -> MinimalResource {
    from_kubernetes_resource(resource, BTreeMap::new())
}
