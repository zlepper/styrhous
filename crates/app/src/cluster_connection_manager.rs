use crate::api_resource::ApiResource;
use crate::helpers::ResultExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::resource_detail::{
    ManagedResource, ManagedResourceAssociation, PodEnvironmentVariableDetail,
    PodEnvironmentVariableSource, ResourceDetail, ResourceDetailPayload, ResourceEvent,
    ResourceOwner,
};
use crate::resource_handlers;
use crate::resource_schema::ResourceSchema;
use crate::resource_table::{CellValue, CustomResourceColumn};
use crate::worker::{WorkerResult, WorkerResultSender};
use anyhow::{Context, Result, bail};
use futures_util::future::try_join_all;
use futures_util::pin_mut;
use futures_util::stream::StreamExt;
use http::Request;
use itertools::Itertools;
use k8s_openapi::api::apps::v1::{Deployment, ReplicaSet};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{ConfigMap, Event as KubernetesEvent, Namespace, Pod, Secret};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{APIGroup, GroupVersionForDiscovery};
use k8s_openapi::{ClusterResourceScope, NamespaceResourceScope};
use kube::api::DynamicObject;
use kube::api::GroupVersionKind;
use kube::config::KubeConfigOptions;
use kube::config::Kubeconfig;
use kube::runtime::watcher;
use kube::runtime::watcher::{Event, ListSemantic};
use kube::{Api, Resource};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use time::OffsetDateTime;
use tokio::task::{JoinHandle, JoinSet};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct Cluster {
    pub name: String,
    pub cluster: Option<String>,
    pub is_current: bool,
}

pub async fn reload_kubeconfig() -> Result<WorkerResult> {
    let cfg = Kubeconfig::read().with_context(|| "Error reading kubeconfig")?;
    let current_context = cfg.current_context.clone();

    let mut clusters = Vec::new();

    for named_context in cfg.contexts {
        clusters.push(Cluster {
            name: named_context.name.clone(),
            cluster: named_context.context.map(|c| c.cluster).clone(),
            is_current: current_context.as_deref() == Some(named_context.name.as_str()),
        });
    }

    Ok(WorkerResult::KubernetesClustersUpdated(clusters))
}

pub struct ClusterConnection {
    client: kube::Client,
    join_handles: Vec<JoinHandle<()>>,
    cluster_key: i32,
}

impl Debug for ClusterConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterStateRunner")
            .field("cluster_key", &self.cluster_key)
            .finish()
    }
}

impl ClusterConnection {
    /// Get a clone of the kube client for starting additional watchers
    pub fn client(&self) -> kube::Client {
        self.client.clone()
    }

    pub async fn new(
        cluster_key: i32,
        context_name: &str,
        event_output: WorkerResultSender,
    ) -> Result<Self> {
        let config = kube::Config::from_kubeconfig(&KubeConfigOptions {
            context: Some(context_name.to_string()),
            ..Default::default()
        })
        .await
        .with_context(|| "Error creating Kubernetes config")?;
        let client =
            kube::Client::try_from(config).with_context(|| "Error creating Kubernetes client")?;

        let namespaces_task = {
            let namespace_watcher = KubernetesNamespaceWatcher {
                event_sender: event_output.clone(),
                client: client.clone(),
                cluster_key,
            };

            namespace_watcher.watch_namespaces()
        };

        let namespaces_handle = tokio::spawn(namespaces_task);

        let api_resources_task = {
            let api_resource_inspector = KubernetesApiInspector {
                client: client.clone(),
            };
            let event_output = event_output.clone();

            async move {
                match api_resource_inspector.inspect_api().await {
                    Err(error) => event_output
                        .send(WorkerResult::KubernetesApisLoadFailed {
                            cluster_key,
                            error: format!("{error:#?}"),
                        })
                        .log_if_error("Failed to send error from inspecting resource api"),
                    Ok(inspection) => {
                        event_output
                            .send(WorkerResult::KubernetesApisLoaded {
                                cluster_key,
                                api_resources: inspection.api_resources,
                            })
                            .log_if_error("Failed to send kubernetes API resources");
                        event_output
                            .send(WorkerResult::KubernetesCustomResourceColumnsLoaded {
                                cluster_key,
                                columns: inspection.custom_resource_columns,
                            })
                            .log_if_error("Failed to send custom resource columns");
                        event_output
                            .send(WorkerResult::KubernetesResourceSchemasLoaded {
                                cluster_key,
                                schemas: inspection.resource_schemas,
                            })
                            .log_if_error("Failed to send custom resource schemas");
                    }
                }
            }
        };

        let api_resources_handle = tokio::spawn(api_resources_task);

        Ok(Self {
            client,
            join_handles: vec![namespaces_handle, api_resources_handle],
            cluster_key,
        })
    }
}

struct KubernetesApiInspector {
    client: kube::Client,
}

struct ApiInspection {
    api_resources: Vec<ApiResource>,
    custom_resource_columns: BTreeMap<ApiResource, Vec<CustomResourceColumn>>,
    resource_schemas: BTreeMap<ApiResource, ResourceSchema>,
}

impl KubernetesApiInspector {
    async fn get_api_resources_for_group_versions(
        &self,
        api_group: APIGroup,
        versions: Vec<GroupVersionForDiscovery>,
    ) -> Result<Vec<ApiResource>> {
        let tasks = versions.iter().map(|api_group_version| {
            self.client
                .list_api_group_resources(&api_group_version.group_version)
        });

        let api_group_name = api_group.name;
        let resources = try_join_all(tasks)
            .await?
            .iter()
            .zip(versions)
            .flat_map(|(resources, version)| {
                let version_name = version.version.clone();

                let mut temp = Vec::new();

                for resource in &resources.resources {
                    // Skip resources like "Status" and "Scale"
                    if resource.name.contains('/') {
                        continue;
                    }

                    temp.push(ApiResource {
                        group: api_group_name.clone(),
                        version: version_name.clone(),
                        kind: resource.kind.clone(),
                        name: resource.name.clone(),
                        namespaced: resource.namespaced,
                    });
                }

                temp
            })
            .collect();

        Ok(resources)
    }

    async fn get_core_api_resources(&self) -> Result<Vec<ApiResource>> {
        let core_api_versions = self.client.list_core_api_versions().await?;

        let mut resources = Vec::new();

        for version in &core_api_versions.versions {
            let api_resources = self.client.list_core_api_resources(version).await?;

            for resource in api_resources.resources {
                if resource.name.contains("/") {
                    continue;
                }

                resources.push(ApiResource {
                    group: "core".to_string(),
                    version: version.clone(),
                    kind: resource.kind.clone(),
                    name: resource.name.clone(),
                    namespaced: resource.namespaced,
                });
            }
        }

        Ok(resources)
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

        let mut resources = try_join_all(tasks)
            .await?
            .into_iter()
            .flatten()
            .collect_vec();
        resources.extend(core_resources);

        let (custom_resource_columns, resource_schemas) = self.custom_resource_metadata().await;
        Ok(ApiInspection {
            api_resources: resources,
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

impl Drop for ClusterConnection {
    fn drop(&mut self) {
        for handle in self.join_handles.drain(..) {
            handle.abort_handle().abort()
        }
    }
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
                        .send(WorkerResult::KubernetesNamespacesLoadFailed {
                            cluster_key: self.cluster_key,
                            error: format!("{error:#?}"),
                        })
                        .log_if_error("Failed to send namespace watcher error");
                    return;
                }
            };
            match ev {
                Event::Apply(item) => {
                    self.event_sender
                        .send(WorkerResult::KubernetesNamespacesAdded {
                            namespace: item.into(),
                            cluster_key: self.cluster_key,
                        })
                        .log_if_error("Failed to send updated namespace");
                }
                Event::Delete(item) => {
                    self.event_sender
                        .send(WorkerResult::KubernetesNamespacesDeleted {
                            cluster_key: self.cluster_key,
                            namespace_name: item.metadata.name.expect(
                                "Deleted Namespace from the api server did not have a name",
                            ),
                        })
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
                        .send(WorkerResult::KubernetesNamespacesReplaced {
                            cluster_key: self.cluster_key,
                            namespaces: buffer,
                        })
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
) -> Result<WorkerResult> {
    info!("Starting cluster connection: {}", cluster_name);
    let runner = ClusterConnection::new(cluster_key, cluster_name, event_sender).await?;

    Ok(WorkerResult::KubernetesClusterConnectionCreated {
        cluster_key,
        runner: Some(runner),
    })
}

/// Start watching a resource type in its selected namespace scope.
pub async fn start_resource_watcher(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    event_sender: WorkerResultSender,
) -> Result<WorkerResult> {
    info!(
        "Starting resource watcher for {}/{} in {}",
        api_resource.group,
        api_resource.name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

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
            .send(WorkerResult::KubernetesCustomResourceColumnsLoaded {
                cluster_key,
                columns: BTreeMap::from([(api_resource.clone(), custom_columns.clone())]),
            })
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

    tokio::spawn(watcher.watch_resources());

    Ok(WorkerResult::KubernetesResourceWatchStarted {
        cluster_key,
        api_resource,
        namespace,
    })
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
                    .send(WorkerResult::KubernetesResourceWatchFailed {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: self.namespace.clone(),
                        error: format!("{error:#?}"),
                    })
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
                    .send(WorkerResult::KubernetesResourceWatchFailed {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: self.namespace.clone(),
                        error,
                    })
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
                        .send(WorkerResult::KubernetesResourceWatchFailed {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            error: format!("{error:#?}"),
                        })
                        .log_if_error("Failed to send resource watcher error");
                    return;
                }
            };
            match ev {
                Event::Apply(item) => {
                    let resource = extract_minimal_resource(&item, &self.custom_columns);
                    self.event_sender
                        .send(WorkerResult::KubernetesResourceAdded {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            resource,
                        })
                        .log_if_error("Failed to send resource added");
                }
                Event::Delete(item) => {
                    let uid = get_resource_uid(&item);
                    self.event_sender
                        .send(WorkerResult::KubernetesResourceDeleted {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            resource_uid: uid,
                        })
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
                        .send(WorkerResult::KubernetesResourcesReplaced {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            resources: buffer,
                        })
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
                .send(WorkerResult::KubernetesResourceWatchFailed {
                    cluster_key: self.cluster_key,
                    api_resource: self.api_resource,
                    namespace: None,
                    error: "A namespaced typed watcher was started without a namespace".to_owned(),
                })
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
                        .send(WorkerResult::KubernetesResourceWatchFailed {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            error: format!("{error:#?}"),
                        })
                        .log_if_error("Failed to send typed resource watcher error");
                    return;
                }
            };

            match event {
                Event::Apply(item) => self
                    .event_sender
                    .send(WorkerResult::KubernetesResourceAdded {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: self.namespace.clone(),
                        resource: (self.extract)(&item),
                    })
                    .log_if_error("Failed to send typed resource added"),
                Event::Delete(item) => self
                    .event_sender
                    .send(WorkerResult::KubernetesResourceDeleted {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: self.namespace.clone(),
                        resource_uid: get_resource_uid(&item),
                    })
                    .log_if_error("Failed to send typed resource deleted"),
                Event::Init => buffer.clear(),
                Event::InitApply(item) => buffer.push((self.extract)(&item)),
                Event::InitDone => {
                    self.event_sender
                        .send(WorkerResult::KubernetesResourcesReplaced {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: self.namespace.clone(),
                            resources: buffer,
                        })
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
                .send(WorkerResult::KubernetesResourceWatchFailed {
                    cluster_key: self.cluster_key,
                    api_resource: self.api_resource,
                    namespace: self.namespace,
                    error: "A cluster-scoped typed watcher was started with a namespace".to_owned(),
                })
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
                        .send(WorkerResult::KubernetesResourceWatchFailed {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: None,
                            error: format!("{error:#?}"),
                        })
                        .log_if_error("Failed to send typed resource watcher error");
                    return;
                }
            };

            match event {
                Event::Apply(item) => self
                    .event_sender
                    .send(WorkerResult::KubernetesResourceAdded {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: None,
                        resource: (self.extract)(&item),
                    })
                    .log_if_error("Failed to send typed resource added"),
                Event::Delete(item) => self
                    .event_sender
                    .send(WorkerResult::KubernetesResourceDeleted {
                        cluster_key: self.cluster_key,
                        api_resource: self.api_resource.clone(),
                        namespace: None,
                        resource_uid: get_resource_uid(&item),
                    })
                    .log_if_error("Failed to send typed resource deleted"),
                Event::Init => buffer.clear(),
                Event::InitApply(item) => buffer.push((self.extract)(&item)),
                Event::InitDone => {
                    self.event_sender
                        .send(WorkerResult::KubernetesResourcesReplaced {
                            cluster_key: self.cluster_key,
                            api_resource: self.api_resource.clone(),
                            namespace: None,
                            resources: buffer,
                        })
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
            &ts.0.to_rfc3339(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    });

    MinimalResource {
        uid,
        name: metadata.name.clone().unwrap_or_default(),
        namespace: metadata.namespace.clone(),
        creation_timestamp,
        cells: extract_custom_cells(&obj.data, custom_columns),
        log_containers: Vec::new(),
    }
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
            let values = value.as_array()?.iter().cloned().collect::<Vec<_>>();
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
        if matches!(column.type_.as_str(), "integer" | "number") {
            if let Some(number) = value.as_i64() {
                return Some(CellValue::Number(number));
            }
        }
        if matches!(column.type_.as_str(), "date" | "date-time") {
            if let Some(value) = value.as_str().and_then(parse_timestamp) {
                return Some(CellValue::Timestamp(value));
            }
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

pub(crate) fn minimal_resource_from_typed<T: Resource>(
    obj: &T,
    cells: BTreeMap<String, CellValue>,
) -> MinimalResource {
    let metadata = obj.meta();
    let creation_timestamp = metadata.creation_timestamp.as_ref().and_then(|timestamp| {
        OffsetDateTime::parse(
            &timestamp.0.to_rfc3339(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    });

    MinimalResource {
        uid: get_resource_uid(obj),
        name: metadata.name.clone().unwrap_or_default(),
        namespace: metadata.namespace.clone(),
        creation_timestamp,
        cells,
        log_containers: Vec::new(),
    }
}

/// Keep one inspector history entry current independently of the compact
/// resource-table watcher. The worker owns it until that entry leaves history.
pub async fn watch_resource_detail(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    resource_uid: String,
    history_entry_id: u64,
    event_sender: WorkerResultSender,
) {
    let root_name = resource_name.clone();
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
        watch_managed_resources(
            cluster_key,
            client,
            api_resource,
            namespace,
            root_name,
            resource_uid,
            history_entry_id,
            event_sender,
        ),
    );
}

/// Watch the small, well-known set of resource kinds which can make up a
/// built-in workload controller hierarchy. Kubernetes has no generic reverse
/// owner-reference query, so this deliberately does not attempt custom types.
async fn watch_managed_resources(
    cluster_key: i32,
    client: kube::Client,
    root_api_resource: ApiResource,
    namespace: Option<String>,
    root_name: String,
    root_uid: String,
    history_entry_id: u64,
    event_sender: WorkerResultSender,
) {
    let resource_types = managed_resource_types(&root_api_resource);
    if resource_types.is_empty() {
        event_sender
            .send(WorkerResult::ManagedResourcesReplaced {
                cluster_key,
                history_entry_id,
                resources: Vec::new(),
            })
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
                    .send(WorkerResult::ManagedResourcesReplaced {
                        cluster_key,
                        history_entry_id,
                        resources,
                    })
                    .log_if_error("Failed to send managed resource update");
            }
            ManagedResourceUpdate::Failed {
                api_resource,
                error,
            } => event_sender
                .send(WorkerResult::ManagedResourcesWatchFailed {
                    cluster_key,
                    history_entry_id,
                    error: format!("Unable to watch {}: {error}", api_resource.display_name()),
                })
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
    let api = match create_dynamic_api(&client, &api_resource, namespace.as_deref()).await {
        Ok(api) => api,
        Err(error) => {
            send_detail_error(&event_sender, cluster_key, history_entry_id, false, error);
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
                send_detail_error(&event_sender, cluster_key, history_entry_id, false, error);
                return;
            }
        };
        match event {
            Event::Apply(object) => {
                event_sender
                    .send(WorkerResult::ResourceDetailUpdated {
                        cluster_key,
                        history_entry_id,
                        detail: resource_detail_from_dynamic(&client, api_resource.clone(), object)
                            .await,
                    })
                    .log_if_error("Failed to send resource detail update");
            }
            Event::InitApply(object) => {
                found_during_initial_list = true;
                event_sender
                    .send(WorkerResult::ResourceDetailUpdated {
                        cluster_key,
                        history_entry_id,
                        detail: resource_detail_from_dynamic(&client, api_resource.clone(), object)
                            .await,
                    })
                    .log_if_error("Failed to send resource detail update");
            }
            Event::Delete(_) => event_sender
                .send(WorkerResult::ResourceDetailDeleted {
                    cluster_key,
                    history_entry_id,
                })
                .log_if_error("Failed to send resource detail deletion"),
            Event::Init => found_during_initial_list = false,
            Event::InitDone if !found_during_initial_list => event_sender
                .send(WorkerResult::ResourceDetailDeleted {
                    cluster_key,
                    history_entry_id,
                })
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
                send_detail_error(&event_sender, cluster_key, history_entry_id, true, error);
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
        send_detail_events(&event_sender, cluster_key, history_entry_id, &events);
    }
}

fn send_detail_events(
    event_sender: &WorkerResultSender,
    cluster_key: i32,
    history_entry_id: u64,
    events: &BTreeMap<String, ResourceEvent>,
) {
    let mut events = events.values().cloned().collect::<Vec<_>>();
    events.sort_by(|left, right| right.last_timestamp.cmp(&left.last_timestamp));
    event_sender
        .send(WorkerResult::ResourceEventsReplaced {
            cluster_key,
            history_entry_id,
            events,
        })
        .log_if_error("Failed to send resource event update");
}

fn send_detail_error(
    event_sender: &WorkerResultSender,
    cluster_key: i32,
    history_entry_id: u64,
    events: bool,
    error: impl std::fmt::Debug,
) {
    event_sender
        .send(WorkerResult::ResourceDetailWatchFailed {
            cluster_key,
            history_entry_id,
            events,
            error: format!("{error:#?}"),
        })
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
            &timestamp.0.to_rfc3339(),
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
        creation_timestamp,
        owner: metadata
            .owner_references
            .as_ref()
            .and_then(|owners| owners.first())
            .map(|owner| ResourceOwner {
                kind: owner.kind.clone(),
                name: owner.name.clone(),
            }),
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
        if matches!(variable.source, PodEnvironmentVariableSource::Literal) {
            if let Some(value) = &variable.value {
                variable.value = Some(expand_environment_variable_value(value, &values));
            }
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
        while let Some(character) = characters.next() {
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
            &timestamp.0.to_rfc3339(),
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
    } else {
        event.last_timestamp.as_ref().and_then(|timestamp| {
            OffsetDateTime::parse(
                &timestamp.0.to_rfc3339(),
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

/// Helper to create a namespaced or cluster-scoped API for a given resource type
async fn create_dynamic_api(
    client: &kube::Client,
    api_resource: &ApiResource,
    namespace: Option<&str>,
) -> Result<Api<DynamicObject>> {
    let group = if api_resource.group == "core" {
        ""
    } else {
        &api_resource.group
    };

    let gvk = GroupVersionKind::gvk(group, &api_resource.version, &api_resource.kind);
    let (ar, caps) = kube::discovery::pinned_kind(client, &gvk).await?;

    let api = match (caps.scope, namespace) {
        (kube::discovery::Scope::Namespaced, Some(namespace)) => {
            Api::namespaced_with(client.clone(), namespace, &ar)
        }
        (kube::discovery::Scope::Cluster, None) => Api::all_with(client.clone(), &ar),
        (scope, namespace) => bail!(
            "Resource scope mismatch: discovered {scope:?} scope with namespace {namespace:?}"
        ),
    };

    Ok(api)
}

/// Fetch a resource's full YAML representation
pub async fn get_resource_yaml(
    editor_id: u64,
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
) -> Result<WorkerResult> {
    info!(
        "Getting YAML for {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    let api = create_dynamic_api(&client, &api_resource, namespace.as_deref()).await?;
    let mut obj = api.get(&resource_name).await?;

    // Strip server-managed fields that clutter the editor and cause issues on apply
    if let Some(metadata) = obj.data.get_mut("metadata") {
        if let Some(meta_obj) = metadata.as_object_mut() {
            meta_obj.remove("managedFields");
            meta_obj.remove("resourceVersion");
            meta_obj.remove("uid");
            meta_obj.remove("creationTimestamp");
        }
    }
    obj.metadata.managed_fields = None;
    obj.metadata.resource_version = None;
    obj.metadata.uid = None;
    obj.metadata.creation_timestamp = None;

    let yaml = serde_yaml::to_string(&obj)?;

    Ok(WorkerResult::ResourceYamlFetched {
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
) -> Result<WorkerResult> {
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
    Ok(WorkerResult::ResourceSchemaLoaded {
        editor_id,
        cluster_key,
        api_resource,
        schema,
    })
}

/// Validate the same server-side apply request used by Save without persisting a change.
pub async fn validate_resource_yaml(
    editor_id: u64,
    revision: u64,
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
    yaml: String,
) -> Result<WorkerResult> {
    let mut obj: DynamicObject = serde_yaml::from_str(&yaml)?;
    if let Some(metadata) = obj.data.get_mut("metadata")
        && let Some(meta_obj) = metadata.as_object_mut()
    {
        for field in [
            "managedFields",
            "resourceVersion",
            "uid",
            "creationTimestamp",
        ] {
            meta_obj.remove(field);
        }
    }
    obj.metadata.managed_fields = None;
    obj.metadata.resource_version = None;
    obj.metadata.uid = None;
    obj.metadata.creation_timestamp = None;

    let api = create_dynamic_api(&client, &api_resource, namespace.as_deref()).await?;
    let params = kube::api::PatchParams::apply("kubernetes-dev-ui")
        .force()
        .validation(kube::api::ValidationDirective::Strict)
        .dry_run();
    api.patch(&resource_name, &params, &kube::api::Patch::Apply(&obj))
        .await?;
    Ok(WorkerResult::ResourceYamlValidated {
        editor_id,
        revision,
        cluster_key,
        api_resource,
        namespace,
        resource_name,
    })
}

/// Delete a resource
pub async fn delete_resource(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: Option<String>,
    resource_name: String,
) -> Result<WorkerResult> {
    info!(
        "Deleting {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    let api = create_dynamic_api(&client, &api_resource, namespace.as_deref()).await?;
    api.delete(&resource_name, &Default::default()).await?;

    Ok(WorkerResult::ResourceDeleteCompleted {
        cluster_key,
        api_resource,
        namespace,
        resource_name,
    })
}

/// Trigger a Deployment rollout the same way `kubectl rollout restart` does.
pub async fn restart_deployment(
    cluster_key: i32,
    client: kube::Client,
    namespace: String,
    resource_name: String,
) -> Result<WorkerResult> {
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

    Ok(WorkerResult::DeploymentRestartCompleted {
        cluster_key,
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
) -> Result<WorkerResult> {
    info!(
        "Applying YAML for {}/{} {} in {}",
        api_resource.group,
        api_resource.name,
        resource_name,
        namespace.as_deref().unwrap_or("cluster-wide scope")
    );

    let mut obj: DynamicObject = serde_yaml::from_str(&yaml)?;

    // Strip fields that cannot be sent with server-side apply
    if let Some(metadata) = obj.data.get_mut("metadata") {
        if let Some(meta_obj) = metadata.as_object_mut() {
            meta_obj.remove("managedFields");
            meta_obj.remove("resourceVersion");
            meta_obj.remove("uid");
            meta_obj.remove("creationTimestamp");
        }
    }
    // Also clear from the typed metadata
    obj.metadata.managed_fields = None;
    obj.metadata.resource_version = None;
    obj.metadata.uid = None;
    obj.metadata.creation_timestamp = None;

    let api = create_dynamic_api(&client, &api_resource, namespace.as_deref()).await?;

    // Use server-side apply with force to take ownership of fields
    let patch_params = kube::api::PatchParams::apply("kubernetes-dev-ui").force();
    api.patch(
        &resource_name,
        &patch_params,
        &kube::api::Patch::Apply(&obj),
    )
    .await?;

    Ok(WorkerResult::ResourceApplyCompleted {
        editor_id,
        cluster_key,
        api_resource,
        namespace,
        resource_name,
    })
}

/// Replace selected existing data values while preserving every other field. The
/// fetched object's resourceVersion makes a concurrent update fail rather than
/// silently overwriting it.
pub async fn update_resource_data(
    cluster_key: i32,
    client: kube::Client,
    api_resource: ApiResource,
    namespace: String,
    resource_name: String,
    expected_values: &BTreeMap<String, String>,
    updated_values: &BTreeMap<String, String>,
    expected_resource_version: &str,
) -> Result<WorkerResult> {
    if expected_values.is_empty() || updated_values.is_empty() {
        bail!("Resource data update must contain at least one existing value");
    }
    if expected_values.keys().ne(updated_values.keys()) {
        bail!("Resource data update expected and updated keys must match");
    }
    if expected_resource_version.is_empty() {
        bail!("Resource data update is missing the watched resource version");
    }

    if resource_handlers::matches_namespaced_api_resource::<ConfigMap>(&api_resource) {
        let api: Api<ConfigMap> = Api::namespaced(client, &namespace);
        let mut config_map = api.get(&resource_name).await?;
        if config_map.metadata.resource_version.as_deref() != Some(expected_resource_version) {
            bail!("ConfigMap changed on the cluster; reload its data before saving");
        }
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
        if secret.metadata.resource_version.as_deref() != Some(expected_resource_version) {
            bail!("Secret changed on the cluster; reload its data before saving");
        }
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

    Ok(WorkerResult::ResourceDataUpdateCompleted {
        cluster_key,
        resource_name,
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
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

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
