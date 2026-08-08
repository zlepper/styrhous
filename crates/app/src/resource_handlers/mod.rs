pub(crate) mod cluster_metadata;
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

pub(crate) trait ResourceHandler: Sync {
    fn watcher(&self, context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>>;
    fn table_definition(&self, api_resource: &ApiResource) -> Option<ResourceTableDefinition>;
}

struct HandlerDefinition {
    watcher: fn(TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>>,
    table_definition: fn(&ApiResource) -> Option<ResourceTableDefinition>,
}

impl ResourceHandler for HandlerDefinition {
    fn watcher(&self, context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
        (self.watcher)(context)
    }

    fn table_definition(&self, api_resource: &ApiResource) -> Option<ResourceTableDefinition> {
        (self.table_definition)(api_resource)
    }
}

static POD_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: pod::watcher,
    table_definition: pod::table_definition,
};
static DEPLOYMENT_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: deployment::watcher,
    table_definition: deployment::table_definition,
};
static STATEFUL_SET_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: stateful_set::watcher,
    table_definition: stateful_set::table_definition,
};
static DAEMON_SET_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: daemon_set::watcher,
    table_definition: daemon_set::table_definition,
};
static REPLICA_SET_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: replica_set::watcher,
    table_definition: replica_set::table_definition,
};
static REPLICATION_CONTROLLER_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: replication_controller::watcher,
    table_definition: replication_controller::table_definition,
};
static JOB_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: job::watcher,
    table_definition: job::table_definition,
};
static CRON_JOB_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: cron_job::watcher,
    table_definition: cron_job::table_definition,
};
static SERVICE_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: service::watcher,
    table_definition: service::table_definition,
};
static NODE_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: node::watcher,
    table_definition: node::table_definition,
};
static PERSISTENT_VOLUME_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: persistent_volume::watcher,
    table_definition: persistent_volume::table_definition,
};
static STORAGE_CLASS_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: storage_class::watcher,
    table_definition: storage_class::table_definition,
};
static METADATA_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: metadata::watcher,
    table_definition: metadata::table_definition,
};
static CLUSTER_METADATA_HANDLER: HandlerDefinition = HandlerDefinition {
    watcher: cluster_metadata::watcher,
    table_definition: cluster_metadata::table_definition,
};

static HANDLERS: [&dyn ResourceHandler; 14] = [
    &POD_HANDLER,
    &DEPLOYMENT_HANDLER,
    &STATEFUL_SET_HANDLER,
    &DAEMON_SET_HANDLER,
    &REPLICA_SET_HANDLER,
    &REPLICATION_CONTROLLER_HANDLER,
    &JOB_HANDLER,
    &CRON_JOB_HANDLER,
    &SERVICE_HANDLER,
    &NODE_HANDLER,
    &PERSISTENT_VOLUME_HANDLER,
    &STORAGE_CLASS_HANDLER,
    &METADATA_HANDLER,
    &CLUSTER_METADATA_HANDLER,
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
        .find_map(|handler| handler.table_definition(api_resource))
        .unwrap_or_default()
}

pub(crate) fn watcher_for(context: TypedWatcherContext) -> Option<Box<dyn ResourceWatcher>> {
    HANDLERS
        .iter()
        .find_map(|handler| handler.watcher(context.clone()))
}

/// Builds the resource-specific portion of a detail response. The generic metadata
/// is always present, even when no handler recognises the resource.
pub(crate) fn detail_payload(
    api_resource: &ApiResource,
    object: &kube::api::DynamicObject,
) -> ResourceDetailPayload {
    if matches_namespaced_api_resource::<k8s_openapi::api::core::v1::Pod>(api_resource) {
        return pod::detail_payload(object).unwrap_or(ResourceDetailPayload::Generic);
    }
    ResourceDetailPayload::Generic
}
