use crate::api_resource::ApiResource;
use crate::helpers::ResultExt;
use crate::minimal_namespace::MinimalNamespace;
use crate::minimal_resource::MinimalResource;
use crate::worker::{WorkerResult, WorkerResultSender};
use anyhow::{Context, Result, bail};
use futures_util::future::try_join_all;
use futures_util::pin_mut;
use futures_util::stream::StreamExt;
use itertools::Itertools;
use k8s_openapi::api::core::v1::Namespace;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{APIGroup, GroupVersionForDiscovery};
use kube::Api;
use kube::api::DynamicObject;
use kube::api::GroupVersionKind;
use kube::config::KubeConfigOptions;
use kube::config::Kubeconfig;
use kube::runtime::watcher;
use kube::runtime::watcher::{Event, ListSemantic};
use std::fmt::Debug;
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
                    Ok(apis) => event_output
                        .send(WorkerResult::KubernetesApisLoaded {
                            cluster_key,
                            api_resources: apis,
                        })
                        .log_if_error("Failed to send kubernetes API resources"),
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

    pub async fn inspect_api(&self) -> Result<Vec<ApiResource>> {
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

        Ok(resources)
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

    let watcher = KubernetesResourceWatcher {
        client,
        event_sender: event_sender.clone(),
        cluster_key,
        api_resource: api_resource.clone(),
        namespace: namespace.clone(),
    };

    tokio::spawn(watcher.watch_resources());

    Ok(WorkerResult::KubernetesResourceWatchStarted {
        cluster_key,
        api_resource,
        namespace,
    })
}

struct KubernetesResourceWatcher {
    client: kube::Client,
    event_sender: WorkerResultSender,
    cluster_key: i32,
    api_resource: ApiResource,
    namespace: Option<String>,
}

impl KubernetesResourceWatcher {
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
                    let resource = extract_minimal_resource(&item, &self.api_resource);
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
                    buffer.push(extract_minimal_resource(&item, &self.api_resource));
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

/// Get a unique identifier for a resource
fn get_resource_uid(obj: &DynamicObject) -> String {
    obj.metadata.uid.clone().unwrap_or_else(|| {
        format!(
            "{}/{}",
            obj.metadata.namespace.as_deref().unwrap_or(""),
            obj.metadata.name.as_deref().unwrap_or("")
        )
    })
}

/// Extract a MinimalResource from a DynamicObject
fn extract_minimal_resource(obj: &DynamicObject, api_resource: &ApiResource) -> MinimalResource {
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

    // Extract status/phase based on resource type
    let (phase, ready_status) = extract_status(obj, api_resource);

    MinimalResource {
        uid,
        name: metadata.name.clone().unwrap_or_default(),
        namespace: metadata.namespace.clone(),
        creation_timestamp,
        phase,
        ready_status,
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

/// Extract status information based on resource type
fn extract_status(
    obj: &DynamicObject,
    api_resource: &ApiResource,
) -> (Option<String>, Option<String>) {
    let status = obj.data.get("status");

    match api_resource.kind.as_str() {
        "Pod" => {
            let phase = status
                .and_then(|s| s.get("phase"))
                .and_then(|p| p.as_str())
                .map(String::from);

            // Count ready containers
            let ready_status = status
                .and_then(|s| s.get("containerStatuses"))
                .and_then(|cs| cs.as_array())
                .map(|containers| {
                    let total = containers.len();
                    let ready = containers
                        .iter()
                        .filter(|c| c.get("ready").and_then(|r| r.as_bool()).unwrap_or(false))
                        .count();
                    format!("{}/{}", ready, total)
                });

            (phase, ready_status)
        }
        "Deployment" | "ReplicaSet" | "StatefulSet" => {
            let ready = status
                .and_then(|s| s.get("readyReplicas"))
                .and_then(|r| r.as_i64())
                .unwrap_or(0);
            let desired = status
                .and_then(|s| s.get("replicas"))
                .and_then(|r| r.as_i64())
                .unwrap_or(0);

            let ready_status = Some(format!("{}/{}", ready, desired));
            let phase = if ready == desired && desired > 0 {
                Some("Ready".to_string())
            } else if ready > 0 {
                Some("Progressing".to_string())
            } else {
                Some("Pending".to_string())
            };

            (phase, ready_status)
        }
        "Service" => {
            let svc_type = obj
                .data
                .get("spec")
                .and_then(|s| s.get("type"))
                .and_then(|t| t.as_str())
                .map(String::from);
            (svc_type, None)
        }
        "Job" => {
            let succeeded = status
                .and_then(|s| s.get("succeeded"))
                .and_then(|r| r.as_i64())
                .unwrap_or(0);
            let failed = status
                .and_then(|s| s.get("failed"))
                .and_then(|r| r.as_i64())
                .unwrap_or(0);
            let active = status
                .and_then(|s| s.get("active"))
                .and_then(|r| r.as_i64())
                .unwrap_or(0);

            let phase = if succeeded > 0 {
                Some("Complete".to_string())
            } else if failed > 0 {
                Some("Failed".to_string())
            } else if active > 0 {
                Some("Running".to_string())
            } else {
                Some("Pending".to_string())
            };

            (phase, None)
        }
        _ => {
            // Generic: try to extract a "phase" or "state" field
            let phase = status
                .and_then(|s| s.get("phase").or_else(|| s.get("state")))
                .and_then(|p| p.as_str())
                .map(String::from);
            (phase, None)
        }
    }
}
