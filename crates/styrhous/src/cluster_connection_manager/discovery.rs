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
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use tokio::sync::Mutex;

    struct FakeRunner(Mutex<VecDeque<Result<CliOutput>>>);

    #[async_trait::async_trait]
    impl CliRunner for FakeRunner {
        async fn run(&self, _program: &str, _args: &[String]) -> Result<CliOutput> {
            self.0.lock().await.pop_front().expect("unexpected command")
        }
    }

    fn successful_json(json: &str) -> Result<CliOutput> {
        Ok(CliOutput {
            stdout: json.as_bytes().to_vec(),
            stderr: String::new(),
            success: true,
        })
    }

    #[tokio::test]
    async fn tailscale_discovery_filters_tag_trims_dns_and_sorts() {
        let runner = FakeRunner(Mutex::new(VecDeque::from([successful_json(
            r#"{"Peer":{"b":{"HostName":"beta","DNSName":"beta.tailnet.ts.net.","Online":false,"Tags":["tag:k8s-operator"]},"a":{"HostName":"alpha","DNSName":"alpha.tailnet.ts.net.","Online":true,"Tags":["tag:k8s-operator"]},"ignored":{"HostName":"ignored","Tags":[]}}}"#,
        )])));
        let clusters = discover_tailscale(&runner, &["alpha.tailnet.ts.net".into()])
            .await
            .unwrap();
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].host_name, "alpha");
        assert!(clusters[0].configured);
        assert!(!clusters[1].online);
    }

    #[tokio::test]
    async fn non_zero_cli_status_surfaces_stderr() {
        let runner = FakeRunner(Mutex::new(VecDeque::from([Ok(CliOutput {
            stdout: vec![],
            stderr: "not signed in".into(),
            success: false,
        })])));
        let error = run_success(&runner, "az", &["account", "list"])
            .await
            .unwrap_err();
        assert!(error.to_string().contains("not signed in"));
    }

    #[tokio::test]
    async fn azure_discovery_keeps_accessible_subscriptions_when_another_is_denied() {
        struct AzureRunner;
        #[async_trait::async_trait]
        impl CliRunner for AzureRunner {
            async fn run(&self, _program: &str, args: &[String]) -> Result<CliOutput> {
                let output = match args
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    ["account", "list", "--output", "json"] => successful_json(
                        r#"[{"id":"available-id","name":"Available","tenantId":"development-tenant"},{"id":"blocked-id","name":"Production","tenantId":"production-tenant"}]"#,
                    ),
                    ["rest", ..] => successful_json(
                        r#"[{"tenantId":"development-tenant","displayName":"Acme Development","defaultDomain":"development.example"},{"tenantId":"production-tenant","displayName":"Acme Production","defaultDomain":"production.example"}]"#,
                    ),
                    ["aks", "list", "--subscription", "available-id", ..] => successful_json(
                        r#"[{"name":"available-cluster","location":"westeurope","resourceGroup":"platform","tags":null}]"#,
                    ),
                    ["aks", "list", "--subscription", "blocked-id", ..] => Ok(CliOutput {
                        stdout: Vec::new(),
                        stderr: "PIM elevation required".into(),
                        success: false,
                    }),
                    unexpected => panic!("unexpected Azure command: {unexpected:?}"),
                }?;
                Ok(output)
            }
        }

        let discovery = discover_aks(&AzureRunner, &[]).await.unwrap();

        assert_eq!(discovery.clusters.len(), 1);
        assert_eq!(discovery.clusters[0].name, "available-cluster");
        assert_eq!(discovery.clusters[0].tenant_name, "Acme Development");
        assert_eq!(
            discovery.warning.as_deref(),
            Some("Could not inspect 1 subscription. Refresh discovery to retry.")
        );
    }

    #[tokio::test]
    async fn azure_discovery_associates_multiple_subscriptions_with_one_tenant() {
        struct AzureRunner;
        #[async_trait::async_trait]
        impl CliRunner for AzureRunner {
            async fn run(&self, _program: &str, args: &[String]) -> Result<CliOutput> {
                match args
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    ["account", "list", "--output", "json"] => successful_json(
                        r#"[{"id":"first-id","name":"First","tenantId":"tenant-id"},{"id":"second-id","name":"Second","tenantId":"tenant-id"}]"#,
                    ),
                    ["rest", ..] => successful_json(
                        r#"[{"tenantId":"tenant-id","displayName":"Shared tenant","defaultDomain":"shared.example"}]"#,
                    ),
                    ["aks", "list", "--subscription", "first-id", ..] => successful_json(
                        r#"[{"name":"first-cluster","location":"westeurope","resourceGroup":"platform"}]"#,
                    ),
                    ["aks", "list", "--subscription", "second-id", ..] => successful_json(
                        r#"[{"name":"second-cluster","location":"westeurope","resourceGroup":"platform"}]"#,
                    ),
                    unexpected => panic!("unexpected Azure command: {unexpected:?}"),
                }
            }
        }

        let discovery = discover_aks(&AzureRunner, &[]).await.unwrap();

        assert_eq!(discovery.clusters.len(), 2);
        assert!(
            discovery
                .clusters
                .iter()
                .all(|cluster| cluster.tenant_name == "Shared tenant")
        );
        assert_eq!(discovery.clusters[0].subscription_name, "First");
        assert_eq!(discovery.clusters[1].subscription_name, "Second");
    }

    #[test]
    fn generated_aks_context_name_marks_cluster_as_configured() {
        let cluster = AzureAksCluster {
            name: "payments".into(),
            location: "westeurope".into(),
            resource_group: "platform".into(),
            tags: None,
        };

        assert!(aks_cluster_is_configured(
            &["clusterUser_platform_payments".into()],
            &cluster
        ));
    }

    #[tokio::test]
    async fn tenant_metadata_failure_keeps_clusters_from_accessible_subscriptions() {
        struct AzureRunner;
        #[async_trait::async_trait]
        impl CliRunner for AzureRunner {
            async fn run(&self, _program: &str, args: &[String]) -> Result<CliOutput> {
                match args
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    ["account", "list", "--output", "json"] => successful_json(
                        r#"[{"id":"available-id","name":"Available","tenantId":"tenant-id"}]"#,
                    ),
                    ["rest", ..] => Ok(CliOutput {
                        stdout: Vec::new(),
                        stderr: "tenant listing denied".into(),
                        success: false,
                    }),
                    ["aks", "list", "--subscription", "available-id", ..] => successful_json(
                        r#"[{"name":"available-cluster","location":"westeurope","resourceGroup":"platform"}]"#,
                    ),
                    unexpected => panic!("unexpected Azure command: {unexpected:?}"),
                }
            }
        }

        let discovery = discover_aks(&AzureRunner, &[]).await.unwrap();

        assert_eq!(discovery.clusters.len(), 1);
        assert_eq!(discovery.clusters[0].tenant_name, "tenant-id");
        assert_eq!(
            discovery.warning.as_deref(),
            Some("Azure tenant metadata is unavailable. Refresh discovery to retry.")
        );
    }

    #[tokio::test]
    async fn non_zero_cli_status_truncates_unicode_stderr_without_panicking() {
        let runner = FakeRunner(Mutex::new(VecDeque::from([Ok(CliOutput {
            stdout: vec![],
            stderr: "é".repeat(MAX_STDERR_LENGTH),
            success: false,
        })])));

        assert!(
            run_success(&runner, "az", &["account", "list"])
                .await
                .is_err()
        );
    }
}
