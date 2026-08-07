use crate::api_resource::ApiResource;
use crate::helpers::ResultExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::resource_handlers;
use crate::resource_table::{CellValue, CustomResourceColumn};
use crate::worker::{WorkerResult, WorkerResultSender};
use anyhow::{Context, Result, bail};
use futures_util::future::try_join_all;
use futures_util::pin_mut;
use futures_util::stream::StreamExt;
use itertools::Itertools;
use k8s_openapi::api::core::v1::Namespace;
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
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use time::OffsetDateTime;
use tokio::task::JoinHandle;
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

        let custom_resource_columns = self.custom_resource_columns().await;
        Ok(ApiInspection {
            api_resources: resources,
            custom_resource_columns,
        })
    }

    async fn custom_resource_columns(&self) -> BTreeMap<ApiResource, Vec<CustomResourceColumn>> {
        let crds = Api::<CustomResourceDefinition>::all(self.client.clone());
        let Ok(crds) = crds.list(&Default::default()).await else {
            // Access to CRDs is commonly restricted. Dynamic resources still work without
            // their optional columns, so do not fail API discovery in that case.
            return BTreeMap::new();
        };

        crds.items
            .iter()
            .flat_map(|crd| {
                let spec = &crd.spec;
                spec.versions.iter().filter_map(move |version| {
                    version.additional_printer_columns.as_ref().map(|columns| {
                        (
                            ApiResource {
                                group: spec.group.clone(),
                                version: version.name.clone(),
                                kind: spec.names.kind.clone(),
                                name: spec.names.plural.clone(),
                                namespaced: spec.scope == "Namespaced",
                            },
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
                        )
                    })
                })
            })
            .collect()
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
                            namespace_name: item
                                .metadata
                                .namespace
                                .expect("Namespace from the api server did not have a name"),
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
        cluster_key,
        api_resource,
        namespace,
        resource_name,
        yaml,
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

/// Apply (replace) a resource from YAML
pub async fn apply_resource_yaml(
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
        cluster_key,
        api_resource,
        namespace,
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
}
