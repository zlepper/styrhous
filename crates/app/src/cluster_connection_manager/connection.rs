//! Kubeconfig discovery and the lifetime owner for a connected cluster.

use super::{KubernetesApiInspector, KubernetesNamespaceWatcher};
use crate::helpers::ResultExt;
use crate::worker::{WorkerResult, WorkerResultSender};
use anyhow::{Context, Result};
use kube::config::{KubeConfigOptions, Kubeconfig};
use std::fmt::Debug;
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct Cluster {
    pub name: String,
    pub is_current: bool,
}

pub async fn reload_kubeconfig() -> Result<WorkerResult> {
    let cfg = Kubeconfig::read().with_context(|| "Error reading kubeconfig")?;
    let current_context = cfg.current_context.clone();
    let clusters = cfg
        .contexts
        .into_iter()
        .map(|named_context| Cluster {
            is_current: current_context.as_deref() == Some(named_context.name.as_str()),
            name: named_context.name,
        })
        .collect();
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
    /// Get a clone of the kube client for starting additional watchers.
    pub fn client(&self) -> kube::Client {
        self.client.clone()
    }

    pub async fn new(
        cluster_key: i32,
        context_name: &str,
        event_output: WorkerResultSender,
    ) -> Result<Self> {
        let config = kube::Config::from_kubeconfig(&KubeConfigOptions {
            context: Some(context_name.to_owned()),
            ..Default::default()
        })
        .await
        .with_context(|| "Error creating Kubernetes config")?;
        let client =
            kube::Client::try_from(config).with_context(|| "Error creating Kubernetes client")?;

        let namespaces_handle = tokio::spawn(
            KubernetesNamespaceWatcher {
                event_sender: event_output.clone(),
                client: client.clone(),
                cluster_key,
            }
            .watch_namespaces(),
        );
        let api_resources_handle = tokio::spawn(load_api_resources(
            cluster_key,
            client.clone(),
            event_output,
        ));

        Ok(Self {
            client,
            join_handles: vec![namespaces_handle, api_resources_handle],
            cluster_key,
        })
    }
}

impl Drop for ClusterConnection {
    fn drop(&mut self) {
        for handle in self.join_handles.drain(..) {
            handle.abort_handle().abort();
        }
    }
}

async fn load_api_resources(
    cluster_key: i32,
    client: kube::Client,
    event_output: WorkerResultSender,
) {
    match (KubernetesApiInspector { client }).inspect_api().await {
        Err(error) => event_output
            .send(WorkerResult::KubernetesApisLoadFailed {
                cluster_key,
                error: format!("{error:#?}"),
            })
            .await
            .log_if_error("Failed to send error from inspecting resource api"),
        Ok(inspection) => {
            event_output
                .send(WorkerResult::KubernetesApisLoaded {
                    cluster_key,
                    api_resources: inspection.api_resources,
                    scalable_api_resources: inspection.scalable_api_resources,
                })
                .await
                .log_if_error("Failed to send kubernetes API resources");
            event_output
                .send(WorkerResult::KubernetesCustomResourceColumnsLoaded {
                    cluster_key,
                    columns: inspection.custom_resource_columns,
                })
                .await
                .log_if_error("Failed to send custom resource columns");
            event_output
                .send(WorkerResult::KubernetesResourceSchemasLoaded {
                    cluster_key,
                    schemas: inspection.resource_schemas,
                })
                .await
                .log_if_error("Failed to send custom resource schemas");
        }
    }
}
