//! Managed Kubernetes discovery backed by locally installed provider CLIs.

use super::kubeconfig_context_references;
use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;
use tokio::process::Command;

const TAILSCALE_K8S_OPERATOR_TAG: &str = "tag:k8s-operator";
const MAX_STDERR_LENGTH: usize = 2_000;
const MAX_AKS_DISCOVERY_CONCURRENCY: usize = 8;
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default)]
pub struct ClusterDiscoveryTools {
    pub azure_cli: bool,
    pub tailscale: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ClusterDiscovery {
    pub tools: ClusterDiscoveryTools,
    pub aks_clusters: Vec<AvailableAksCluster>,
    pub tailscale_clusters: Vec<AvailableTailscaleCluster>,
    pub azure_error: Option<String>,
    pub azure_warning: Option<String>,
    pub tailscale_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableAksCluster {
    pub name: String,
    pub location: String,
    pub resource_group: String,
    pub tags: BTreeMap<String, String>,
    pub subscription_id: String,
    pub subscription_name: String,
    pub tenant_name: String,
    pub tenant_default_domain: String,
    pub configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableTailscaleCluster {
    pub host_name: String,
    pub dns_name: String,
    pub online: bool,
    pub configured: bool,
}

#[derive(Debug)]
struct CliOutput {
    stdout: Vec<u8>,
    stderr: String,
    success: bool,
}

#[async_trait::async_trait]
trait CliRunner: Send + Sync {
    async fn run(&self, program: &str, args: &[String]) -> Result<CliOutput>;
}

struct SystemCliRunner;

#[async_trait::async_trait]
impl CliRunner for SystemCliRunner {
    async fn run(&self, program: &str, args: &[String]) -> Result<CliOutput> {
        let output = tokio::time::timeout(CLI_TIMEOUT, Command::new(program).args(args).output())
            .await
            .with_context(|| {
                format!(
                    "{program} timed out after {} seconds",
                    CLI_TIMEOUT.as_secs()
                )
            })?
            .with_context(|| format!("Could not start {program}"))?;
        Ok(CliOutput {
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
        })
    }
}

pub async fn discover_managed_clusters() -> Result<ClusterDiscovery> {
    discover_with(&SystemCliRunner).await
}

pub async fn add_aks_cluster(
    subscription_id: &str,
    resource_group: &str,
    cluster_name: &str,
) -> Result<()> {
    let runner = SystemCliRunner;
    run_success(
        &runner,
        "az",
        &[
            "aks",
            "get-credentials",
            "--subscription",
            subscription_id,
            "--resource-group",
            resource_group,
            "--name",
            cluster_name,
        ],
    )
    .await?;
    Ok(())
}

pub async fn add_tailscale_cluster(host_name: &str) -> Result<()> {
    let runner = SystemCliRunner;
    run_success(
        &runner,
        "tailscale",
        &["configure", "kubeconfig", host_name],
    )
    .await?;
    Ok(())
}

async fn discover_with(runner: &impl CliRunner) -> Result<ClusterDiscovery> {
    let tools = ClusterDiscoveryTools {
        azure_cli: installed(runner, "az").await,
        tailscale: installed(runner, "tailscale").await,
    };
    let configured_names = kubeconfig_context_references().unwrap_or_default();
    let (aks_clusters, azure_error, azure_warning) = if tools.azure_cli {
        match discover_aks(runner, &configured_names).await {
            Ok(discovery) => (discovery.clusters, None, discovery.warning),
            Err(error) => (Vec::new(), Some(format!("{error:#}")), None),
        }
    } else {
        (Vec::new(), None, None)
    };
    let (tailscale_clusters, tailscale_error) = if tools.tailscale {
        match discover_tailscale(runner, &configured_names).await {
            Ok(clusters) => (clusters, None),
            Err(error) => (Vec::new(), Some(format!("{error:#}"))),
        }
    } else {
        (Vec::new(), None)
    };
    Ok(ClusterDiscovery {
        tools,
        aks_clusters,
        tailscale_clusters,
        azure_error,
        azure_warning,
        tailscale_error,
    })
}

async fn installed(runner: &impl CliRunner, program: &str) -> bool {
    runner
        .run(program, &["--version".to_owned()])
        .await
        .is_ok_and(|output| output.success)
}

async fn discover_aks(
    runner: &impl CliRunner,
    configured_names: &[String],
) -> Result<AzureDiscovery> {
    let subscriptions: Vec<AzureSubscription> =
        json_output(runner, "az", &["account", "list", "--output", "json"]).await?;
    let (tenants, tenant_metadata_unavailable): (Vec<AzureTenant>, bool) = match json_output(
        runner,
        "az",
        &[
            "rest",
            "--method",
            "get",
            "--url",
            "/tenants?api-version=2020-01-01",
            "--query",
            "value",
            "--output",
            "json",
        ],
    )
    .await
    {
        Ok(tenants) => (tenants, false),
        Err(_) => (Vec::new(), true),
    };
    let tenants = tenants
        .into_iter()
        .map(|tenant| (tenant.tenant_id.clone(), tenant))
        .collect::<HashMap<_, _>>();
    let cluster_tasks = subscriptions
        .clone()
        .into_iter()
        .map(|subscription| async move {
            let clusters = json_output::<Vec<AzureAksCluster>>(
                runner,
                "az",
                &[
                    "aks",
                    "list",
                    "--subscription",
                    subscription.id.as_str(),
                    "--output",
                    "json",
                ],
            )
            .await;
            (subscription, clusters)
        });
    let mut output = Vec::new();
    let mut inspected_subscriptions = 0;
    let mut failed_subscriptions = 0;
    let mut tenant_metadata_missing = false;
    let subscription_results = futures_util::stream::iter(cluster_tasks)
        .buffer_unordered(MAX_AKS_DISCOVERY_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    for (subscription, clusters) in subscription_results {
        let clusters = match clusters {
            Ok(clusters) => {
                inspected_subscriptions += 1;
                clusters
            }
            Err(_) => {
                failed_subscriptions += 1;
                continue;
            }
        };
        let tenant = tenants.get(&subscription.tenant_id);
        tenant_metadata_missing |= tenant.is_none();
        output.extend(clusters.into_iter().map(|cluster| {
            AvailableAksCluster {
                configured: aks_cluster_is_configured(configured_names, &cluster),
                name: cluster.name,
                location: cluster.location,
                resource_group: cluster.resource_group,
                tags: cluster.tags.unwrap_or_default(),
                subscription_id: subscription.id.clone(),
                subscription_name: subscription.name.clone(),
                tenant_name: tenant
                    .map(|tenant| tenant.display_name.clone())
                    .unwrap_or_else(|| subscription.tenant_id.clone()),
                tenant_default_domain: tenant
                    .map(|tenant| tenant.default_domain.clone())
                    .unwrap_or_default(),
            }
        }));
    }
    output.sort_by(|left, right| {
        (&left.tenant_name, &left.subscription_name, &left.name).cmp(&(
            &right.tenant_name,
            &right.subscription_name,
            &right.name,
        ))
    });
    if inspected_subscriptions == 0 && !subscriptions.is_empty() {
        bail!("Could not inspect any Azure subscriptions. Refresh discovery to retry.");
    }
    let mut warning_parts = Vec::new();
    if failed_subscriptions > 0 {
        warning_parts.push(format!(
            "Could not inspect {failed_subscriptions} subscription{}",
            if failed_subscriptions == 1 { "" } else { "s" }
        ));
    }
    if tenant_metadata_unavailable || tenant_metadata_missing {
        warning_parts.push("Azure tenant metadata is unavailable".to_owned());
    }
    let warning = (!warning_parts.is_empty())
        .then(|| format!("{}. Refresh discovery to retry.", warning_parts.join(". ")));
    Ok(AzureDiscovery {
        clusters: output,
        warning,
    })
}

fn aks_cluster_is_configured(configured_names: &[String], cluster: &AzureAksCluster) -> bool {
    let generated_contexts = [
        format!("clusterUser_{}_{}", cluster.resource_group, cluster.name),
        format!("clusterAdmin_{}_{}", cluster.resource_group, cluster.name),
    ];
    configured_names.iter().any(|name| {
        name == &cluster.name || generated_contexts.iter().any(|context| name == context)
    })
}

struct AzureDiscovery {
    clusters: Vec<AvailableAksCluster>,
    warning: Option<String>,
}

async fn discover_tailscale(
    runner: &impl CliRunner,
    configured_names: &[String],
) -> Result<Vec<AvailableTailscaleCluster>> {
    let status: TailscaleStatus = json_output(runner, "tailscale", &["status", "--json"]).await?;
    let mut output = status
        .peer
        .into_values()
        .filter(|peer| peer.tags.contains(TAILSCALE_K8S_OPERATOR_TAG))
        .map(|peer| {
            let dns_name = peer.dns_name.trim_end_matches('.').to_owned();
            AvailableTailscaleCluster {
                configured: configured_names
                    .iter()
                    .any(|name| name == &peer.host_name || name == &dns_name),
                host_name: peer.host_name,
                dns_name,
                online: peer.online,
            }
        })
        .collect::<Vec<_>>();
    output.sort_by(|left, right| left.host_name.cmp(&right.host_name));
    Ok(output)
}

async fn json_output<T: for<'de> Deserialize<'de>>(
    runner: &impl CliRunner,
    program: &str,
    args: &[&str],
) -> Result<T> {
    let output = run_success(runner, program, args).await?;
    serde_yaml::from_slice(&output.stdout)
        .with_context(|| format!("Could not parse JSON returned by {program}"))
}

async fn run_success(runner: &impl CliRunner, program: &str, args: &[&str]) -> Result<CliOutput> {
    let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let output = runner.run(program, &args).await?;
    if !output.success {
        let stderr = truncate_text(output.stderr.trim(), MAX_STDERR_LENGTH);
        bail!(
            "{program} {} failed{}",
            args.join(" "),
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }
    Ok(output)
}

fn truncate_text(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzureSubscription {
    id: String,
    name: String,
    tenant_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzureTenant {
    tenant_id: String,
    display_name: String,
    default_domain: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzureAksCluster {
    name: String,
    location: String,
    resource_group: String,
    #[serde(default)]
    tags: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct TailscaleStatus {
    peer: HashMap<String, TailscalePeer>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct TailscalePeer {
    host_name: String,
    #[serde(rename = "DNSName")]
    dns_name: String,
    online: bool,
    tags: HashSet<String>,
}

#[cfg(test)]
mod tests;
