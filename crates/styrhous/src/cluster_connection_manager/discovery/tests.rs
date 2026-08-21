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
