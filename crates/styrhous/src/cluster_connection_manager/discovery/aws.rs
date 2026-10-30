use super::*;
use std::path::{Path, PathBuf};

const MAX_AWS_CONCURRENCY: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AwsProfile {
    Current,
    Named(String),
}

impl AwsProfile {
    pub fn label(&self) -> &str {
        match self {
            Self::Current => "Current CLI identity",
            Self::Named(name) => name,
        }
    }

    fn append_cli_option(&self, args: &mut Vec<String>) {
        if let Self::Named(name) = self {
            args.extend(["--profile".to_owned(), name.clone()]);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsAccount {
    pub id: String,
    pub partition: String,
    pub profiles: Vec<AwsProfile>,
}

#[derive(Debug, Clone, Default)]
pub struct AwsAccountDiscovery {
    pub cli_installed: bool,
    pub accounts: Vec<AwsAccount>,
    pub warning: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableEksCluster {
    pub name: String,
    pub region: String,
    pub account_id: String,
    pub partition: String,
    pub profile: AwsProfile,
    pub tags: BTreeMap<String, String>,
    pub configured: bool,
}

impl AvailableEksCluster {
    pub fn arn(&self) -> String {
        eks_cluster_arn(&self.partition, &self.region, &self.account_id, &self.name)
    }
}

#[derive(Debug, Clone)]
pub struct EksDiscovery {
    pub account_id: String,
    pub profile: AwsProfile,
    pub clusters: Vec<AvailableEksCluster>,
    pub warning: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsCallerIdentity {
    account: String,
    arn: String,
}

impl AwsCallerIdentity {
    fn partition(&self) -> Result<&str> {
        self.arn
            .split(':')
            .nth(1)
            .filter(|partition| !partition.is_empty())
            .context("AWS returned an invalid caller ARN")
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsRegions {
    regions: Vec<AwsRegion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AwsRegion {
    region_name: String,
}

#[derive(Debug, Deserialize)]
struct EksClusters {
    clusters: Vec<String>,
}

pub async fn discover_aws_accounts() -> AwsAccountDiscovery {
    discover_accounts_with(&SystemCliRunner).await
}

async fn discover_accounts_with(runner: &impl CliRunner) -> AwsAccountDiscovery {
    if !installed(runner, "aws").await {
        return AwsAccountDiscovery::default();
    }

    let mut discovery = AwsAccountDiscovery {
        cli_installed: true,
        ..Default::default()
    };
    let mut warnings = Vec::new();
    let mut profiles = vec![AwsProfile::Current];
    match run_success(runner, "aws", &["configure", "list-profiles"]).await {
        Ok(output) => {
            profiles.extend(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(|name| AwsProfile::Named(name.to_owned())),
            );
        }
        Err(_) => warnings.push("Could not list AWS CLI profiles".to_owned()),
    }
    profiles.sort();
    profiles.dedup();

    let requests = profiles.into_iter().map(|profile| async move {
        let result = caller_identity(runner, &profile).await;
        (profile, result)
    });
    let identities = futures_util::stream::iter(requests)
        .buffer_unordered(MAX_AWS_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    let mut accounts = BTreeMap::<(String, String), Vec<(AwsProfile, String)>>::new();
    let mut failed = Vec::new();
    for (profile, identity) in identities {
        match identity {
            Ok(identity) => match identity.partition() {
                Ok(partition) => accounts
                    .entry((identity.account.clone(), partition.to_owned()))
                    .or_default()
                    .push((profile, identity.arn)),
                Err(_) => failed.push(profile.label().to_owned()),
            },
            Err(_) => failed.push(profile.label().to_owned()),
        }
    }
    discovery.accounts = accounts
        .into_iter()
        .map(|((id, partition), mut identities)| {
            identities.sort_by(|left, right| left.0.cmp(&right.0));
            // The active CLI identity often resolves to a named profile. Show
            // it separately only when it is genuinely another identity.
            if let Some(current_arn) = identities
                .iter()
                .find(|(profile, _)| matches!(profile, AwsProfile::Current))
                .map(|(_, arn)| arn.clone())
                && identities.iter().any(|(profile, arn)| {
                    matches!(profile, AwsProfile::Named(_)) && arn == &current_arn
                })
            {
                identities.retain(|(profile, _)| !matches!(profile, AwsProfile::Current));
            }
            AwsAccount {
                id,
                partition,
                profiles: identities.into_iter().map(|(profile, _)| profile).collect(),
            }
        })
        .collect();
    if !failed.is_empty() {
        failed.sort();
        warnings.push(format!(
            "Could not resolve AWS identity for {}",
            failed.join(", ")
        ));
    }
    discovery.warning =
        (!warnings.is_empty()).then(|| format!("{}. Sign in and refresh.", warnings.join(". ")));
    if discovery.accounts.is_empty() {
        discovery.error = Some("No accessible AWS CLI identities were found. Configure a profile or sign in, then refresh.".to_owned());
    }
    discovery
}

async fn caller_identity(
    runner: &impl CliRunner,
    profile: &AwsProfile,
) -> Result<AwsCallerIdentity> {
    aws_json(runner, profile, &["sts", "get-caller-identity"]).await
}

pub async fn discover_eks_clusters(account_id: &str, profile: &AwsProfile) -> Result<EksDiscovery> {
    discover_eks_with(&SystemCliRunner, account_id, profile).await
}

async fn discover_eks_with(
    runner: &impl CliRunner,
    expected_account_id: &str,
    profile: &AwsProfile,
) -> Result<EksDiscovery> {
    let configured = kubeconfig_context_references()
        .unwrap_or_default()
        .into_iter()
        .collect();
    discover_eks_with_configured(runner, expected_account_id, profile, configured).await
}

async fn discover_eks_with_configured(
    runner: &impl CliRunner,
    expected_account_id: &str,
    profile: &AwsProfile,
    configured: std::collections::HashSet<String>,
) -> Result<EksDiscovery> {
    let identity = caller_identity(runner, profile).await?;
    if identity.account != expected_account_id {
        bail!(
            "The selected AWS profile now resolves to a different account. Refresh accounts before scanning."
        );
    }
    let partition = identity.partition()?.to_owned();
    let configured_region = configured_region(runner, profile).await;
    let bootstrap_region = match partition.as_str() {
        "aws" => "us-east-1",
        "aws-us-gov" => "us-gov-west-1",
        "aws-cn" => "cn-north-1",
        _ => configured_region.as_deref().context(
            "Set a Region in this AWS profile to discover enabled Regions in this partition",
        )?,
    };
    let mut warning_parts = Vec::new();
    let regions: Vec<String> = match aws_json::<AwsRegions>(
        runner,
        profile,
        &["ec2", "describe-regions", "--region", bootstrap_region],
    )
    .await
    {
        Ok(regions) => regions
            .regions
            .into_iter()
            .map(|region| region.region_name)
            .collect(),
        Err(_) => {
            let region = configured_region.context(
                "Could not list enabled AWS Regions, and the selected profile has no configured Region",
            )?;
            warning_parts.push(format!(
                "Could not list enabled AWS Regions. Scanned only {region}; results may be incomplete"
            ));
            vec![region]
        }
    };
    let mut regions = regions;
    regions.sort();
    regions.dedup();

    let requests = regions.iter().cloned().map(|region| async move {
        let result = aws_json::<EksClusters>(
            runner,
            profile,
            &["eks", "list-clusters", "--region", &region],
        )
        .await;
        (region, result)
    });
    let results = futures_util::stream::iter(requests)
        .buffer_unordered(MAX_AWS_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    let mut clusters = Vec::new();
    let mut failed_regions = Vec::new();
    for (region, result) in results {
        match result {
            Ok(result) => clusters.extend(result.clusters.into_iter().map(|name| {
                let arn = eks_cluster_arn(&partition, &region, expected_account_id, &name);
                AvailableEksCluster {
                    configured: configured.contains(&arn),
                    name,
                    region: region.clone(),
                    account_id: expected_account_id.to_owned(),
                    partition: partition.clone(),
                    profile: profile.clone(),
                    tags: BTreeMap::new(),
                }
            })),
            Err(_) => failed_regions.push(region),
        }
    }
    if !regions.is_empty() && failed_regions.len() == regions.len() {
        bail!("Could not inspect any AWS Regions for EKS clusters. Check permissions and refresh.");
    }
    clusters.sort_by(|left, right| (&left.region, &left.name).cmp(&(&right.region, &right.name)));
    clusters.dedup_by(|left, right| left.region == right.region && left.name == right.name);
    let tag_targets = clusters
        .iter()
        .enumerate()
        .map(|(index, cluster)| (index, cluster.name.clone(), cluster.region.clone()))
        .collect::<Vec<_>>();
    let tag_requests = tag_targets
        .into_iter()
        .map(|(index, name, region)| async move {
            let result = aws_json::<Option<BTreeMap<String, String>>>(
                runner,
                profile,
                &[
                    "eks",
                    "describe-cluster",
                    "--name",
                    &name,
                    "--region",
                    &region,
                    "--query",
                    "cluster.tags",
                ],
            )
            .await;
            (index, result)
        });
    let tag_results = futures_util::stream::iter(tag_requests)
        .buffer_unordered(MAX_AWS_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    let mut failed_tag_reads = 0;
    for (index, result) in tag_results {
        match result {
            Ok(tags) => clusters[index].tags = tags.unwrap_or_default(),
            Err(_) => failed_tag_reads += 1,
        }
    }
    if !failed_regions.is_empty() {
        failed_regions.sort();
        warning_parts.push(format!(
            "Could not inspect EKS in {}. Refresh to retry",
            failed_regions.join(", ")
        ));
    }
    if failed_tag_reads > 0 {
        warning_parts.push(format!(
            "Could not read tags for {failed_tag_reads} EKS cluster{}; some tags may be missing",
            if failed_tag_reads == 1 { "" } else { "s" }
        ));
    }
    Ok(EksDiscovery {
        account_id: expected_account_id.to_owned(),
        profile: profile.clone(),
        clusters,
        warning: (!warning_parts.is_empty()).then(|| format!("{}.", warning_parts.join(". "))),
    })
}

async fn configured_region(runner: &impl CliRunner, profile: &AwsProfile) -> Option<String> {
    for variable in ["AWS_REGION", "AWS_DEFAULT_REGION"] {
        if let Some(region) = std::env::var(variable)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
        {
            return Some(region);
        }
    }
    let mut args = vec![
        "configure".to_owned(),
        "get".to_owned(),
        "region".to_owned(),
    ];
    profile.append_cli_option(&mut args);
    let output = runner.run("aws", &args).await.ok()?;
    if !output.success {
        return None;
    }
    let region = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!region.is_empty()).then_some(region)
}

async fn aws_json<T: for<'de> Deserialize<'de>>(
    runner: &impl CliRunner,
    profile: &AwsProfile,
    args: &[&str],
) -> Result<T> {
    let mut args = args
        .iter()
        .map(|part| (*part).to_owned())
        .collect::<Vec<_>>();
    profile.append_cli_option(&mut args);
    args.extend(["--output".to_owned(), "json".to_owned()]);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = run_success(runner, "aws", &refs).await?;
    serde_json::from_slice(&output.stdout).context("Could not parse JSON returned by AWS CLI")
}

fn eks_cluster_arn(partition: &str, region: &str, account_id: &str, name: &str) -> String {
    format!("arn:{partition}:eks:{region}:{account_id}:cluster/{name}")
}

pub async fn add_eks_cluster(
    account_id: &str,
    region: &str,
    name: &str,
    profile: &AwsProfile,
) -> Result<()> {
    let path = writable_kubeconfig_path()?;
    add_eks_with(&SystemCliRunner, account_id, region, name, profile, &path).await
}

async fn add_eks_with(
    runner: &impl CliRunner,
    account_id: &str,
    region: &str,
    name: &str,
    profile: &AwsProfile,
    path: &Path,
) -> Result<()> {
    let identity = caller_identity(runner, profile).await?;
    if identity.account != account_id {
        bail!(
            "The selected AWS profile now resolves to a different account. Refresh accounts before importing."
        );
    }
    let arn = eks_cluster_arn(identity.partition()?, region, account_id, name);
    let mut args = vec![
        "eks",
        "update-kubeconfig",
        "--name",
        name,
        "--region",
        region,
        "--dry-run",
    ];
    let path_string = path.to_string_lossy().into_owned();
    args.extend(["--kubeconfig", &path_string]);
    let mut owned_args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    profile.append_cli_option(&mut owned_args);
    owned_args.extend(["--output".to_owned(), "json".to_owned()]);
    let refs = owned_args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = run_success(runner, "aws", &refs).await?;
    let yaml =
        std::str::from_utf8(&output.stdout).context("AWS CLI returned invalid kubeconfig text")?;
    let yaml = if let AwsProfile::Named(name) = profile {
        pin_profile_in_kubeconfig(yaml, &arn, name)?
    } else {
        yaml.to_owned()
    };
    write_kubeconfig_atomically(path, yaml.as_bytes())
}

fn pin_profile_in_kubeconfig(yaml: &str, arn: &str, profile: &str) -> Result<String> {
    let mut config: serde_yaml::Value =
        serde_yaml::from_str(yaml).context("Could not parse generated kubeconfig")?;
    let users = config
        .get_mut("users")
        .and_then(serde_yaml::Value::as_sequence_mut)
        .context("Generated kubeconfig has no users")?;
    let user = users
        .iter_mut()
        .find(|user| user.get("name").and_then(serde_yaml::Value::as_str) == Some(arn))
        .context("Generated kubeconfig has no EKS user")?;
    let exec = user
        .get_mut("user")
        .and_then(|user| user.get_mut("exec"))
        .context("Generated EKS user has no exec authentication")?;
    if exec.get("command").and_then(serde_yaml::Value::as_str) != Some("aws") {
        bail!("Generated EKS authentication does not call AWS CLI");
    }
    let args = exec
        .get_mut("args")
        .and_then(serde_yaml::Value::as_sequence_mut)
        .context("Generated EKS authentication has no arguments")?;
    args.splice(
        0..0,
        [
            serde_yaml::Value::String("--profile".to_owned()),
            serde_yaml::Value::String(profile.to_owned()),
        ],
    );
    serde_yaml::to_string(&config).context("Could not serialize generated kubeconfig")
}

fn writable_kubeconfig_path() -> Result<PathBuf> {
    let path = super::super::kubeconfig::kubeconfig_paths()?
        .into_iter()
        .next()
        .context("Could not find a writable kubeconfig path")?;
    super::super::kubeconfig::resolve_kubeconfig_path(&path)
}

use super::super::kubeconfig::write_kubeconfig_atomically;

#[cfg(test)]
mod tests;
