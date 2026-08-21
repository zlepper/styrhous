use crate::api_resource::ApiResource;
use crate::helm_release::{HelmRelease, StorageDriver, decode_release};
use crate::helpers::ResultExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::pod_metrics::{
    NodeUsage, POD_METRICS_POLL_INTERVAL, PodUsage, node_usage_from_value, pod_usage_from_value,
};
use crate::resource_detail::{
    ManagedResource, ManagedResourceAssociation, PodEnvironmentVariableDetail,
    PodEnvironmentVariableSource, ResourceDetail, ResourceDetailPayload, ResourceEvent,
    ResourceOwner,
};
use crate::resource_handlers;
use crate::resource_schema::ResourceSchema;
use crate::resource_table::{CellValue, CustomResourceColumn};
use crate::worker::*;
use anyhow::{Context, Result, bail};
use futures_util::future::try_join_all;
use futures_util::pin_mut;
use futures_util::stream::StreamExt;
use http::Request;
use k8s_openapi::api::apps::v1::{Deployment, ReplicaSet};
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{ConfigMap, Event as KubernetesEvent, Namespace, Pod, Secret};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    APIGroup, GroupVersionForDiscovery, ObjectMeta, OwnerReference,
};
use k8s_openapi::{ClusterResourceScope, NamespaceResourceScope};
use kube::api::{DeleteParams, DynamicObject, GroupVersionKind, ListParams, Preconditions};
use kube::runtime::watcher;
use kube::runtime::watcher::{Event, ListSemantic};
use kube::{Api, Resource};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use time::OffsetDateTime;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

mod connection;
mod discovery;
mod dynamic_api;
mod resource_data;
mod resource_yaml;

pub use connection::{
    Cluster, ClusterConnection, kubeconfig_context_references, reload_kubeconfig,
};
pub use discovery::{
    AvailableAksCluster, AvailableTailscaleCluster, ClusterDiscovery, ClusterDiscoveryTools,
    add_aks_cluster, add_tailscale_cluster, discover_managed_clusters,
};

mod api_inspection;
mod detail_watches;
mod extraction;
mod managed_watches;
mod namespace_watcher;
mod resource_actions;
mod resource_details;
mod resource_mutations;
mod resource_mutations_prelude;
mod watcher_orchestration;
mod watcher_scopes;
mod watcher_types;

pub(crate) use api_inspection::*;
pub(crate) use detail_watches::*;
pub(crate) use extraction::*;
pub(crate) use managed_watches::*;
pub(crate) use namespace_watcher::*;
pub(crate) use resource_actions::*;
pub(crate) use resource_details::*;
pub(crate) use resource_mutations::*;
pub(crate) use resource_mutations_prelude::*;
pub(crate) use watcher_orchestration::*;
pub(crate) use watcher_scopes::*;
pub(crate) use watcher_types::*;

#[cfg(test)]
mod tests;
