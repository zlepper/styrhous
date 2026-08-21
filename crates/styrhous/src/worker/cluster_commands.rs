use super::*;

#[async_trait]
impl WorkerCommand for LoadClusters {
    type Output = Result<KubernetesClustersUpdated, WorkerError>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        state.stop_all_clusters().await;
        reload_kubeconfig()
            .await
            .map_err(|error| WorkerError { error })
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
    }
}

#[async_trait]
impl WorkerCommand for LoadImportedClusters {
    type Output = Result<ImportedKubernetesClusters, ManagedClusterDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        let KubernetesClustersUpdated(clusters) =
            reload_kubeconfig()
                .await
                .map_err(|error| ManagedClusterDiscoveryFailed {
                    error: format!(
                        "Could not reload kubeconfig after importing the cluster: {error:#}"
                    ),
                })?;
        Ok(ImportedKubernetesClusters(clusters))
    }
}

#[async_trait]
impl WorkerCommand for LoadManagedClusterDiscovery {
    type Output = Result<ManagedClusterDiscoveryUpdated, ManagedClusterDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        Ok(discover_managed_clusters().await?.into())
    }
}

#[async_trait]
impl WorkerCommand for AddAksCluster {
    type Output = Result<ManagedClusterImported, ManagedClusterDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        add_aks_cluster(
            &self.subscription_id,
            &self.resource_group,
            &self.cluster_name,
        )
        .await?;
        Ok(ManagedClusterImported)
    }
}

#[async_trait]
impl WorkerCommand for AddTailscaleCluster {
    type Output = Result<ManagedClusterImported, ManagedClusterDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        add_tailscale_cluster(&self.host_name).await?;
        Ok(ManagedClusterImported)
    }
}

impl From<crate::cluster_connection_manager::ClusterDiscovery> for ManagedClusterDiscoveryUpdated {
    fn from(discovery: crate::cluster_connection_manager::ClusterDiscovery) -> Self {
        Self {
            tools: discovery.tools,
            aks_clusters: discovery.aks_clusters,
            tailscale_clusters: discovery.tailscale_clusters,
            azure_error: discovery.azure_error,
            azure_warning: discovery.azure_warning,
            tailscale_error: discovery.tailscale_error,
        }
    }
}

impl From<anyhow::Error> for ManagedClusterDiscoveryFailed {
    fn from(error: anyhow::Error) -> Self {
        Self {
            error: format!("{error:#}"),
        }
    }
}

#[async_trait]
impl WorkerCommand for ConnectToCluster {
    type Output = Result<KubernetesClusterConnectionCreated, ClusterConnectionFailed>;

    async fn execute(self, state: &WorkerState) -> Self::Output {
        let cluster_key = self.cluster_key;
        state.stop_cluster(cluster_key).await;
        let result =
            start_cluster_connection(cluster_key, &self.cluster, state.results.clone()).await;
        match result {
            Ok(connection) => {
                state
                    .connections
                    .lock()
                    .await
                    .insert(cluster_key, connection);
                Ok(KubernetesClusterConnectionCreated { cluster_key })
            }
            Err(error) => Err(ClusterConnectionFailed {
                cluster_key,
                error: format!("{error:#?}"),
            }),
        }
    }

    fn serializes_session_lifecycle(&self) -> bool {
        true
    }

    fn cluster_key(&self) -> Option<i32> {
        Some(self.cluster_key)
    }
}
