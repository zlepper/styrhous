use std::fmt::Debug;
use crate::worker::{WorkerResult, WorkerResultSender};
use anyhow::Context;
use anyhow::Result;
use kube::config::Kubeconfig;
use futures_util::future::try_join_all;
use futures_util::pin_mut;
use k8s_openapi::api::core::v1::Namespace;
use kube::{Api};
use kube::config::KubeConfigOptions;
use kube::runtime::watcher;
use tokio::task::JoinHandle;
use futures_util::stream::StreamExt;
use itertools::Itertools;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{APIGroup, GroupVersionForDiscovery};
use kube::runtime::watcher::{Event, ListSemantic};
use tracing::info;
use crate::api_resource::ApiResource;
use crate::helpers::ResultExt;
use crate::minimal_namespace::MinimalNamespace;

#[derive(Debug, Clone)]
pub struct Cluster {
    pub name: String,
    pub cluster: Option<String>,
}

pub async fn reload_kubeconfig() -> Result<WorkerResult> {
    let cfg = Kubeconfig::read().with_context(|| "Error reading kubeconfig")?;

    let mut clusters = Vec::new();

    for named_context in cfg.contexts {
        clusters.push(Cluster {
            name: named_context.name.clone(),
            cluster: named_context.context.map(|c| c.cluster).clone(),
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
    pub async fn new(cluster_key: i32, context_name: &str, event_output: WorkerResultSender) -> Result<Self> {
        let config = kube::Config::from_kubeconfig(&KubeConfigOptions {
            context: Some(context_name.to_string()),
            ..Default::default()
        }).await.with_context(|| "Error creating Kubernetes config")?;
        let client = kube::Client::try_from(config).with_context(|| "Error creating Kubernetes client")?;


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
                    Err(e) => {
                        event_output.send(WorkerResult::CommandFailed {
                            command: None,
                            error: e
                        }).log_if_error("Failed to send error from inspecting resource api")
                    }
                    Ok(apis) => {
                        event_output.send(WorkerResult::KubernetesApisLoaded {
                            cluster_key,
                            api_resources: apis
                        }).log_if_error("Failed to send kubernetes API resources")
                    }
                }

            }
        };

        let api_resources_handle = tokio::spawn(api_resources_task);

        Ok(Self {
            client,
            join_handles: vec![namespaces_handle, api_resources_handle],
            cluster_key
        })
    }
}

struct KubernetesApiInspector {
    client: kube::Client,
}

impl KubernetesApiInspector {
    async fn get_api_resources_for_group_versions(&self, api_group: APIGroup, versions: Vec<GroupVersionForDiscovery>) -> Result<Vec<ApiResource>> {

        if api_group.name.ends_with(".k8s.io") {
            return Ok(Vec::new());
        }

        let tasks = versions.iter().map(|api_group_version| {
            self.client.list_api_group_resources(&api_group_version.group_version)
        });

        let api_group_name = api_group.name;
        let resources = try_join_all(tasks).await?
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
                    });
                }


                temp
            })
            .collect();

        Ok(resources)
    }

    async fn get_core_api_resources(&self) -> Result<Vec<ApiResource>> {
        let core_api_versions = self.client.list_core_api_versions().await?;

        let tasks = core_api_versions.versions.iter().map(|version| {
            self.client.list_core_api_resources(&version)
        });

        let mut resources = Vec::new();

        for resource in try_join_all(tasks).await?.into_iter().flat_map(|r| r.resources) {
            if resource.name.contains("/") {
                continue;
            }

            resources.push(ApiResource {
                group: "core".to_string(),
                version: resource.version.clone().unwrap_or("".to_string()),
                kind: resource.kind.clone(),
                name: resource.name.clone(),
            });
        }

        Ok(resources)
    }

    pub async fn inspect_api(&self) -> Result<Vec<ApiResource>> {
        let api_groups = self.client.list_api_groups().await?;

        let tasks = api_groups.groups.into_iter().map(|api_group| {
            let versions = api_group.preferred_version.clone().map(|v| vec![v]).unwrap_or_else(|| api_group.versions.clone());

            self.get_api_resources_for_group_versions(api_group, versions)
        });


        let core_resources = self.get_core_api_resources().await?;

        let mut resources = try_join_all(tasks).await?.into_iter().flatten().collect_vec();
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


        let stream = watcher(namespace_api, watcher_config())
            .filter_map(|p| async {
                info!("ev: {:?}", p);
                p.ok()
            });

        pin_mut!(stream);

        while let Some(ev) = stream.next().await {
            match ev {
                Event::Apply(item) => {
                    self.event_sender.send(WorkerResult::KubernetesNamespacesAdded {
                        namespace: item.into(),
                        cluster_key: self.cluster_key,
                    }).log_if_error("Failed to send updated namespace");
                }
                Event::Delete(item) => {
                    self.event_sender.send(WorkerResult::KubernetesNamespacesDeleted {
                        cluster_key: self.cluster_key,
                        namespace_name: item.metadata.namespace.expect("Namespace from the api server did not have a name"),
                    }).log_if_error("Failed to send notification about deleted namespace");
                }
                Event::Init => {
                    buffer.clear();
                }
                Event::InitApply(item) => {
                    buffer.push(item.into());
                }
                Event::InitDone => {
                    self.event_sender.send(WorkerResult::KubernetesNamespacesReplaced {
                        cluster_key: self.cluster_key,
                        namespaces: buffer,
                    }).log_if_error("Failed to send entire replaced namespace list");
                    buffer = Vec::new();
                }
            }
        }
    }
}

fn watcher_config() -> watcher::Config {
    watcher::Config {
        list_semantic: ListSemantic::Any,
        initial_list_strategy:  watcher::InitialListStrategy::ListWatch,
        ..Default::default()
    }
}

pub async fn start_cluster_connection(cluster_key: i32, cluster_name: &str, event_sender: WorkerResultSender) -> Result<WorkerResult> {
    info!("Starting cluster connection: {}", cluster_name);
    let runner = ClusterConnection::new(cluster_key, cluster_name, event_sender).await?;

    Ok(WorkerResult::KubernetesClusterConnectionCreated {
        cluster_key,
        runner,
    })
}