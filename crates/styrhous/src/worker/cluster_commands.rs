use super::*;
use crate::cluster_connection_manager::kubeconfig::{
    KUBECONFIG_WRITE_LOCK, save_context_presentation,
};

#[async_trait]
impl WorkerCommand for SaveContextPresentation {
    type Output = Result<ContextPresentationSaved, ContextPresentationSaveFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        let _guard = KUBECONFIG_WRITE_LOCK.lock().await;
        let result = self
            .presentation
            .normalized()
            .map_err(anyhow::Error::msg)
            .and_then(|presentation| {
                save_context_presentation(&self.context_name, &presentation)?;
                Ok(presentation)
            });
        match result {
            Ok(presentation) => Ok(ContextPresentationSaved {
                request_id: self.request_id,
                context_name: self.context_name,
                presentation,
            }),
            Err(error) => Err(ContextPresentationSaveFailed {
                request_id: self.request_id,
                context_name: self.context_name,
                error: format!("{error:#}"),
            }),
        }
    }
}

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
impl WorkerCommand for LoadAwsAccounts {
    type Output = AwsAccountsUpdated;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        let discovery = discover_aws_accounts().await;
        AwsAccountsUpdated {
            request_id: self.request_id,
            cli_installed: discovery.cli_installed,
            accounts: discovery.accounts,
            warning: discovery.warning,
            error: discovery.error,
        }
    }
}

#[async_trait]
impl WorkerCommand for ScanEksClusters {
    type Output = Result<EksClustersUpdated, EksDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        let discovery = discover_eks_clusters(&self.account_id, &self.profile)
            .await
            .map_err(|error| EksDiscoveryFailed {
                request_id: self.request_id,
                error: format!("{error:#}"),
            })?;
        Ok(EksClustersUpdated {
            request_id: self.request_id,
            account_id: discovery.account_id,
            profile: discovery.profile,
            clusters: discovery.clusters,
            warning: discovery.warning,
        })
    }
}

#[async_trait]
impl WorkerCommand for AddAksCluster {
    type Output = Result<ManagedClusterImported, ManagedClusterDiscoveryFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        let _guard = KUBECONFIG_WRITE_LOCK.lock().await;
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
        let _guard = KUBECONFIG_WRITE_LOCK.lock().await;
        add_tailscale_cluster(&self.host_name).await?;
        Ok(ManagedClusterImported)
    }
}

#[async_trait]
impl WorkerCommand for AddEksCluster {
    type Output = Result<ManagedClusterImported, EksImportFailed>;

    async fn execute(self, _state: &WorkerState) -> Self::Output {
        let _guard = KUBECONFIG_WRITE_LOCK.lock().await;
        add_eks_cluster(&self.account_id, &self.region, &self.name, &self.profile)
            .await
            .map_err(|error| EksImportFailed {
                request_id: self.request_id,
                error: format!("{error:#}"),
            })?;
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
