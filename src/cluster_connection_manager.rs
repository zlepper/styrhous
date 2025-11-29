use crate::worker::WorkerResult;
use anyhow::Context;
use kube::config::Kubeconfig;

#[derive(Debug, Clone)]
pub struct Cluster {
    pub name: String,
    pub cluster: Option<String>,
}
/*
#[tauri::command]
pub async fn get_clusters(state: State<'_, Clusters>) -> Result<Vec<Cluster>, ()> {
    let clusters = state.state.lock().await;
    Ok(clusters.clone())
}

#[tauri::command]
pub async fn get_cluster_tools() -> Result<InstalledTools, ()> {
    let tailscale = has_tailscale_installed().await;
    let azure_cli = azure_cli_helper::has_azure_cli_installed().await;

    Ok(InstalledTools { tailscale, azure_cli })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledTools {
    pub tailscale: bool,
    pub azure_cli: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableTailscaleCluster {
    pub name: String,
    pub online: bool,
    pub in_kube_config: bool,
}

#[tauri::command]
pub async fn get_available_tailscale_clusters(
    state: State<'_, Clusters>,
) -> Result<Vec<AvailableTailscaleCluster>, String> {
    let hosts = tailscale_helper::get_possible_clusters()
        .await
        .error_to_string()?;

    let current_clusters = state.state.lock().await;

    let hosts = hosts
        .iter()
        .map(|h| AvailableTailscaleCluster {
            name: h.host_name.clone(),
            online: h.online,
            in_kube_config: current_clusters.iter().any(|c| {
                c.name == h.host_name
                    || c.name == h.dns_name
                    || c.cluster.as_ref() == Some(&h.host_name)
                    || c.cluster.as_ref() == Some(&h.dns_name)
            }),
        })
        .collect();

    Ok(hosts)
}

#[tauri::command]
pub async fn add_tailscale_cluster<R: Runtime>(
    tailscale_hostname: String,
    app: AppHandle<R>,
) -> Result<(), String> {
    tailscale_helper::add_tailscale_cluster(tailscale_hostname)
        .await
        .error_to_string()?;

    reload_kubeconfig(app).await?;

    Ok(())
}


#[tauri::command]
pub async fn get_available_aks_clusters(state: State<'_, Clusters>) -> Result<Vec<AvailableAksCluster>, String> {
    let mut aks_clusters = azure_cli_helper::get_available_aks_clusters().await.error_to_string()?;

    let kube_clusters = state.state.lock().await;

    for cluster in aks_clusters.iter_mut() {
        cluster.in_kube_config = kube_clusters.iter().any(|c| c.name == cluster.name);
    }

    Ok(aks_clusters)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddAksClusterParams {
    pub subscription_id: String,
    pub resource_group: String,
    pub cluster_name: String,
}

#[tauri::command]
pub async fn add_aks_cluster<R: Runtime>(args: AddAksClusterParams, app: AppHandle<R>) -> Result<(), String> {
    azure_cli_helper::add_aks_cluster(&args.subscription_id, &args.resource_group, &args.cluster_name)
        .await
        .error_to_string()?;

    reload_kubeconfig(app).await?;

    Ok(())
}
*/

pub async fn reload_kubeconfig() -> anyhow::Result<WorkerResult> {
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
