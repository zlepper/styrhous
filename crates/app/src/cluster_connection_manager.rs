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
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{ConfigMap, Event as KubernetesEvent, Namespace, Pod, Secret};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    APIGroup, GroupVersionForDiscovery, ObjectMeta,
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
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

mod connection;
mod dynamic_api;
mod resource_data;
mod resource_yaml;

pub use connection::{Cluster, ClusterConnection, reload_kubeconfig};

struct KubernetesApiInspector {
    client: kube::Client,
}

struct ApiInspection {
    api_resources: Vec<ApiResource>,
    scalable_api_resources: BTreeSet<ApiResource>,
    pod_metrics_api_available: bool,
    node_metrics_api_available: bool,
    custom_resource_columns: BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    resource_schemas: BTreeMap<ApiResource, ResourceSchema>,
}

struct DiscoveredApiResources {
    api_resources: Vec<ApiResource>,
    scalable_api_resources: BTreeSet<ApiResource>,
}

impl KubernetesApiInspector {
    async fn get_api_resources_for_group_versions(
        &self,
        api_group: APIGroup,
        versions: Vec<GroupVersionForDiscovery>,
    ) -> Result<DiscoveredApiResources> {
        let tasks = versions.iter().map(|api_group_version| {
            self.client
                .list_api_group_resources(&api_group_version.group_version)
        });

        let api_group_name = api_group.name;
        let resources = try_join_all(tasks)
            .await?
            .iter()
            .zip(versions)
            .map(|(resources, version)| {
                let version_name = version.version.clone();

                let mut api_resources = Vec::new();
                let mut scalable_api_resources = BTreeSet::new();

                for resource in &resources.resources {
                    // Skip resources like "Status" and "Scale"
                    if resource.name.contains('/') {
                        continue;
                    }

                    let api_resource = ApiResource {
                        group: api_group_name.clone(),
                        version: version_name.clone(),
                        kind: resource.kind.clone(),
                        name: resource.name.clone(),
                        namespaced: resource.namespaced,
                    };
                    if supports_scale_subresource(&resources.resources, &resource.name) {
                        scalable_api_resources.insert(api_resource.clone());
                    }
                    api_resources.push(api_resource);
                }

                DiscoveredApiResources {
                    api_resources,
                    scalable_api_resources,
                }
            })
            .fold(
                DiscoveredApiResources {
                    api_resources: Vec::new(),
                    scalable_api_resources: BTreeSet::new(),
                },
                |mut all, discovered| {
                    all.api_resources.extend(discovered.api_resources);
                    all.scalable_api_resources
                        .extend(discovered.scalable_api_resources);
                    all
                },
            );

        Ok(resources)
    }

    async fn get_core_api_resources(&self) -> Result<DiscoveredApiResources> {
        let core_api_versions = self.client.list_core_api_versions().await?;

        let mut discovered = DiscoveredApiResources {
            api_resources: Vec::new(),
            scalable_api_resources: BTreeSet::new(),
        };

        for version in &core_api_versions.versions {
            let api_resources = self.client.list_core_api_resources(version).await?;

            for resource in &api_resources.resources {
                if resource.name.contains("/") {
                    continue;
                }

                let api_resource = ApiResource {
                    group: "core".to_string(),
                    version: version.clone(),
                    kind: resource.kind.clone(),
                    name: resource.name.clone(),
                    namespaced: resource.namespaced,
                };
                if supports_scale_subresource(&api_resources.resources, &resource.name) {
                    discovered
                        .scalable_api_resources
                        .insert(api_resource.clone());
                }
                discovered.api_resources.push(api_resource);
            }
        }

        Ok(discovered)
    }

    pub async fn inspect_api(&self) -> Result<ApiInspection> {
        let api_groups = self.client.list_api_groups().await?;

        let tasks = api_groups.groups.into_iter().map(|api_group| {
            let versions = api_group
                .preferred_version
                .clone()
                .map(|v| vec![v])
                .unwrap_or_else(|| api_group.versions.clone());

            self.get_api_resources_for_group_versions(api_group, versions)
        });

        let core_resources = self.get_core_api_resources().await?;

        let discovered_resources =
            try_join_all(tasks)
                .await?
                .into_iter()
                .fold(core_resources, |mut all, discovered| {
                    all.api_resources.extend(discovered.api_resources);
                    all.scalable_api_resources
                        .extend(discovered.scalable_api_resources);
                    all
                });

        let pod_metrics_api_available =
            pod_metrics_api_available(&discovered_resources.api_resources);
        let node_metrics_api_available =
            node_metrics_api_available(&discovered_resources.api_resources);
        let (custom_resource_columns, resource_schemas) = self.custom_resource_metadata().await;
        Ok(ApiInspection {
            api_resources: discovered_resources.api_resources,
            scalable_api_resources: discovered_resources.scalable_api_resources,
            pod_metrics_api_available,
            node_metrics_api_available,
            custom_resource_columns,
            resource_schemas,
        })
    }

    async fn custom_resource_metadata(
        &self,
    ) -> (
        BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
        BTreeMap<ApiResource, ResourceSchema>,
    ) {
        let crds = Api::<CustomResourceDefinition>::all(self.client.clone());
        let Ok(crds) = crds.list(&Default::default()).await else {
            // Access to CRDs is commonly restricted. Dynamic resources still work without
            // their optional columns, so do not fail API discovery in that case.
            return (BTreeMap::new(), BTreeMap::new());
        };

        let mut columns_by_resource = BTreeMap::new();
        let mut schemas_by_resource = BTreeMap::new();
        for crd in &crds.items {
            let spec = &crd.spec;
            for version in &spec.versions {
                let api_resource = ApiResource {
                    group: spec.group.clone(),
                    version: version.name.clone(),
                    kind: spec.names.kind.clone(),
                    name: spec.names.plural.clone(),
                    namespaced: spec.scope == "Namespaced",
                };
                if let Some(columns) = &version.additional_printer_columns {
                    columns_by_resource.insert(
                        api_resource.clone(),
                        columns
                            .iter()
                            .enumerate()
                            .map(|(index, column)| CustomResourceColumn {
                                id: format!("crd-{index}"),
                                label: column.name.clone(),
                                json_path: column.json_path.clone(),
                                type_: column.type_.clone(),
                                format: column.format.clone(),
                            })
                            .collect(),
                    );
                }
                if let Some(schema) = version
                    .schema
                    .as_ref()
                    .and_then(|schema| schema.open_api_v3_schema.as_ref())
                    && let Ok(root) = k8s_openapi::serde_json::to_value(schema)
                {
                    schemas_by_resource.insert(api_resource, ResourceSchema::new(root));
                }
            }
        }
        (columns_by_resource, schemas_by_resource)
    }

    async fn custom_resource_columns(&self) -> BTreeMap<ApiResource, Vec<CustomResourceColumn>> {
        self.custom_resource_metadata().await.0
    }
}

fn pod_metrics_api_available(api_resources: &[ApiResource]) -> bool {
    metrics_api_available(api_resources, "PodMetrics", "pods")
}

fn node_metrics_api_available(api_resources: &[ApiResource]) -> bool {
    metrics_api_available(api_resources, "NodeMetrics", "nodes")
}

fn metrics_api_available(api_resources: &[ApiResource], kind: &str, name: &str) -> bool {
    api_resources.iter().any(|resource| {
        resource.group == "metrics.k8s.io"
            && resource.version == "v1beta1"
            && resource.kind == kind
            && resource.name == name
    })
}

struct KubernetesNamespaceWatcher {
    client: kube::Client,
    event_sender: WorkerResultSender,
    cluster_key: i32,
}

impl KubernetesNamespaceWatcher {
    async fn watch_namespaces(self) {
        let namespace_api = Api::<Namespace>::all(self.client.clone());

        let mut buffer = Vec::<MinimalNamespace>::new();

        let stream = watcher(namespace_api, watcher_config());
        pin_mut!(stream);

        while let Some(event) = stream.next().await {
            let ev = match event {
                Ok(event) => event,
                Err(error) => {
                    warn!("Namespace watcher error: {error:?}");
                    self.event_sender
                        .send(KubernetesNamespacesLoadFailed {
                            cluster_key: self.cluster_key,
                            error: format!("{error:#?}"),
                        })
                        .await
                        .log_if_error("Failed to send namespace watcher error");
                    return;
                }
            };
            match ev {
                Event::Apply(item) => {
                    self.event_sender
                        .send(KubernetesNamespacesAdded {
                            namespace: item.into(),
                            cluster_key: self.cluster_key,
                        })
                        .await
                        .log_if_error("Failed to send updated namespace");
                }
                Event::Delete(item) => {
                    self.event_sender
                        .send(KubernetesNamespacesDeleted {
                            cluster_key: self.cluster_key,
                            namespace_name: item.metadata.name.expect(
                                "Deleted Namespace from the api server did not have a name",
                            ),
                        })
                        .await
                        .log_if_error("Failed to send notification about deleted namespace");
                }
                Event::Init => {
                    buffer.clear();
                }
                Event::InitApply(item) => {
                    buffer.push(item.into());
                }
                Event::InitDone => {
                    self.event_sender
                        .send(KubernetesNamespacesReplaced {
                            cluster_key: self.cluster_key,
                            namespaces: buffer,
                        })
                        .await
                        .log_if_error("Failed to send entire replaced namespace list");
                    buffer = Vec::new();
                }
            }
        }
    }
}

fn watcher_config() -> watcher::Config {
    watcher::Config {
        list_semantic: ListSemantic::Any,
        initial_list_strategy: watcher::InitialListStrategy::ListWatch,
        ..Default::default()
    }
}

pub async fn start_cluster_connection(
    cluster_key: i32,
    cluster_name: &str,
    event_sender: WorkerResultSender,
) -> Result<ClusterConnection> {
    info!("Starting cluster connection: {}", cluster_name);
    ClusterConnection::new(cluster_key, cluster_name, event_sender).await
}

/// Start watching a resource type in its selected namespace scope.
pub async fn start_resource_watcher(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    event_sender: WorkerResultSender,
) -> Result<(KubernetesResourceWatchStarted, tokio::task::JoinHandle<()>)> {
    info!(
        "Starting resource watcher for {}/{} in {}",
        api_resource.group,
        api_resource.name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    if api_resource.is_helm_releases() {
        let namespace = namespace.context("Helm releases require a namespace")?;
        let task = tokio::spawn(watch_helm_releases(
            cluster_key,
            client,
            namespace.clone(),
            event_sender,
        ));
        return Ok((
            KubernetesResourceWatchStarted {
                cluster_key,
                api_resource,
                namespace: Some(namespace),
            },
            task,
        ));
    }

    let context = TypedWatcherContext {
        client: client.clone(),
        event_sender: event_sender.clone(),
        cluster_key,
        api_resource: api_resource.clone(),
        namespace: namespace.clone(),
    };
    let watcher = if let Some(watcher) = resource_handlers::watcher_for(context) {
        watcher
    } else {
        let custom_columns = KubernetesApiInspector {
            client: client.clone(),
        }
        .custom_resource_columns()
        .await
        .remove(&api_resource)
        .unwrap_or_default();
        event_sender
            .send(KubernetesCustomResourceColumnsLoaded {
                cluster_key,
                columns: BTreeMap::from([(api_resource.clone(), custom_columns.clone())]),
            })
            .await
            .log_if_error("Failed to send custom resource columns");
        Box::new(DynamicKubernetesResourceWatcher {
            client,
            event_sender,
            cluster_key,
            api_resource: api_resource.clone(),
            namespace: namespace.clone(),
            custom_columns,
        })
    };

    let task = tokio::spawn(watcher.watch_resources());

    Ok((
        KubernetesResourceWatchStarted {
            cluster_key,
            api_resource,
            namespace,
        },
        task,
    ))
}

async fn watch_helm_releases(
    cluster_key: i32,
    client: kube::Client,
    namespace: String,
    event_sender: WorkerResultSender,
) {
    let secrets = Api::<Secret>::namespaced(client.clone(), &namespace);
    let config_maps = Api::<ConfigMap>::namespaced(client, &namespace);
    let secret_stream = watcher(secrets, watcher_config().labels("owner=helm")).boxed();
    let config_map_stream = watcher(config_maps, watcher_config().labels("owner=helm")).boxed();
    futures_util::pin_mut!(secret_stream);
    futures_util::pin_mut!(config_map_stream);
    let mut records = BTreeMap::<String, HelmRelease>::new();
    let mut secrets_active = true;
    let mut config_maps_active = true;
    let mut secrets_synced = false;
    let mut config_maps_synced = false;

    while secrets_active || config_maps_active {
        tokio::select! {
            event = secret_stream.next(), if secrets_active => match event {
                Some(Ok(event)) => {
                    secrets_synced |= matches!(&event, Event::InitDone);
                    let changed = apply_helm_secret_event(event, &mut records);
                    if changed && secrets_synced && config_maps_synced { send_helm_releases(&event_sender, cluster_key, &namespace, &records).await; }
                }
                Some(Err(error)) => {
                    secrets_active = false;
                    secrets_synced = true;
                    event_sender.send(HelmReleaseBackendFailed { cluster_key, namespace: namespace.clone(), backend: "Secrets", error: format!("{error:#}") }).await.log_if_error("Failed to send Helm Secrets error");
                    if config_maps_synced {
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                }
                None => {
                    secrets_active = false;
                    secrets_synced = true;
                    if config_maps_synced {
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                },
            },
            event = config_map_stream.next(), if config_maps_active => match event {
                Some(Ok(event)) => {
                    config_maps_synced |= matches!(&event, Event::InitDone);
                    let changed = apply_helm_config_map_event(event, &mut records);
                    if changed && secrets_synced && config_maps_synced { send_helm_releases(&event_sender, cluster_key, &namespace, &records).await; }
                }
                Some(Err(error)) => {
                    config_maps_active = false;
                    config_maps_synced = true;
                    event_sender.send(HelmReleaseBackendFailed { cluster_key, namespace: namespace.clone(), backend: "ConfigMaps", error: format!("{error:#}") }).await.log_if_error("Failed to send Helm ConfigMaps error");
                    if secrets_synced {
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                }
                None => {
                    config_maps_active = false;
                    config_maps_synced = true;
                    if secrets_synced {
                        send_helm_releases(&event_sender, cluster_key, &namespace, &records).await;
                    }
                },
            },
        }
    }
}

async fn send_helm_releases(
    event_sender: &WorkerResultSender,
    cluster_key: i32,
    namespace: &str,
    records: &BTreeMap<String, HelmRelease>,
) {
    let releases = merged_helm_releases(records);
    event_sender
        .send(HelmReleasesReplaced {
            cluster_key,
            namespace: namespace.to_owned(),
            releases,
        })
        .await
        .log_if_error("Failed to send Helm releases");
}

fn merged_helm_releases(records: &BTreeMap<String, HelmRelease>) -> Vec<HelmRelease> {
    let mut releases = BTreeMap::<(String, String, i64), HelmRelease>::new();
    for release in records.values().cloned() {
        let key = (
            release.namespace.clone(),
            release.name.clone(),
            release.revision,
        );
        match releases.get(&key) {
            Some(existing) if existing.storage == StorageDriver::Secret => {}
            _ => {
                releases.insert(key, release);
            }
        }
    }
    releases.into_values().collect()
}

fn helm_storage_key(prefix: &str, name: &str) -> String {
    format!("{prefix}/{name}")
}

fn apply_helm_secret_event(
    event: Event<Secret>,
    records: &mut BTreeMap<String, HelmRelease>,
) -> bool {
    match event {
        Event::Apply(secret) => upsert_helm_secret(secret, records),
        Event::Delete(secret) => records
            .remove(&helm_storage_key(
                "secret",
                secret.metadata.name.as_deref().unwrap_or_default(),
            ))
            .is_some(),
        Event::Init => {
            records.retain(|key, _| !key.starts_with("secret/"));
            false
        }
        Event::InitApply(secret) => upsert_helm_secret(secret, records),
        Event::InitDone => true,
    }
}

fn apply_helm_config_map_event(
    event: Event<ConfigMap>,
    records: &mut BTreeMap<String, HelmRelease>,
) -> bool {
    match event {
        Event::Apply(config_map) => upsert_helm_config_map(config_map, records),
        Event::Delete(config_map) => records
            .remove(&helm_storage_key(
                "configmap",
                config_map.metadata.name.as_deref().unwrap_or_default(),
            ))
            .is_some(),
        Event::Init => {
            records.retain(|key, _| !key.starts_with("configmap/"));
            false
        }
        Event::InitApply(config_map) => upsert_helm_config_map(config_map, records),
        Event::InitDone => true,
    }
}

fn upsert_helm_secret(secret: Secret, records: &mut BTreeMap<String, HelmRelease>) -> bool {
    let name = secret.metadata.name.unwrap_or_default();
    let key = helm_storage_key("secret", &name);
    let Some(encoded) = secret.data.and_then(|data| data.get("release").cloned()) else {
        return records.remove(&key).is_some();
    };
    match decode_release(StorageDriver::Secret, name, &encoded.0) {
        Ok(mut release) => {
            release.storage_labels = secret.metadata.labels.unwrap_or_default();
            release.storage_annotations = secret.metadata.annotations.unwrap_or_default();
            records.insert(key, release);
            true
        }
        Err(_) => records.remove(&key).is_some(),
    }
}

fn upsert_helm_config_map(
    config_map: ConfigMap,
    records: &mut BTreeMap<String, HelmRelease>,
) -> bool {
    let name = config_map.metadata.name.unwrap_or_default();
    let key = helm_storage_key("configmap", &name);
    let Some(encoded) = config_map
        .data
        .and_then(|data| data.get("release").cloned())
    else {
        return records.remove(&key).is_some();
    };
    match decode_release(StorageDriver::ConfigMap, name, encoded.as_bytes()) {
        Ok(mut release) => {
            release.storage_labels = config_map.metadata.labels.unwrap_or_default();
            release.storage_annotations = config_map.metadata.annotations.unwrap_or_default();
            records.insert(key, release);
            true
        }
        Err(_) => records.remove(&key).is_some(),
    }
}

type ResourceWatcherFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Object-safe dispatch point for concrete Kubernetes resource watchers.
pub(crate) trait ResourceWatcher: Send {
    fn watch_resources(self: Box<Self>) -> ResourceWatcherFuture;
}

#[derive(Clone)]
pub(crate) struct TypedWatcherContext {
    pub(crate) client: kube::Client,
    pub(crate) event_sender: WorkerResultSender,
    pub(crate) cluster_key: i32,
    pub(crate) api_resource: ApiResource,
    pub(crate) namespace: Option<String>,
}

struct DynamicKubernetesResourceWatcher {
    client: kube::Client,
    event_sender: WorkerResultSender,
    cluster_key: i32,
    api_resource: ApiResource,
    namespace: Option<String>,
    custom_columns: Vec<CustomResourceColumn>,
}

impl DynamicKubernetesResourceWatcher {
    async fn watch_resources(self) {
        // Convert our ApiResource to kube's ApiResource using discovery
        let group = if self.api_resource.group == "core" {
            ""
        } else {
            &self.api_resource.group
        };

        let gvk = GroupVersionKind::gvk(group, &self.api_resource.version, &self.api_resource.kind);

        let discovery_result = kube::discovery::pinned_kind(&self.client, &gvk).await;
        let (ar, caps) = match discovery_result {
            Ok(r) => r,
            Err(error) => {
                warn!(
                    "Failed to discover API resource {}/{}: {}",
                    self.api_resource.group, self.api_resource.name, error
                );
                self.event_sender
                    .send(KubernetesResourceWatchFailed {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: self.namespace.clone(),
                        error: format!("{error:#?}"),
                    })
                    .await
                    .log_if_error("Failed to send resource watcher discovery error");
                return;
            }
        };

        let api: Api<DynamicObject> = match (caps.scope, self.namespace.as_deref()) {
            (kube::discovery::Scope::Namespaced, Some(namespace)) => {
                Api::namespaced_with(self.client.clone(), namespace, &ar)
            }
            (kube::discovery::Scope::Cluster, None) => Api::all_with(self.client.clone(), &ar),
            (scope, namespace) => {
                let error = format!(
                    "Resource scope mismatch: discovered {scope:?} scope with namespace {namespace:?}"
                );
                self.event_sender
                    .send(KubernetesResourceWatchFailed {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: self.namespace.clone(),
                        error,
                    })
                    .await
                    .log_if_error("Failed to send resource watcher scope error");
                return;
            }
        };

        let mut buffer = Vec::<MinimalResource>::new();

        let stream = watcher(api, watcher_config());
        pin_mut!(stream);

        while let Some(event) = stream.next().await {
            let ev = match event {
                Ok(event) => event,
                Err(error) => {
                    warn!("Resource watcher error: {error:?}");
                    self.event_sender
                        .send(KubernetesResourceWatchFailed {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            error: format!("{error:#?}"),
                        })
                        .await
                        .log_if_error("Failed to send resource watcher error");
                    return;
                }
            };
            match ev {
                Event::Apply(item) => {
                    let resource = extract_minimal_resource(&item, &self.custom_columns);
                    self.event_sender
                        .send(KubernetesResourceAdded {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            resource,
                        })
                        .await
                        .log_if_error("Failed to send resource added");
                }
                Event::Delete(item) => {
                    let uid = get_resource_uid(&item);
                    self.event_sender
                        .send(KubernetesResourceDeleted {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            resource_uid: uid,
                        })
                        .await
                        .log_if_error("Failed to send resource deleted");
                }
                Event::Init => {
                    buffer.clear();
                }
                Event::InitApply(item) => {
                    buffer.push(extract_minimal_resource(&item, &self.custom_columns));
                }
                Event::InitDone => {
                    self.event_sender
                        .send(KubernetesResourcesReplaced {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            resources: buffer,
                        })
                        .await
                        .log_if_error("Failed to send resources replaced");
                    buffer = Vec::new();
                }
            }
        }
    }
}

impl ResourceWatcher for DynamicKubernetesResourceWatcher {
    fn watch_resources(self: Box<Self>) -> ResourceWatcherFuture {
        Box::pin(async move { (*self).watch_resources().await })
    }
}

struct TypedKubernetesResourceWatcher<T> {
    client: kube::Client,
    event_sender: WorkerResultSender,
    cluster_key: i32,
    api_resource: ApiResource,
    namespace: Option<String>,
    extract: fn(&T) -> MinimalResource,
}

impl<T> TypedKubernetesResourceWatcher<T>
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    async fn watch_resources(self) {
        let Some(namespace) = self.namespace.as_deref() else {
            self.event_sender
                .send(KubernetesResourceWatchFailed {
                    cluster_key: self.cluster_key,
                    api_resource: self.api_resource,
                    namespace: None,
                    error: "A namespaced typed watcher was started without a namespace".to_owned(),
                })
                .await
                .log_if_error("Failed to send resource watcher scope error");
            return;
        };

        let api = Api::<T>::namespaced(self.client.clone(), namespace);
        let mut buffer = Vec::<MinimalResource>::new();
        let stream = watcher(api, watcher_config());
        pin_mut!(stream);

        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    warn!("Typed resource watcher error: {error:?}");
                    self.event_sender
                        .send(KubernetesResourceWatchFailed {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            error: format!("{error:#?}"),
                        })
                        .await
                        .log_if_error("Failed to send typed resource watcher error");
                    return;
                }
            };

            match event {
                Event::Apply(item) => self
                    .event_sender
                    .send(KubernetesResourceAdded {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: self.namespace.clone(),
                        resource: (self.extract)(&item),
                    })
                    .await
                    .log_if_error("Failed to send typed resource added"),
                Event::Delete(item) => self
                    .event_sender
                    .send(KubernetesResourceDeleted {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: self.namespace.clone(),
                        resource_uid: get_resource_uid(&item),
                    })
                    .await
                    .log_if_error("Failed to send typed resource deleted"),
                Event::Init => buffer.clear(),
                Event::InitApply(item) => buffer.push((self.extract)(&item)),
                Event::InitDone => {
                    self.event_sender
                        .send(KubernetesResourcesReplaced {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            resources: buffer,
                        })
                        .await
                        .log_if_error("Failed to send typed resources replaced");
                    buffer = Vec::new();
                }
            }
        }
    }
}

impl<T> ResourceWatcher for TypedKubernetesResourceWatcher<T>
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    fn watch_resources(self: Box<Self>) -> ResourceWatcherFuture {
        Box::pin(async move { (*self).watch_resources().await })
    }
}

pub(crate) fn namespaced_typed_watcher<T>(
    context: TypedWatcherContext,
    extract: fn(&T) -> MinimalResource,
) -> Box<dyn ResourceWatcher>
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    Box::new(TypedKubernetesResourceWatcher {
        client: context.client,
        event_sender: context.event_sender,
        cluster_key: context.cluster_key,
        api_resource: context.api_resource,
        namespace: context.namespace,
        extract,
    })
}

struct ClusterTypedKubernetesResourceWatcher<T> {
    client: kube::Client,
    event_sender: WorkerResultSender,
    cluster_key: i32,
    api_resource: ApiResource,
    namespace: Option<String>,
    extract: fn(&T) -> MinimalResource,
}

impl<T> ClusterTypedKubernetesResourceWatcher<T>
where
    T: Resource<DynamicType = (), Scope = ClusterResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    async fn watch_resources(self) {
        if self.namespace.is_some() {
            self.event_sender
                .send(KubernetesResourceWatchFailed {
                    cluster_key: self.cluster_key,
                    api_resource: self.api_resource,
                    namespace: self.namespace,
                    error: "A cluster-scoped typed watcher was started with a namespace".to_owned(),
                })
                .await
                .log_if_error("Failed to send resource watcher scope error");
            return;
        }

        let api = Api::<T>::all(self.client.clone());
        let mut buffer = Vec::<MinimalResource>::new();
        let stream = watcher(api, watcher_config());
        pin_mut!(stream);

        while let Some(event) = stream.next().await {
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    warn!("Typed resource watcher error: {error:?}");
                    self.event_sender
                        .send(KubernetesResourceWatchFailed {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: None,
                            error: format!("{error:#?}"),
                        })
                        .await
                        .log_if_error("Failed to send typed resource watcher error");
                    return;
                }
            };

            match event {
                Event::Apply(item) => self
                    .event_sender
                    .send(KubernetesResourceAdded {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: None,
                        resource: (self.extract)(&item),
                    })
                    .await
                    .log_if_error("Failed to send typed resource added"),
                Event::Delete(item) => self
                    .event_sender
                    .send(KubernetesResourceDeleted {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: None,
                        resource_uid: get_resource_uid(&item),
                    })
                    .await
                    .log_if_error("Failed to send typed resource deleted"),
                Event::Init => buffer.clear(),
                Event::InitApply(item) => buffer.push((self.extract)(&item)),
                Event::InitDone => {
                    self.event_sender
                        .send(KubernetesResourcesReplaced {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: None,
                            resources: buffer,
                        })
                        .await
                        .log_if_error("Failed to send typed resources replaced");
                    buffer = Vec::new();
                }
            }
        }
    }
}

impl<T> ResourceWatcher for ClusterTypedKubernetesResourceWatcher<T>
where
    T: Resource<DynamicType = (), Scope = ClusterResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    fn watch_resources(self: Box<Self>) -> ResourceWatcherFuture {
        Box::pin(async move { (*self).watch_resources().await })
    }
}

pub(crate) fn cluster_typed_watcher<T>(
    context: TypedWatcherContext,
    extract: fn(&T) -> MinimalResource,
) -> Box<dyn ResourceWatcher>
where
    T: Resource<DynamicType = (), Scope = ClusterResourceScope>
        + Clone
        + Debug
        + Send
        + Sync
        + for<'de> k8s_openapi::serde::Deserialize<'de>
        + 'static,
{
    Box::new(ClusterTypedKubernetesResourceWatcher {
        client: context.client,
        event_sender: context.event_sender,
        cluster_key: context.cluster_key,
        api_resource: context.api_resource,
        namespace: context.namespace,
        extract,
    })
}

/// Get a unique identifier for a resource
fn get_resource_uid<T: Resource>(obj: &T) -> String {
    let metadata = obj.meta();
    metadata.uid.clone().unwrap_or_else(|| {
        format!(
            "{}/{}",
            metadata.namespace.as_deref().unwrap_or(""),
            metadata.name.as_deref().unwrap_or("")
        )
    })
}

fn resource_owners(metadata: &ObjectMeta) -> Vec<ResourceOwner> {
    metadata
        .owner_references
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|owner| ResourceOwner {
            api_version: owner.api_version.clone(),
            kind: owner.kind.clone(),
            name: owner.name.clone(),
            uid: owner.uid.clone(),
            controller: owner.controller == Some(true),
        })
        .collect()
}

fn controller_owner(metadata: &ObjectMeta) -> Option<ResourceOwner> {
    resource_owners(metadata)
        .into_iter()
        .find(|owner| owner.controller)
}

/// Extract a MinimalResource from a DynamicObject
fn extract_minimal_resource(
    obj: &DynamicObject,
    custom_columns: &[CustomResourceColumn],
) -> MinimalResource {
    let metadata = &obj.metadata;
    let uid = get_resource_uid(obj);

    // Parse creation timestamp
    let creation_timestamp = metadata.creation_timestamp.as_ref().and_then(|ts| {
        OffsetDateTime::parse(
            &ts.0.to_string(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    });

    MinimalResource {
        uid,
        name: metadata.name.clone().unwrap_or_default(),
        namespace: metadata.namespace.clone(),
        creation_timestamp,
        controller_owner: controller_owner(metadata),
        labels: metadata.labels.clone().unwrap_or_default(),
        annotations: metadata.annotations.clone().unwrap_or_default(),
        cells: extract_custom_cells(&obj.data, custom_columns),
        log_containers: Vec::new(),
    }
    .with_lifecycle_metadata(
        metadata.deletion_timestamp.is_some(),
        metadata.finalizers.clone().unwrap_or_default(),
    )
}

fn extract_custom_cells(
    data: &k8s_openapi::serde_json::Value,
    columns: &[CustomResourceColumn],
) -> BTreeMap<String, CellValue> {
    use jsonpath_rust::JsonPath;

    columns
        .iter()
        .filter_map(|column| {
            let path = JsonPath::try_from(column.json_path.as_str()).ok()?;
            let value = path.find(data);
            let values = value.as_array()?.to_vec();
            custom_cell_value(column, &values).map(|cell| (column.id.clone(), cell))
        })
        .collect()
}

fn custom_cell_value(
    column: &CustomResourceColumn,
    values: &[k8s_openapi::serde_json::Value],
) -> Option<CellValue> {
    let value = values.first()?;
    if values.len() == 1 {
        if matches!(column.type_.as_str(), "integer" | "number")
            && let Some(number) = value.as_i64()
        {
            return Some(CellValue::Number(number));
        }
        if matches!(column.type_.as_str(), "date" | "date-time")
            && let Some(value) = value.as_str().and_then(parse_timestamp)
        {
            return Some(CellValue::Timestamp(value));
        }
        return json_value_to_text(value).map(CellValue::Text);
    }

    let values = values.iter().filter_map(json_value_to_text).collect();
    Some(CellValue::List(values))
}

fn parse_timestamp(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
}

fn json_value_to_text(value: &k8s_openapi::serde_json::Value) -> Option<String> {
    match value {
        k8s_openapi::serde_json::Value::Null => None,
        k8s_openapi::serde_json::Value::String(value) => Some(value.clone()),
        k8s_openapi::serde_json::Value::Bool(value) => Some(value.to_string()),
        k8s_openapi::serde_json::Value::Number(value) => Some(value.to_string()),
        other => Some(other.to_string()),
    }
}

pub struct ResourceDetailWatchRequest {
    pub cluster_key: i32,
    pub client: kube::Client,
    pub api_resource: ApiResource,
    pub namespace: Option<String>,
    pub resource_name: String,
    pub resource_uid: String,
    pub history_entry_id: u64,
    pub pod_metrics_api_available: bool,
    pub node_metrics_api_available: bool,
    pub event_sender: WorkerResultSender,
}

/// Keep one inspector history entry current independently of the compact
/// resource-table watcher. The worker owns it until that entry leaves history.
pub async fn watch_resource_detail(request: ResourceDetailWatchRequest) {
    let ResourceDetailWatchRequest {
        cluster_key,
        client,
        api_resource,
        namespace,
        resource_name,
        resource_uid,
        history_entry_id,
        pod_metrics_api_available,
        node_metrics_api_available,
        event_sender,
    } = request;
    let root_name = resource_name.clone();
    let metrics_api_resource = api_resource.clone();
    tokio::join!(
        watch_detail_object(
            cluster_key,
            client.clone(),
            api_resource.clone(),
            namespace.clone(),
            resource_name,
            history_entry_id,
            event_sender.clone(),
        ),
        watch_detail_events(
            cluster_key,
            client.clone(),
            namespace.clone(),
            resource_uid.clone(),
            history_entry_id,
            event_sender.clone(),
        ),
        watch_managed_resources(ManagedResourceWatchRequest {
            cluster_key,
            client: client.clone(),
            root_api_resource: api_resource.clone(),
            namespace: namespace.clone(),
            root_name: root_name.clone(),
            root_uid: resource_uid,
            history_entry_id,
            event_sender: event_sender.clone(),
        }),
        watch_pod_detail_metrics(PodDetailMetricsWatchRequest {
            cluster_key,
            client: client.clone(),
            api_resource: metrics_api_resource,
            namespace: namespace.clone(),
            resource_name: root_name.clone(),
            history_entry_id,
            pod_metrics_api_available,
            event_sender: event_sender.clone(),
        }),
        watch_node_detail_metrics(NodeDetailMetricsWatchRequest {
            cluster_key,
            client,
            api_resource,
            resource_name: root_name,
            history_entry_id,
            node_metrics_api_available,
            event_sender,
        }),
    );
}

/// Poll a visible namespace rather than watching the Metrics API: metrics-server publishes
/// sampled values and is not a source of Kubernetes object lifecycle events.
pub async fn watch_pod_metrics_namespace(
    cluster_key: i32,
    client: kube::Client,
    namespace: String,
    event_sender: WorkerResultSender,
) {
    let mut interval = tokio::time::interval(POD_METRICS_POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match list_pod_metrics(&client, &namespace).await {
            Ok(usages) => event_sender
                .send(PodMetricsUpdated {
                    cluster_key,
                    namespace: namespace.clone(),
                    usages,
                })
                .await
                .log_if_error("Failed to send Pod metrics update"),
            Err(error) if is_metrics_api_not_found(&error) => {
                report_metrics_api_unavailable(&event_sender, cluster_key).await;
                return;
            }
            Err(error) => event_sender
                .send(PodMetricsWatchFailed {
                    cluster_key,
                    namespace: namespace.clone(),
                    error: format!("{error:#?}"),
                })
                .await
                .log_if_error("Failed to send Pod metrics error"),
        }
    }
}

/// Nodes are cluster-scoped, so one poller supplies the visible Nodes workspace.
pub async fn watch_node_metrics(
    cluster_key: i32,
    client: kube::Client,
    event_sender: WorkerResultSender,
) {
    let mut interval = tokio::time::interval(POD_METRICS_POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match list_node_metrics(&client).await {
            Ok(usages) => event_sender
                .send(NodeMetricsUpdated {
                    cluster_key,
                    usages,
                })
                .await
                .log_if_error("Failed to send Node metrics update"),
            Err(error) if is_metrics_api_not_found(&error) => {
                report_node_metrics_api_unavailable(&event_sender, cluster_key).await;
                return;
            }
            Err(error) => event_sender
                .send(NodeMetricsWatchFailed {
                    cluster_key,
                    error: format!("{error:#?}"),
                })
                .await
                .log_if_error("Failed to send Node metrics error"),
        }
    }
}

struct PodDetailMetricsWatchRequest {
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    history_entry_id: u64,
    pod_metrics_api_available: bool,
    event_sender: WorkerResultSender,
}

async fn watch_pod_detail_metrics(request: PodDetailMetricsWatchRequest) {
    let PodDetailMetricsWatchRequest {
        cluster_key,
        client,
        api_resource,
        namespace,
        resource_name,
        history_entry_id,
        pod_metrics_api_available,
        event_sender,
    } = request;
    if api_resource.kind != "Pod" || api_resource.group != "core" || !pod_metrics_api_available {
        return;
    }
    let Some(namespace) = namespace else {
        return;
    };
    let mut interval = tokio::time::interval(POD_METRICS_POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match get_pod_metrics(&client, &namespace, &resource_name).await {
            Ok(Some(usage)) => event_sender
                .send(ResourceDetailPodUsageUpdated {
                    cluster_key,
                    history_entry_id,
                    usage,
                })
                .await
                .log_if_error("Failed to send Pod detail metrics update"),
            Ok(None) => event_sender
                .send(ResourceDetailPodUsageMissing {
                    cluster_key,
                    history_entry_id,
                })
                .await
                .log_if_error("Failed to send missing Pod detail metrics update"),
            Err(error) if is_metrics_api_not_found(&error) => {
                report_metrics_api_unavailable(&event_sender, cluster_key).await;
                return;
            }
            Err(error) => event_sender
                .send(ResourceDetailPodUsageFailed {
                    cluster_key,
                    history_entry_id,
                    error: format!("{error:#?}"),
                })
                .await
                .log_if_error("Failed to send Pod detail metrics error"),
        }
    }
}

struct NodeDetailMetricsWatchRequest {
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    resource_name: String,
    history_entry_id: u64,
    node_metrics_api_available: bool,
    event_sender: WorkerResultSender,
}

async fn watch_node_detail_metrics(request: NodeDetailMetricsWatchRequest) {
    let NodeDetailMetricsWatchRequest {
        cluster_key,
        client,
        api_resource,
        resource_name,
        history_entry_id,
        node_metrics_api_available,
        event_sender,
    } = request;
    if api_resource.kind != "Node" || api_resource.group != "core" || !node_metrics_api_available {
        return;
    }
    let mut interval = tokio::time::interval(POD_METRICS_POLL_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match get_node_metrics(&client, &resource_name).await {
            Ok(Some(usage)) => event_sender
                .send(ResourceDetailNodeUsageUpdated {
                    cluster_key,
                    history_entry_id,
                    usage,
                })
                .await
                .log_if_error("Failed to send Node detail metrics update"),
            Ok(None) => event_sender
                .send(ResourceDetailNodeUsageMissing {
                    cluster_key,
                    history_entry_id,
                })
                .await
                .log_if_error("Failed to send missing Node detail metrics update"),
            Err(error) if is_metrics_api_not_found(&error) => {
                report_node_metrics_api_unavailable(&event_sender, cluster_key).await;
                return;
            }
            Err(error) => event_sender
                .send(ResourceDetailNodeUsageFailed {
                    cluster_key,
                    history_entry_id,
                    error: format!("{error:#?}"),
                })
                .await
                .log_if_error("Failed to send Node detail metrics error"),
        }
    }
}

fn is_metrics_api_not_found(error: &anyhow::Error) -> bool {
    error.downcast_ref::<kube::Error>().is_some_and(|error| {
        matches!(error, kube::Error::Api(response)
            if response.code == 404
                && response.message.contains("the server could not find the requested resource"))
    })
}

async fn report_metrics_api_unavailable(event_sender: &WorkerResultSender, cluster_key: i32) {
    event_sender
        .send(PodMetricsApiUnavailable { cluster_key })
        .await
        .log_if_error("Failed to send Metrics API unavailable");
}

async fn report_node_metrics_api_unavailable(event_sender: &WorkerResultSender, cluster_key: i32) {
    event_sender
        .send(NodeMetricsApiUnavailable { cluster_key })
        .await
        .log_if_error("Failed to send Node Metrics API unavailable");
}

fn metrics_pod_api(client: &kube::Client, namespace: &str) -> Api<DynamicObject> {
    let gvk = GroupVersionKind::gvk("metrics.k8s.io", "v1beta1", "PodMetrics");
    let resource = kube::core::ApiResource::from_gvk_with_plural(&gvk, "pods");
    Api::namespaced_with(client.clone(), namespace, &resource)
}

fn metrics_node_api(client: &kube::Client) -> Api<DynamicObject> {
    let gvk = GroupVersionKind::gvk("metrics.k8s.io", "v1beta1", "NodeMetrics");
    let resource = kube::core::ApiResource::from_gvk_with_plural(&gvk, "nodes");
    Api::all_with(client.clone(), &resource)
}

async fn list_pod_metrics(
    client: &kube::Client,
    namespace: &str,
) -> Result<BTreeMap<String, PodUsage>> {
    let metrics = metrics_pod_api(client, namespace)
        .list(&ListParams::default())
        .await?;
    metrics
        .items
        .into_iter()
        .map(|metric| pod_usage_from_value(k8s_openapi::serde_json::to_value(metric)?))
        .collect()
}

async fn get_pod_metrics(
    client: &kube::Client,
    namespace: &str,
    name: &str,
) -> Result<Option<PodUsage>> {
    let metrics = match metrics_pod_api(client, namespace).get(name).await {
        Ok(metrics) => metrics,
        Err(kube::Error::Api(response)) if is_pod_metric_sample_missing(&response, name) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let (_, usage) = pod_usage_from_value(k8s_openapi::serde_json::to_value(metrics)?)?;
    Ok(Some(usage))
}

async fn list_node_metrics(client: &kube::Client) -> Result<BTreeMap<String, NodeUsage>> {
    let metrics = metrics_node_api(client)
        .list(&ListParams::default())
        .await?;
    metrics
        .items
        .into_iter()
        .map(|metric| node_usage_from_value(k8s_openapi::serde_json::to_value(metric)?))
        .collect()
}

async fn get_node_metrics(client: &kube::Client, name: &str) -> Result<Option<NodeUsage>> {
    let metrics = match metrics_node_api(client).get(name).await {
        Ok(metrics) => metrics,
        Err(kube::Error::Api(response)) if is_node_metric_sample_missing(&response, name) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let (_, usage) = node_usage_from_value(k8s_openapi::serde_json::to_value(metrics)?)?;
    Ok(Some(usage))
}

fn is_pod_metric_sample_missing(response: &kube::core::Status, name: &str) -> bool {
    response.code == 404
        && response
            .details
            .as_ref()
            .is_some_and(|details| details.group == "metrics.k8s.io" && details.name == name)
}

fn is_node_metric_sample_missing(response: &kube::core::Status, name: &str) -> bool {
    response.code == 404
        && response
            .details
            .as_ref()
            .is_some_and(|details| details.group == "metrics.k8s.io" && details.name == name)
}

/// Watch the small, well-known set of resource kinds which can make up a
/// built-in workload controller hierarchy. Kubernetes has no generic reverse
/// owner-reference query, so this deliberately does not attempt custom types.
struct ManagedResourceWatchRequest {
    cluster_key: i32,
    client: kube::Client,
    root_api_resource: ApiResource,
    namespace: Option<String>,
    root_name: String,
    root_uid: String,
    history_entry_id: u64,
    event_sender: WorkerResultSender,
}

async fn watch_managed_resources(request: ManagedResourceWatchRequest) {
    let ManagedResourceWatchRequest {
        cluster_key,
        client,
        root_api_resource,
        namespace,
        root_name,
        root_uid,
        history_entry_id,
        event_sender,
    } = request;
    let resource_types = managed_resource_types(&root_api_resource);
    if resource_types.is_empty() {
        event_sender
            .send(ManagedResourcesReplaced {
                cluster_key,
                history_entry_id,
                resources: Vec::new(),
            })
            .await
            .log_if_error("Failed to send empty managed resources");
        return;
    }

    let (updates_sender, mut updates_receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut tasks = JoinSet::new();
    for resource_type in resource_types {
        let client = client.clone();
        let namespace = namespace.clone();
        let root_name = root_name.clone();
        let updates_sender = updates_sender.clone();
        tasks.spawn(async move {
            match resource_type {
                ManagedResourceType::ReplicaSet => {
                    if let Some(namespace) = namespace {
                        watch_managed_type::<ReplicaSet>(
                            client,
                            namespace,
                            updates_sender,
                            resource_handlers::replica_set::extract,
                        )
                        .await
                    }
                }
                ManagedResourceType::Job => {
                    if let Some(namespace) = namespace {
                        watch_managed_type::<Job>(
                            client,
                            namespace,
                            updates_sender,
                            resource_handlers::job::extract,
                        )
                        .await
                    }
                }
                ManagedResourceType::Pod => {
                    if let Some(namespace) = namespace {
                        watch_managed_type::<Pod>(
                            client,
                            namespace,
                            updates_sender,
                            resource_handlers::pod::extract,
                        )
                        .await
                    }
                }
                ManagedResourceType::PodOnNode => {
                    watch_pods_on_node(client, root_name, updates_sender).await
                }
            }
        });
    }
    drop(updates_sender);

    let mut by_type = BTreeMap::<ApiResource, Vec<ManagedResource>>::new();
    while let Some(update) = updates_receiver.recv().await {
        match update {
            ManagedResourceUpdate::Replaced {
                api_resource,
                resources,
            } => {
                by_type.insert(api_resource.clone(), resources);
                let resources = by_type
                    .values()
                    .flatten()
                    .filter(|resource| {
                        if root_api_resource.kind == "Node" {
                            belongs_to_node(resource, &root_name)
                        } else {
                            belongs_to_workload_tree(
                                resource,
                                &root_uid,
                                &root_api_resource,
                                &by_type,
                            )
                        }
                    })
                    .cloned()
                    .collect();
                event_sender
                    .send(ManagedResourcesReplaced {
                        cluster_key,
                        history_entry_id,
                        resources,
                    })
                    .await
                    .log_if_error("Failed to send managed resource update");
            }
            ManagedResourceUpdate::Failed {
                api_resource,
                error,
            } => event_sender
                .send(ManagedResourcesWatchFailed {
                    cluster_key,
                    history_entry_id,
                    error: format!("Unable to watch {}: {error}", api_resource.display_name()),
                })
                .await
                .log_if_error("Failed to send managed resource watch failure"),
        }
    }
    while tasks.join_next().await.is_some() {}
}

#[derive(Clone, Copy)]
enum ManagedResourceType {
    ReplicaSet,
    Job,
    Pod,
    PodOnNode,
}

fn managed_resource_types(api_resource: &ApiResource) -> Vec<ManagedResourceType> {
    match (api_resource.group.as_str(), api_resource.kind.as_str()) {
        ("apps", "Deployment") => vec![ManagedResourceType::ReplicaSet, ManagedResourceType::Pod],
        ("batch", "CronJob") => vec![ManagedResourceType::Job, ManagedResourceType::Pod],
        ("apps", "ReplicaSet")
        | ("apps", "StatefulSet")
        | ("apps", "DaemonSet")
        | ("core", "ReplicationController")
        | ("batch", "Job") => vec![ManagedResourceType::Pod],
        ("core", "Node") => vec![ManagedResourceType::PodOnNode],
        _ => Vec::new(),
    }
}

enum ManagedResourceUpdate {
    Replaced {
        api_resource: ApiResource,
        resources: Vec<ManagedResource>,
    },
    Failed {
        api_resource: ApiResource,
        error: String,
    },
}

async fn watch_managed_type<T>(
    client: kube::Client,
    namespace: String,
    sender: tokio::sync::mpsc::UnboundedSender<ManagedResourceUpdate>,
    extract: fn(&T) -> MinimalResource,
) where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + Clone
        + k8s_openapi::serde::de::DeserializeOwned
        + std::fmt::Debug
        + Send
        + 'static,
{
    let api_resource = api_resource_for::<T>();
    let api = Api::<T>::namespaced(client, &namespace);
    let stream = watcher(api, watcher_config());
    pin_mut!(stream);
    let mut resources = BTreeMap::new();
    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                let _ = sender.send(ManagedResourceUpdate::Failed {
                    api_resource,
                    error: format!("{error:#?}"),
                });
                return;
            }
        };
        match event {
            Event::Init => resources.clear(),
            Event::InitApply(resource) | Event::Apply(resource) => {
                if let Some(resource) = managed_resource_from_typed(&resource, extract) {
                    resources.insert(resource.uid.clone(), resource);
                }
            }
            Event::Delete(resource) => {
                resources.remove(&get_resource_uid(&resource));
            }
            Event::InitDone => {}
        }
        let _ = sender.send(ManagedResourceUpdate::Replaced {
            api_resource: api_resource.clone(),
            resources: resources.values().cloned().collect(),
        });
    }
}

async fn watch_pods_on_node(
    client: kube::Client,
    node_name: String,
    sender: tokio::sync::mpsc::UnboundedSender<ManagedResourceUpdate>,
) {
    let api_resource = api_resource_for::<Pod>();
    let api = Api::<Pod>::all(client);
    let config = watcher_config().fields(&format!("spec.nodeName={node_name}"));
    let stream = watcher(api, config);
    pin_mut!(stream);
    let mut resources = BTreeMap::new();
    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                let _ = sender.send(ManagedResourceUpdate::Failed {
                    api_resource,
                    error: format!("{error:#?}"),
                });
                return;
            }
        };
        match event {
            Event::Init => resources.clear(),
            Event::InitApply(resource) | Event::Apply(resource) => {
                if let Some(resource) = scheduled_pod_from_typed(&resource) {
                    resources.insert(resource.uid.clone(), resource);
                }
            }
            Event::Delete(resource) => {
                resources.remove(&get_resource_uid(&resource));
            }
            Event::InitDone => {}
        }
        let _ = sender.send(ManagedResourceUpdate::Replaced {
            api_resource: api_resource.clone(),
            resources: resources.values().cloned().collect(),
        });
    }
}

pub(crate) trait ApiResourceScope {
    const NAMESPACED: bool;
}

impl ApiResourceScope for NamespaceResourceScope {
    const NAMESPACED: bool = true;
}

impl ApiResourceScope for ClusterResourceScope {
    const NAMESPACED: bool = false;
}

pub(crate) fn api_resource_for<T>() -> ApiResource
where
    T: Resource<DynamicType = ()>,
    T::Scope: ApiResourceScope,
{
    let group = T::group(&());
    ApiResource {
        group: if group.is_empty() {
            "core".into()
        } else {
            group.into_owned()
        },
        version: T::version(&()).into_owned(),
        kind: T::kind(&()).into_owned(),
        name: T::plural(&()).into_owned(),
        namespaced: T::Scope::NAMESPACED,
    }
}

fn managed_resource_from_typed<T>(
    resource: &T,
    extract: impl FnOnce(&T) -> MinimalResource,
) -> Option<ManagedResource>
where
    T: Resource<DynamicType = (), Scope = NamespaceResourceScope>,
{
    let metadata = resource.meta();
    let controller_owner_uid = metadata
        .owner_references
        .as_ref()?
        .iter()
        .find(|owner| owner.controller == Some(true))?
        .uid
        .clone();
    let minimal_resource = extract(resource);
    Some(ManagedResource {
        api_resource: api_resource_for::<T>(),
        name: minimal_resource.name,
        namespace: minimal_resource.namespace,
        uid: minimal_resource.uid,
        association: ManagedResourceAssociation::ControllerOwnerUid(controller_owner_uid),
        creation_timestamp: minimal_resource.creation_timestamp,
        cells: minimal_resource.cells,
    })
}

fn scheduled_pod_from_typed(resource: &Pod) -> Option<ManagedResource> {
    let node_name = resource.spec.as_ref()?.node_name.clone()?;
    let minimal_resource = resource_handlers::pod::extract(resource);
    Some(ManagedResource {
        api_resource: api_resource_for::<Pod>(),
        name: minimal_resource.name,
        namespace: minimal_resource.namespace,
        uid: minimal_resource.uid,
        association: ManagedResourceAssociation::NodeName(node_name),
        creation_timestamp: minimal_resource.creation_timestamp,
        cells: minimal_resource.cells,
    })
}

fn belongs_to_workload_tree(
    resource: &ManagedResource,
    root_uid: &str,
    root_api_resource: &ApiResource,
    all_resources: &BTreeMap<ApiResource, Vec<ManagedResource>>,
) -> bool {
    if matches!(
        &resource.association,
        ManagedResourceAssociation::ControllerOwnerUid(owner_uid) if owner_uid == root_uid
    ) {
        return is_managed_workload_child(root_api_resource, &resource.api_resource);
    }
    all_resources.values().flatten().any(|parent| {
        matches!(
            &resource.association,
            ManagedResourceAssociation::ControllerOwnerUid(owner_uid) if parent.uid == *owner_uid
        ) && is_managed_workload_child(&parent.api_resource, &resource.api_resource)
            && belongs_to_workload_tree(parent, root_uid, root_api_resource, all_resources)
    })
}

fn belongs_to_node(resource: &ManagedResource, node_name: &str) -> bool {
    matches!(
        &resource.association,
        ManagedResourceAssociation::NodeName(assigned_node) if assigned_node == node_name
    )
}

fn is_managed_workload_child(parent: &ApiResource, child: &ApiResource) -> bool {
    matches!(
        (
            parent.group.as_str(),
            parent.kind.as_str(),
            child.group.as_str(),
            child.kind.as_str(),
        ),
        ("apps", "Deployment", "apps", "ReplicaSet")
            | ("batch", "CronJob", "batch", "Job")
            | ("apps", "ReplicaSet", "core", "Pod")
            | ("apps", "StatefulSet", "core", "Pod")
            | ("apps", "DaemonSet", "core", "Pod")
            | ("core", "ReplicationController", "core", "Pod")
            | ("batch", "Job", "core", "Pod")
    )
}

async fn watch_detail_object(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    history_entry_id: u64,
    event_sender: WorkerResultSender,
) {
    let api = match dynamic_api::create(&client, &api_resource, namespace.as_deref()).await {
        Ok(api) => api,
        Err(error) => {
            send_detail_error(&event_sender, cluster_key, history_entry_id, false, error).await;
            return;
        }
    };
    let config = watcher_config().fields(&format!("metadata.name={resource_name}"));
    let stream = watcher(api, config);
    pin_mut!(stream);
    let mut found_during_initial_list = false;
    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                send_detail_error(&event_sender, cluster_key, history_entry_id, false, error).await;
                return;
            }
        };
        match event {
            Event::Apply(object) => {
                event_sender
                    .send(ResourceDetailUpdated {
                        cluster_key,
                        history_entry_id,
                        detail: Box::new(
                            resource_detail_from_dynamic(&client, api_resource.clone(), object)
                                .await,
                        ),
                    })
                    .await
                    .log_if_error("Failed to send resource detail update");
            }
            Event::InitApply(object) => {
                found_during_initial_list = true;
                event_sender
                    .send(ResourceDetailUpdated {
                        cluster_key,
                        history_entry_id,
                        detail: Box::new(
                            resource_detail_from_dynamic(&client, api_resource.clone(), object)
                                .await,
                        ),
                    })
                    .await
                    .log_if_error("Failed to send resource detail update");
            }
            Event::Delete(_) => event_sender
                .send(ResourceDetailDeleted {
                    cluster_key,
                    history_entry_id,
                })
                .await
                .log_if_error("Failed to send resource detail deletion"),
            Event::Init => found_during_initial_list = false,
            Event::InitDone if !found_during_initial_list => event_sender
                .send(ResourceDetailDeleted {
                    cluster_key,
                    history_entry_id,
                })
                .await
                .log_if_error("Failed to send missing resource detail deletion"),
            Event::InitDone => {}
        }
    }
}

async fn watch_detail_events(
    cluster_key: i32,
    client: kube::Client,
    namespace: Option<String>,
    resource_uid: String,
    history_entry_id: u64,
    event_sender: WorkerResultSender,
) {
    let api: Api<KubernetesEvent> = match namespace.as_deref() {
        Some(namespace) => Api::namespaced(client, namespace),
        None => Api::all(client),
    };
    let config = watcher_config().fields(&format!("involvedObject.uid={resource_uid}"));
    let stream = watcher(api, config);
    pin_mut!(stream);
    let mut events = BTreeMap::new();
    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                send_detail_error(&event_sender, cluster_key, history_entry_id, true, error).await;
                return;
            }
        };
        match event {
            Event::Init => events.clear(),
            Event::InitApply(event) | Event::Apply(event) => {
                events.insert(
                    get_resource_uid(&event),
                    resource_event_from_kubernetes(event),
                );
            }
            Event::Delete(event) => {
                events.remove(&get_resource_uid(&event));
            }
            Event::InitDone => {}
        }
        send_detail_events(&event_sender, cluster_key, history_entry_id, &events).await;
    }
}

async fn send_detail_events(
    event_sender: &WorkerResultSender,
    cluster_key: i32,
    history_entry_id: u64,
    events: &BTreeMap<String, ResourceEvent>,
) {
    let mut events = events.values().cloned().collect::<Vec<_>>();
    events.sort_by_key(|event| std::cmp::Reverse(event.last_timestamp));
    event_sender
        .send(ResourceEventsReplaced {
            cluster_key,
            history_entry_id,
            events,
        })
        .await
        .log_if_error("Failed to send resource event update");
}

async fn send_detail_error(
    event_sender: &WorkerResultSender,
    cluster_key: i32,
    history_entry_id: u64,
    events: bool,
    error: impl std::fmt::Debug,
) {
    event_sender
        .send(ResourceDetailWatchFailed {
            cluster_key,
            history_entry_id,
            events,
            error: format!("{error:#?}"),
        })
        .await
        .log_if_error("Failed to send resource detail watch failure");
}

async fn resource_detail_from_dynamic(
    client: &kube::Client,
    api_resource: ApiResource,
    object: DynamicObject,
) -> ResourceDetail {
    let metadata = &object.metadata;
    let creation_timestamp = metadata.creation_timestamp.as_ref().and_then(|timestamp| {
        OffsetDateTime::parse(
            &timestamp.0.to_string(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    });
    let mut detail = ResourceDetail {
        api_resource: api_resource.clone(),
        name: metadata.name.clone().unwrap_or_default(),
        namespace: metadata.namespace.clone(),
        uid: get_resource_uid(&object),
        resource_version: metadata.resource_version.clone().unwrap_or_default(),
        is_deleting: metadata.deletion_timestamp.is_some(),
        finalizers: metadata.finalizers.clone().unwrap_or_default(),
        creation_timestamp,
        owners: resource_owners(metadata),
        labels: metadata.labels.clone().unwrap_or_default(),
        annotations: metadata.annotations.clone().unwrap_or_default(),
        payload: resource_handlers::detail_payload(&api_resource, &object),
    };
    if let (Some(namespace), ResourceDetailPayload::Pod(pod)) =
        (detail.namespace.as_deref(), &mut detail.payload)
    {
        resolve_pod_environment_variables(client, namespace, pod).await;
    }
    detail
}

async fn resolve_pod_environment_variables(
    client: &kube::Client,
    namespace: &str,
    pod: &mut crate::resource_detail::PodDetail,
) {
    let mut config_map_names = BTreeSet::new();
    let mut secret_names = BTreeSet::new();
    for container in &pod.containers {
        for variable in &container.environment_variables {
            match &variable.source {
                PodEnvironmentVariableSource::ConfigMapKey { name, .. }
                | PodEnvironmentVariableSource::ConfigMapImport { name, .. } => {
                    config_map_names.insert(name.clone());
                }
                PodEnvironmentVariableSource::SecretKey { name, .. }
                | PodEnvironmentVariableSource::SecretImport { name, .. } => {
                    secret_names.insert(name.clone());
                }
                _ => {}
            }
        }
    }

    let config_maps = fetch_config_maps(client, namespace, config_map_names).await;
    let secrets = fetch_secrets(client, namespace, secret_names).await;
    for container in &mut pod.containers {
        let variables = std::mem::take(&mut container.environment_variables);
        let mut variables = variables
            .into_iter()
            .flat_map(|variable| resolve_environment_variable(variable, &config_maps, &secrets))
            .collect::<Vec<_>>();
        expand_environment_variable_references(&mut variables);
        container.environment_variables = variables;
    }
}

async fn fetch_config_maps(
    client: &kube::Client,
    namespace: &str,
    names: BTreeSet<String>,
) -> BTreeMap<String, ConfigMap> {
    let api = Api::<ConfigMap>::namespaced(client.clone(), namespace);
    let mut config_maps = BTreeMap::new();
    for name in names {
        if let Ok(Some(config_map)) = api.get_opt(&name).await {
            config_maps.insert(name, config_map);
        }
    }
    config_maps
}

async fn fetch_secrets(
    client: &kube::Client,
    namespace: &str,
    names: BTreeSet<String>,
) -> BTreeMap<String, Secret> {
    let api = Api::<Secret>::namespaced(client.clone(), namespace);
    let mut secrets = BTreeMap::new();
    for name in names {
        if let Ok(Some(secret)) = api.get_opt(&name).await {
            secrets.insert(name, secret);
        }
    }
    secrets
}

fn resolve_environment_variable(
    mut variable: PodEnvironmentVariableDetail,
    config_maps: &BTreeMap<String, ConfigMap>,
    secrets: &BTreeMap<String, Secret>,
) -> Vec<PodEnvironmentVariableDetail> {
    match &variable.source {
        PodEnvironmentVariableSource::ConfigMapKey { name, key, .. } => {
            variable.value = config_map_value(config_maps.get(name), key);
            vec![variable]
        }
        PodEnvironmentVariableSource::SecretKey { name, key, .. } => {
            variable.value = secret_value(secrets.get(name), key);
            vec![variable]
        }
        PodEnvironmentVariableSource::ConfigMapImport {
            name,
            prefix,
            optional,
        } => {
            let Some(config_map) = config_maps.get(name) else {
                return vec![variable];
            };
            config_map
                .data
                .as_ref()
                .into_iter()
                .flatten()
                .map(|(key, value)| PodEnvironmentVariableDetail {
                    name: format!("{prefix}{key}"),
                    value: Some(value.clone()),
                    source: PodEnvironmentVariableSource::ConfigMapKey {
                        name: name.clone(),
                        key: key.clone(),
                        optional: *optional,
                    },
                })
                .collect()
        }
        PodEnvironmentVariableSource::SecretImport {
            name,
            prefix,
            optional,
        } => {
            let Some(secret) = secrets.get(name) else {
                return vec![variable];
            };
            secret
                .data
                .as_ref()
                .into_iter()
                .flatten()
                .map(|(key, value)| PodEnvironmentVariableDetail {
                    name: format!("{prefix}{key}"),
                    value: Some(String::from_utf8_lossy(&value.0).into_owned()),
                    source: PodEnvironmentVariableSource::SecretKey {
                        name: name.clone(),
                        key: key.clone(),
                        optional: *optional,
                    },
                })
                .collect()
        }
        _ => vec![variable],
    }
}

fn config_map_value(config_map: Option<&ConfigMap>, key: &str) -> Option<String> {
    config_map
        .and_then(|config_map| config_map.data.as_ref())
        .and_then(|data| data.get(key))
        .cloned()
}

fn secret_value(secret: Option<&Secret>, key: &str) -> Option<String> {
    secret
        .and_then(|secret| secret.data.as_ref())
        .and_then(|data| data.get(key))
        .map(|value| String::from_utf8_lossy(&value.0).into_owned())
}

fn expand_environment_variable_references(variables: &mut [PodEnvironmentVariableDetail]) {
    let mut values = BTreeMap::new();
    for variable in variables {
        if matches!(variable.source, PodEnvironmentVariableSource::Literal)
            && let Some(value) = &variable.value
        {
            variable.value = Some(expand_environment_variable_value(value, &values));
        }
        if let Some(value) = &variable.value {
            values.insert(variable.name.clone(), value.clone());
        }
    }
}

fn expand_environment_variable_value(value: &str, values: &BTreeMap<String, String>) -> String {
    let mut result = String::new();
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '$' {
            result.push(character);
            continue;
        }
        if characters.next_if_eq(&'$').is_some() {
            result.push('$');
            continue;
        }
        if characters.next_if_eq(&'(').is_none() {
            result.push('$');
            continue;
        }
        let mut name = String::new();
        for character in characters.by_ref() {
            if character == ')' {
                break;
            }
            name.push(character);
        }
        if let Some(replacement) = values.get(&name) {
            result.push_str(replacement);
        } else {
            result.push_str("$(");
            result.push_str(&name);
            result.push(')');
        }
    }
    result
}

fn resource_event_from_kubernetes(event: KubernetesEvent) -> ResourceEvent {
    let last_timestamp = if let Some(timestamp) = event.event_time.as_ref() {
        OffsetDateTime::parse(
            &timestamp.0.to_string(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    } else {
        event.last_timestamp.as_ref().and_then(|timestamp| {
            OffsetDateTime::parse(
                &timestamp.0.to_string(),
                &time::format_description::well_known::Rfc3339,
            )
            .ok()
        })
    };
    ResourceEvent {
        uid: get_resource_uid(&event),
        type_: event.type_.unwrap_or_else(|| "Normal".to_owned()),
        reason: event.reason.unwrap_or_else(|| "Unknown".to_owned()),
        message: event.message.unwrap_or_default(),
        source: event.source.and_then(|source| source.component),
        count: event.count.unwrap_or(1),
        last_timestamp,
    }
}

fn supports_scale_subresource(
    resources: &[k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource],
    resource_name: &str,
) -> bool {
    let scale_name = format!("{resource_name}/scale");
    resources.iter().any(|resource| {
        resource.name == scale_name
            && resource.verbs.iter().any(|verb| verb == "get")
            && resource.verbs.iter().any(|verb| verb == "patch")
    })
}

/// Fetch the desired replica count through a dynamically discovered Scale subresource.
pub async fn get_resource_scale(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
) -> Result<ResourceScaleFetched> {
    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let scale = api.get_scale(&resource_name).await?;
    let replicas = scale
        .spec
        .context("Scale endpoint returned no desired replica count")?
        .replicas
        .context("Scale endpoint returned no desired replica count")?;

    Ok(ResourceScaleFetched {
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        replicas,
    })
}

/// Update the desired replica count through a dynamically discovered Scale subresource.
pub async fn update_resource_scale(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    replicas: i32,
) -> Result<ResourceScaleUpdated> {
    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let patch: serde_yaml::Value =
        serde_yaml::from_str(&format!("spec:\n  replicas: {replicas}\n"))?;
    api.patch_scale(
        &resource_name,
        &kube::api::PatchParams::default(),
        &kube::api::Patch::Merge(&patch),
    )
    .await?;

    Ok(ResourceScaleUpdated {
        cluster_key,
        resource_name,
    })
}

/// Fetch a resource's full YAML representation
pub async fn get_resource_yaml(
    editor_id: u64,
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
) -> Result<ResourceYamlFetched> {
    info!(
        "Getting YAML for {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let mut obj = api.get(&resource_name).await?;

    resource_yaml::strip_server_managed_metadata(&mut obj);

    let yaml = serde_yaml::to_string(&obj)?;

    Ok(ResourceYamlFetched {
        editor_id,
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        yaml,
    })
}

/// Fetch the OpenAPI v3 group-version document and return the schema for one built-in resource.
/// CRD schemas are sent with API discovery, so this path is only used as a lazy fallback.
pub async fn get_resource_schema(
    editor_id: u64,
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
) -> Result<ResourceSchemaLoaded> {
    let group_version = if api_resource.group == "core" {
        format!("api/{}", api_resource.version)
    } else {
        format!("apis/{}/{}", api_resource.group, api_resource.version)
    };
    let index: k8s_openapi::serde_json::Value = client
        .request(Request::builder().uri("/openapi/v3").body(Vec::new())?)
        .await?;
    let path = index
        .get("paths")
        .and_then(|paths| {
            paths
                .get(&group_version)
                .or_else(|| paths.get(format!("/{group_version}")))
        })
        .and_then(|entry| entry.get("serverRelativeURL"))
        .and_then(k8s_openapi::serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("No OpenAPI v3 schema is available for {group_version}"))?;
    let document: k8s_openapi::serde_json::Value = client
        .request(Request::builder().uri(path).body(Vec::new())?)
        .await?;
    let schema = ResourceSchema::from_openapi_document(document, &api_resource)
        .ok_or_else(|| anyhow::anyhow!("No OpenAPI schema matches {}", api_resource.kind))?;
    Ok(ResourceSchemaLoaded {
        editor_id,
        cluster_key,
        api_resource,
        schema,
    })
}

pub struct ResourceYamlValidationRequest {
    pub editor_id: u64,
    pub revision: u64,
    pub cluster_key: i32,
    pub client: kube::Client,
    pub api_resource: ApiResource,
    pub namespace: Option<String>,
    pub resource_name: String,
    pub yaml: String,
}

/// Validate the same server-side apply request used by Save without persisting a change.
pub async fn validate_resource_yaml(
    request: ResourceYamlValidationRequest,
) -> Result<Result<ResourceYamlValidated, ResourceYamlValidationFailed>> {
    let ResourceYamlValidationRequest {
        editor_id,
        revision,
        cluster_key,
        client,
        api_resource,
        namespace,
        resource_name,
        yaml,
    } = request;
    let mut obj: DynamicObject = serde_yaml::from_str(&yaml)?;
    resource_yaml::strip_server_managed_metadata(&mut obj);

    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let params = kube::api::PatchParams::apply("kubernetes-dev-ui")
        .force()
        .validation(kube::api::ValidationDirective::Strict)
        .dry_run();
    match api
        .patch(&resource_name, &params, &kube::api::Patch::Apply(&obj))
        .await
    {
        Ok(_) => Ok(Ok(ResourceYamlValidated {
            editor_id,
            revision,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
        })),
        Err(kube::Error::Api(status)) => Ok(Err(ResourceYamlValidationFailed {
            editor_id,
            revision,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            error: resource_api_error(&status),
        })),
        Err(error) => Err(error.into()),
    }
}

fn resource_api_error(status: &kube::core::Status) -> ResourceApiError {
    ResourceApiError {
        message: status.message.clone(),
        causes: status
            .details
            .as_ref()
            .map_or(&[][..], |details| details.causes.as_slice())
            .iter()
            .map(|cause| ResourceApiErrorCause {
                field: cause.field.clone(),
                message: cause.message.clone(),
                reason: cause.reason.clone(),
            })
            .collect(),
    }
}

/// Delete a resource
pub async fn delete_resource(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    resource_uid: Option<String>,
    bulk_delete_id: Option<u64>,
) -> Result<ResourceDeleteCompleted> {
    info!(
        "Deleting {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let delete_params = resource_uid.map(|uid| {
        DeleteParams::default().preconditions(Preconditions {
            uid: Some(uid),
            resource_version: None,
        })
    });
    api.delete(&resource_name, &delete_params.unwrap_or_default())
        .await?;

    Ok(ResourceDeleteCompleted {
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        bulk_delete_id,
    })
}

/// Remove finalizers from a resource Kubernetes is already deleting.
pub async fn force_delete_resource(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    resource_uid: String,
) -> Result<ResourceForceDeleteCompleted> {
    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;
    let resource = api.get(&resource_name).await?;
    let metadata = resource.meta();
    if metadata.uid.as_deref() != Some(&resource_uid) {
        anyhow::bail!(
            "Resource was replaced while awaiting confirmation; finalizers were not removed"
        );
    }
    if metadata.deletion_timestamp.is_none() {
        anyhow::bail!("Resource is no longer deleting; finalizers were not removed");
    }
    if metadata.finalizers.as_ref().is_none_or(Vec::is_empty) {
        anyhow::bail!("Resource no longer has finalizers; nothing was removed");
    }
    let resource_version = metadata
        .resource_version
        .as_deref()
        .context("Deleting resource did not include a resource version")?;
    info!(
        "Removing finalizers from {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );
    let patch = k8s_openapi::serde_json::json!({
        "metadata": { "resourceVersion": resource_version, "finalizers": [] }
    });
    api.patch(
        &resource_name,
        &kube::api::PatchParams::default(),
        &kube::api::Patch::Merge(&patch),
    )
    .await?;
    Ok(ResourceForceDeleteCompleted {
        cluster_key,
        resource_name,
    })
}

/// Trigger a Deployment rollout the same way `kubectl rollout restart` does.
pub async fn restart_deployment(
    client: kube::Client,
    namespace: String,
    resource_name: String,
) -> Result<DeploymentRestartCompleted> {
    let restarted_at = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .context("Formatting Deployment restart timestamp")?;
    info!(
        "Restarting rollout for Deployment {} in {}",
        resource_name, namespace
    );

    let api: Api<Deployment> = Api::namespaced(client, &namespace);
    let patch: serde_yaml::Value = serde_yaml::from_str(&format!(
        "spec:\n  template:\n    metadata:\n      annotations:\n        kubectl.kubernetes.io/restartedAt: \"{restarted_at}\"\n"
    ))?;
    api.patch(
        &resource_name,
        &kube::api::PatchParams::default(),
        &kube::api::Patch::Merge(&patch),
    )
    .await?;

    Ok(DeploymentRestartCompleted {
        namespace,
        resource_name,
    })
}

/// Apply (replace) a resource from YAML
pub async fn apply_resource_yaml(
    editor_id: u64,
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    yaml: String,
) -> Result<Result<ResourceApplyCompleted, ResourceApplyFailed>> {
    info!(
        "Applying YAML for {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    let mut obj: DynamicObject = serde_yaml::from_str(&yaml)?;

    resource_yaml::strip_server_managed_metadata(&mut obj);

    let api = dynamic_api::create(&client, &api_resource, namespace.as_deref()).await?;

    // Use server-side apply with force to take ownership of fields
    let patch_params = kube::api::PatchParams::apply("kubernetes-dev-ui").force();
    match api
        .patch(
            &resource_name,
            &patch_params,
            &kube::api::Patch::Apply(&obj),
        )
        .await
    {
        Ok(_) => Ok(Ok(ResourceApplyCompleted {
            editor_id,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
        })),
        Err(kube::Error::Api(status)) => Ok(Err(ResourceApplyFailed {
            editor_id,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            error: resource_api_error(&status),
        })),
        Err(error) => Err(error.into()),
    }
}

pub struct ResourceDataUpdateRequest<'a> {
    pub cluster_key: i32,
    pub history_entry_id: u64,
    pub request_id: u64,
    pub client: kube::Client,
    pub api_resource: ApiResource,
    pub namespace: String,
    pub resource_name: String,
    pub expected_values: &'a BTreeMap<String, String>,
    pub updated_values: &'a BTreeMap<String, String>,
    pub expected_resource_version: &'a str,
}

/// Replace selected existing data values while preserving every other field. The
/// fetched object's resourceVersion makes a concurrent update fail rather than
/// silently overwriting it.
pub async fn update_resource_data(
    request: ResourceDataUpdateRequest<'_>,
) -> Result<ResourceDataUpdateCompleted> {
    let ResourceDataUpdateRequest {
        cluster_key,
        history_entry_id,
        request_id,
        client,
        api_resource,
        namespace,
        resource_name,
        expected_values,
        updated_values,
        expected_resource_version,
    } = request;
    resource_data::validate_update_request(
        expected_values,
        updated_values,
        expected_resource_version,
    )?;

    if resource_handlers::matches_namespaced_api_resource::<ConfigMap>(&api_resource) {
        let api: Api<ConfigMap> = Api::namespaced(client, &namespace);
        let mut config_map = api.get(&resource_name).await?;
        resource_data::validate_resource_version(
            config_map.metadata.resource_version.as_deref(),
            expected_resource_version,
            "ConfigMap",
        )?;
        let data = config_map
            .data
            .as_mut()
            .context("ConfigMap has no text data to update")?;
        for (key, expected) in expected_values {
            if data.get(key) != Some(expected) {
                bail!("ConfigMap data key '{key}' changed or was removed on the cluster");
            }
        }
        for (key, value) in updated_values {
            *data
                .get_mut(key)
                .expect("expected ConfigMap key was verified above") = value.clone();
        }
        api.replace(&resource_name, &Default::default(), &config_map)
            .await?;
    } else if resource_handlers::matches_namespaced_api_resource::<Secret>(&api_resource) {
        let api: Api<Secret> = Api::namespaced(client, &namespace);
        let mut secret = api.get(&resource_name).await?;
        resource_data::validate_resource_version(
            secret.metadata.resource_version.as_deref(),
            expected_resource_version,
            "Secret",
        )?;
        let data = secret
            .data
            .as_mut()
            .context("Secret has no data to update")?;
        for (key, expected) in expected_values {
            let Some(current) = data.get(key) else {
                bail!("Secret data key '{key}' was removed on the cluster");
            };
            if std::str::from_utf8(&current.0).ok() != Some(expected.as_str()) {
                bail!("Secret data key '{key}' changed on the cluster");
            }
        }
        for (key, value) in updated_values {
            *data
                .get_mut(key)
                .expect("expected Secret key was verified above") =
                k8s_openapi::ByteString(value.as_bytes().to_vec());
        }
        api.replace(&resource_name, &Default::default(), &secret)
            .await?;
    } else {
        bail!("Resource data updates are only supported for ConfigMaps and Secrets");
    }

    Ok(ResourceDataUpdateCompleted {
        cluster_key,
        history_entry_id,
        request_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource_table::{
        AVAILABLE_COLUMN, READY_COLUMN, RESTARTS_COLUMN, STATUS_COLUMN, UP_TO_DATE_COLUMN,
    };
    use k8s_openapi::api::apps::v1::{Deployment, DeploymentStatus};
    use k8s_openapi::api::core::v1::{ContainerStatus, Pod, PodStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{APIResource, ObjectMeta, OwnerReference};

    #[test]
    fn duplicate_helm_revisions_prefer_the_secret_storage_record() {
        let release = |storage| HelmRelease {
            storage,
            storage_name: "record".into(),
            name: "demo".into(),
            namespace: "apps".into(),
            revision: 1,
            status: "deployed".into(),
            description: String::new(),
            notes: String::new(),
            chart: "chart".into(),
            chart_version: "1.0.0".into(),
            app_version: String::new(),
            first_deployed: String::new(),
            last_deployed: String::new(),
            values: Default::default(),
            manifest: String::new(),
            storage_labels: BTreeMap::new(),
            storage_annotations: BTreeMap::new(),
        };
        let records = BTreeMap::from([
            ("configmap/record".into(), release(StorageDriver::ConfigMap)),
            ("secret/record".into(), release(StorageDriver::Secret)),
        ]);

        let merged = merged_helm_releases(&records);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].storage, StorageDriver::Secret);
    }

    #[test]
    fn metrics_availability_requires_the_matching_served_resource() {
        let resources = vec![ApiResource {
            group: "metrics.k8s.io".into(),
            version: "v1beta1".into(),
            kind: "PodMetrics".into(),
            name: "pods".into(),
            namespaced: true,
        }];
        assert!(pod_metrics_api_available(&resources));
        assert!(!node_metrics_api_available(&resources));

        let missing_metrics = vec![ApiResource {
            group: "metrics.k8s.io".into(),
            version: "v1beta1".into(),
            kind: "NodeMetrics".into(),
            name: "nodes".into(),
            namespaced: false,
        }];
        assert!(!pod_metrics_api_available(&missing_metrics));
        assert!(node_metrics_api_available(&missing_metrics));
    }

    #[test]
    fn metrics_api_404_is_terminal() {
        let error = anyhow::Error::new(kube::Error::Api(
            kube::core::Status::failure(
                "the server could not find the requested resource",
                "NotFound",
            )
            .with_code(404)
            .boxed(),
        ));

        assert!(is_metrics_api_not_found(&error));
    }

    #[test]
    fn missing_pod_metric_sample_is_not_an_unavailable_metrics_api() {
        let response = kube::core::Status {
            code: 404,
            details: Some(kube::core::response::StatusDetails {
                group: "metrics.k8s.io".into(),
                name: "api".into(),
                kind: "PodMetrics".into(),
                uid: String::new(),
                causes: Vec::new(),
                retry_after_seconds: 0,
            }),
            ..Default::default()
        };

        assert!(is_pod_metric_sample_missing(&response, "api"));
        assert!(!is_metrics_api_not_found(&anyhow::Error::new(
            kube::Error::Api(response.boxed(),)
        )));
    }

    #[test]
    fn missing_node_metric_sample_is_not_an_unavailable_metrics_api() {
        let response = kube::core::Status {
            code: 404,
            details: Some(kube::core::response::StatusDetails {
                group: "metrics.k8s.io".into(),
                name: "worker-a".into(),
                // Kubernetes status details use the resource name, not the GVK kind.
                kind: "nodes".into(),
                uid: String::new(),
                causes: Vec::new(),
                retry_after_seconds: 0,
            }),
            ..Default::default()
        };

        assert!(is_node_metric_sample_missing(&response, "worker-a"));
        assert!(!is_node_metric_sample_missing(&response, "worker-b"));
        assert!(!is_metrics_api_not_found(&anyhow::Error::new(
            kube::Error::Api(response.boxed(),)
        )));
    }

    #[test]
    fn resource_owners_preserve_all_references_and_identify_the_controller() {
        let metadata = ObjectMeta {
            owner_references: Some(vec![
                OwnerReference {
                    api_version: "example.dev/v1".into(),
                    kind: "Backup".into(),
                    name: "api-backup".into(),
                    uid: "backup-uid".into(),
                    controller: Some(false),
                    block_owner_deletion: None,
                },
                OwnerReference {
                    api_version: "apps/v1".into(),
                    kind: "ReplicaSet".into(),
                    name: "api-7b948f".into(),
                    uid: "replicaset-uid".into(),
                    controller: Some(true),
                    block_owner_deletion: None,
                },
            ]),
            ..Default::default()
        };

        let owners = resource_owners(&metadata);
        let dynamic_resource = extract_minimal_resource(
            &DynamicObject {
                types: None,
                metadata: metadata.clone(),
                data: k8s_openapi::serde_json::json!({}),
            },
            &[],
        );
        let typed_resource = crate::minimal_resource::from_kubernetes_resource(
            &Pod {
                metadata: metadata.clone(),
                ..Default::default()
            },
            BTreeMap::new(),
        );

        assert_eq!(owners.len(), 2);
        assert_eq!(owners[0].label(), "Backup / api-backup");
        assert_eq!(owners[1].uid, "replicaset-uid");
        assert_eq!(
            controller_owner(&metadata).map(|owner| owner.name),
            Some("api-7b948f".into())
        );
        assert_eq!(
            dynamic_resource.controller_owner.map(|owner| owner.name),
            Some("api-7b948f".into())
        );
        assert_eq!(
            typed_resource.controller_owner.map(|owner| owner.name),
            Some("api-7b948f".into())
        );
        assert_eq!(
            owners
                .into_iter()
                .find(|owner| owner.controller)
                .map(|owner| owner.name),
            Some("api-7b948f".into())
        );
    }

    #[test]
    fn scale_capability_requires_get_and_patch_on_the_parent_subresource() {
        let resources = vec![
            APIResource {
                name: "deployments/scale".into(),
                verbs: vec!["get".into(), "patch".into()],
                ..Default::default()
            },
            APIResource {
                name: "deployments/status".into(),
                verbs: vec!["get".into(), "patch".into()],
                ..Default::default()
            },
            APIResource {
                name: "statefulsets/scale".into(),
                verbs: vec!["get".into()],
                ..Default::default()
            },
        ];

        assert!(supports_scale_subresource(&resources, "deployments"));
        assert!(!supports_scale_subresource(&resources, "statefulsets"));
        assert!(!supports_scale_subresource(&resources, "replicasets"));
    }

    #[test]
    fn environment_variable_expansion_uses_earlier_values_and_preserves_unknown_references() {
        let mut variables = vec![
            PodEnvironmentVariableDetail {
                name: "HOST".to_owned(),
                value: Some("api".to_owned()),
                source: PodEnvironmentVariableSource::Literal,
            },
            PodEnvironmentVariableDetail {
                name: "URL".to_owned(),
                value: Some("https://$(HOST)/$(SERVICE_PORT)".to_owned()),
                source: PodEnvironmentVariableSource::Literal,
            },
            PodEnvironmentVariableDetail {
                name: "ESCAPED".to_owned(),
                value: Some("$$(HOST)".to_owned()),
                source: PodEnvironmentVariableSource::Literal,
            },
            PodEnvironmentVariableDetail {
                name: "SHELL_STYLE".to_owned(),
                value: Some("$HOST".to_owned()),
                source: PodEnvironmentVariableSource::Literal,
            },
        ];

        expand_environment_variable_references(&mut variables);

        assert_eq!(
            variables[1].value.as_deref(),
            Some("https://api/$(SERVICE_PORT)")
        );
        assert_eq!(variables[2].value.as_deref(), Some("$(HOST)"));
        assert_eq!(variables[3].value.as_deref(), Some("$HOST"));
    }

    #[test]
    fn environment_variable_resolution_expands_config_map_and_secret_imports() {
        let config_maps = BTreeMap::from([(
            "settings".to_owned(),
            ConfigMap {
                data: Some(BTreeMap::from([("HOST".to_owned(), "api".to_owned())])),
                ..Default::default()
            },
        )]);
        let secrets = BTreeMap::from([(
            "credentials".to_owned(),
            Secret {
                data: Some(BTreeMap::from([(
                    "token".to_owned(),
                    k8s_openapi::ByteString(b"secret-value".to_vec()),
                )])),
                ..Default::default()
            },
        )]);
        let variables = [
            PodEnvironmentVariableDetail {
                name: "CONFIG".to_owned(),
                value: None,
                source: PodEnvironmentVariableSource::ConfigMapKey {
                    name: "settings".to_owned(),
                    key: "HOST".to_owned(),
                    optional: false,
                },
            },
            PodEnvironmentVariableDetail {
                name: "Import Secret credentials".to_owned(),
                value: None,
                source: PodEnvironmentVariableSource::SecretImport {
                    name: "credentials".to_owned(),
                    prefix: "APP_".to_owned(),
                    optional: false,
                },
            },
        ];

        let resolved = variables
            .into_iter()
            .flat_map(|variable| resolve_environment_variable(variable, &config_maps, &secrets))
            .collect::<Vec<_>>();

        assert_eq!(resolved[0].value.as_deref(), Some("api"));
        assert_eq!(resolved[1].name, "APP_token");
        assert_eq!(resolved[1].value.as_deref(), Some("secret-value"));
        assert!(matches!(
            resolved[1].source,
            PodEnvironmentVariableSource::SecretKey { .. }
        ));
    }

    #[test]
    fn pod_extractor_populates_ready_status_and_restarts() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some("api".to_owned()),
                namespace: Some("default".to_owned()),
                uid: Some("pod-uid".to_owned()),
                ..Default::default()
            },
            status: Some(PodStatus {
                phase: Some("Running".to_owned()),
                container_statuses: Some(vec![
                    ContainerStatus {
                        ready: true,
                        restart_count: 2,
                        ..Default::default()
                    },
                    ContainerStatus {
                        ready: false,
                        restart_count: 3,
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let resource = crate::resource_handlers::pod::extract(&pod);

        assert_eq!(resource.uid, "pod-uid");
        assert_eq!(
            resource.cells.get(READY_COLUMN),
            Some(&CellValue::Text("1/2".to_owned()))
        );
        assert_eq!(
            resource.cells.get(STATUS_COLUMN),
            Some(&CellValue::Status {
                label: "Running".to_owned(),
                tone: crate::resource_table::StatusTone::Success,
            })
        );
        assert_eq!(
            resource.cells.get(RESTARTS_COLUMN),
            Some(&CellValue::Number(5))
        );
    }

    #[test]
    fn deployment_extractor_populates_replica_columns() {
        let deployment = Deployment {
            metadata: ObjectMeta {
                name: Some("api".to_owned()),
                namespace: Some("default".to_owned()),
                ..Default::default()
            },
            status: Some(DeploymentStatus {
                replicas: Some(4),
                ready_replicas: Some(3),
                updated_replicas: Some(2),
                available_replicas: Some(3),
                ..Default::default()
            }),
            ..Default::default()
        };

        let resource = crate::resource_handlers::deployment::extract(&deployment);

        assert_eq!(
            resource.cells.get(READY_COLUMN),
            Some(&CellValue::Text("3/4".to_owned()))
        );
        assert_eq!(
            resource.cells.get(UP_TO_DATE_COLUMN),
            Some(&CellValue::Number(2))
        );
        assert_eq!(
            resource.cells.get(AVAILABLE_COLUMN),
            Some(&CellValue::Number(3))
        );
    }

    #[test]
    fn dynamic_resource_extractor_evaluates_custom_printer_columns_locally() {
        let columns = vec![CustomResourceColumn {
            id: "crd-0".to_owned(),
            label: "State".to_owned(),
            json_path: ".status.conditions[*].type".to_owned(),
            type_: "string".to_owned(),
            format: None,
        }];

        let cells = extract_custom_cells(
            &k8s_openapi::serde_json::json!({
                "status": { "conditions": [{ "type": "Ready" }, { "type": "Synced" }] }
            }),
            &columns,
        );

        assert_eq!(
            cells.get("crd-0"),
            Some(&CellValue::List(vec![
                "Ready".to_owned(),
                "Synced".to_owned()
            ]))
        );

        let resource = extract_minimal_resource(
            &DynamicObject {
                types: None,
                metadata: ObjectMeta {
                    name: Some("widget".into()),
                    labels: Some(BTreeMap::from([("app".into(), "api".into())])),
                    annotations: Some(BTreeMap::from([(
                        "example.com/team".into(),
                        "platform".into(),
                    )])),
                    ..Default::default()
                },
                data: k8s_openapi::serde_json::json!({}),
            },
            &[],
        );

        assert_eq!(resource.labels["app"], "api");
        assert_eq!(resource.annotations["example.com/team"], "platform");
    }

    #[test]
    fn managed_resource_tree_keeps_only_supported_controller_descendants() {
        let replica_set = ManagedResource {
            api_resource: api_resource_for::<ReplicaSet>(),
            name: "api-7b948f".into(),
            namespace: Some("default".into()),
            uid: "replicaset-uid".into(),
            association: ManagedResourceAssociation::ControllerOwnerUid("deployment-uid".into()),
            creation_timestamp: None,
            cells: BTreeMap::new(),
        };
        let pod = ManagedResource {
            api_resource: api_resource_for::<Pod>(),
            name: "api-7b948f-pod".into(),
            namespace: Some("default".into()),
            uid: "pod-uid".into(),
            association: ManagedResourceAssociation::ControllerOwnerUid("replicaset-uid".into()),
            creation_timestamp: None,
            cells: BTreeMap::new(),
        };
        let unrelated_pod = ManagedResource {
            uid: "other-pod-uid".into(),
            association: ManagedResourceAssociation::ControllerOwnerUid("other-uid".into()),
            ..pod.clone()
        };
        let resources = BTreeMap::from([
            (replica_set.api_resource.clone(), vec![replica_set.clone()]),
            (
                pod.api_resource.clone(),
                vec![pod.clone(), unrelated_pod.clone()],
            ),
        ]);

        assert!(belongs_to_workload_tree(
            &replica_set,
            "deployment-uid",
            &api_resource_for::<Deployment>(),
            &resources
        ));
        assert!(belongs_to_workload_tree(
            &pod,
            "deployment-uid",
            &api_resource_for::<Deployment>(),
            &resources
        ));
        assert!(!belongs_to_workload_tree(
            &unrelated_pod,
            "deployment-uid",
            &api_resource_for::<Deployment>(),
            &resources
        ));
        let directly_owned_pod = ManagedResource {
            association: ManagedResourceAssociation::ControllerOwnerUid("deployment-uid".into()),
            ..pod.clone()
        };
        assert!(!belongs_to_workload_tree(
            &directly_owned_pod,
            "deployment-uid",
            &api_resource_for::<Deployment>(),
            &resources
        ));
    }

    #[test]
    fn node_association_keeps_only_pods_scheduled_to_the_inspected_node() {
        let scheduled = ManagedResource {
            api_resource: api_resource_for::<Pod>(),
            name: "api".into(),
            namespace: Some("default".into()),
            uid: "pod-uid".into(),
            association: ManagedResourceAssociation::NodeName("kind-control-plane".into()),
            creation_timestamp: None,
            cells: BTreeMap::new(),
        };
        let elsewhere = ManagedResource {
            association: ManagedResourceAssociation::NodeName("kind-worker".into()),
            ..scheduled.clone()
        };

        assert!(belongs_to_node(&scheduled, "kind-control-plane"));
        assert!(!belongs_to_node(&elsewhere, "kind-control-plane"));
    }
}
