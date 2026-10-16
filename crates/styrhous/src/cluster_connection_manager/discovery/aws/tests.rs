use super::*;

fn json_output(value: &str) -> Result<CliOutput> {
    Ok(CliOutput {
        stdout: value.as_bytes().to_vec(),
        stderr: String::new(),
        success: true,
    })
}

#[tokio::test]
async fn account_discovery_groups_profiles_and_keeps_accessible_accounts() {
    struct Runner;

    #[async_trait::async_trait]
    impl CliRunner for Runner {
        async fn run(&self, program: &str, args: &[String]) -> Result<CliOutput> {
            assert_eq!(program, "aws");
            let parts = args.iter().map(String::as_str).collect::<Vec<_>>();
            match parts.as_slice() {
                ["--version"] => json_output("aws-cli/2"),
                ["configure", "list-profiles"] => json_output("default\nproduction\nexpired\n"),
                ["sts", "get-caller-identity", "--output", "json"] => json_output(
                    r#"{"Account":"111111111111","Arn":"arn:aws:iam::111111111111:user/default"}"#,
                ),
                [
                    "sts",
                    "get-caller-identity",
                    "--profile",
                    "default",
                    "--output",
                    "json",
                ] => json_output(
                    r#"{"Account":"111111111111","Arn":"arn:aws:iam::111111111111:user/default"}"#,
                ),
                [
                    "sts",
                    "get-caller-identity",
                    "--profile",
                    "production",
                    "--output",
                    "json",
                ] => json_output(
                    r#"{"Account":"222222222222","Arn":"arn:aws:sts::222222222222:assumed-role/admin/session"}"#,
                ),
                [
                    "sts",
                    "get-caller-identity",
                    "--profile",
                    "expired",
                    "--output",
                    "json",
                ] => Ok(CliOutput {
                    stdout: Vec::new(),
                    stderr: "SSO session expired".to_owned(),
                    success: false,
                }),
                unexpected => panic!("unexpected AWS command: {unexpected:?}"),
            }
        }
    }

    let discovery = discover_accounts_with(&Runner).await;

    assert!(discovery.cli_installed);
    assert_eq!(discovery.accounts.len(), 2);
    assert_eq!(discovery.accounts[0].id, "111111111111");
    assert_eq!(
        discovery.accounts[0].profiles,
        vec![AwsProfile::Named("default".into())]
    );
    assert_eq!(
        discovery.accounts[1].profiles,
        vec![AwsProfile::Named("production".into())]
    );
    assert!(discovery.warning.as_deref().unwrap().contains("expired"));
}

#[tokio::test]
async fn eks_scan_uses_only_selected_profile_and_keeps_partial_region_results() {
    struct Runner;

    #[async_trait::async_trait]
    impl CliRunner for Runner {
        async fn run(&self, program: &str, args: &[String]) -> Result<CliOutput> {
            assert_eq!(program, "aws");
            let parts = args.iter().map(String::as_str).collect::<Vec<_>>();
            assert!(!parts.contains(&"other-profile"));
            match parts.as_slice() {
                [
                    "sts",
                    "get-caller-identity",
                    "--profile",
                    "chosen",
                    "--output",
                    "json",
                ] => json_output(
                    r#"{"Account":"222222222222","Arn":"arn:aws:sts::222222222222:assumed-role/admin/session"}"#,
                ),
                ["configure", "get", "region", "--profile", "chosen"] => json_output("eu-west-1\n"),
                [
                    "ec2",
                    "describe-regions",
                    "--region",
                    "us-east-1",
                    "--profile",
                    "chosen",
                    "--output",
                    "json",
                ] => json_output(
                    r#"{"Regions":[{"RegionName":"us-east-1"},{"RegionName":"eu-west-1"},{"RegionName":"us-west-2"}]}"#,
                ),
                [
                    "eks",
                    "list-clusters",
                    "--region",
                    "us-east-1",
                    "--profile",
                    "chosen",
                    "--output",
                    "json",
                ] => json_output(r#"{"clusters":["payments"]}"#),
                [
                    "eks",
                    "list-clusters",
                    "--region",
                    "eu-west-1",
                    "--profile",
                    "chosen",
                    "--output",
                    "json",
                ] => json_output(r#"{"clusters":["payments","catalog"]}"#),
                [
                    "eks",
                    "list-clusters",
                    "--region",
                    "us-west-2",
                    "--profile",
                    "chosen",
                    "--output",
                    "json",
                ] => Ok(CliOutput {
                    stdout: Vec::new(),
                    stderr: "access denied".to_owned(),
                    success: false,
                }),
                [
                    "eks",
                    "describe-cluster",
                    "--name",
                    "catalog",
                    "--region",
                    "eu-west-1",
                    "--query",
                    "cluster.tags",
                    "--profile",
                    "chosen",
                    "--output",
                    "json",
                ] => json_output(r#"{"environment":"production","team":"catalog"}"#),
                [
                    "eks",
                    "describe-cluster",
                    "--name",
                    "payments",
                    "--region",
                    "eu-west-1",
                    "--query",
                    "cluster.tags",
                    "--profile",
                    "chosen",
                    "--output",
                    "json",
                ] => Ok(CliOutput {
                    stdout: Vec::new(),
                    stderr: "access denied".to_owned(),
                    success: false,
                }),
                [
                    "eks",
                    "describe-cluster",
                    "--name",
                    "payments",
                    "--region",
                    "us-east-1",
                    "--query",
                    "cluster.tags",
                    "--profile",
                    "chosen",
                    "--output",
                    "json",
                ] => json_output("null"),
                unexpected => panic!("unexpected AWS command: {unexpected:?}"),
            }
        }
    }

    let discovery = discover_eks_with(&Runner, "222222222222", &AwsProfile::Named("chosen".into()))
        .await
        .unwrap();

    assert_eq!(discovery.clusters.len(), 3);
    assert_eq!(discovery.clusters[0].region, "eu-west-1");
    assert_eq!(discovery.clusters[0].name, "catalog");
    assert_eq!(discovery.clusters[0].tags["team"], "catalog");
    assert!(discovery.clusters[1].tags.is_empty());
    assert_eq!(
        discovery.clusters[2].arn(),
        "arn:aws:eks:us-east-1:222222222222:cluster/payments"
    );
    let warning = discovery.warning.unwrap();
    assert!(warning.contains("us-west-2"));
    assert!(warning.contains("Could not read tags for 1 EKS cluster"));
}

#[tokio::test]
async fn eks_scan_matches_configured_context_by_full_cluster_arn() {
    struct Runner;

    #[async_trait::async_trait]
    impl CliRunner for Runner {
        async fn run(&self, _program: &str, args: &[String]) -> Result<CliOutput> {
            let parts = args.iter().map(String::as_str).collect::<Vec<_>>();
            match parts.as_slice() {
                ["sts", "get-caller-identity", "--output", "json"] => json_output(
                    r#"{"Account":"222222222222","Arn":"arn:aws:iam::222222222222:user/test"}"#,
                ),
                ["configure", "get", "region"] => json_output("eu-west-1"),
                [
                    "ec2",
                    "describe-regions",
                    "--region",
                    "us-east-1",
                    "--output",
                    "json",
                ] => json_output(
                    r#"{"Regions":[{"RegionName":"eu-west-1"},{"RegionName":"us-east-1"}]}"#,
                ),
                [
                    "eks",
                    "list-clusters",
                    "--region",
                    "eu-west-1",
                    "--output",
                    "json",
                ]
                | [
                    "eks",
                    "list-clusters",
                    "--region",
                    "us-east-1",
                    "--output",
                    "json",
                ] => json_output(r#"{"clusters":["payments"]}"#),
                [
                    "eks",
                    "describe-cluster",
                    "--name",
                    "payments",
                    "--region",
                    _,
                    "--query",
                    "cluster.tags",
                    "--output",
                    "json",
                ] => json_output("{}"),
                unexpected => panic!("unexpected AWS command: {unexpected:?}"),
            }
        }
    }

    let configured = std::collections::HashSet::from([
        "arn:aws:eks:eu-west-1:222222222222:cluster/payments".to_owned(),
    ]);
    let discovery =
        discover_eks_with_configured(&Runner, "222222222222", &AwsProfile::Current, configured)
            .await
            .unwrap();

    assert_eq!(discovery.clusters.len(), 2);
    assert!(discovery.clusters[0].configured);
    assert_eq!(discovery.clusters[0].region, "eu-west-1");
    assert!(!discovery.clusters[1].configured);
    assert_eq!(discovery.clusters[1].region, "us-east-1");
}

#[tokio::test]
async fn eks_scan_rejects_profile_that_changed_accounts_before_region_calls() {
    struct Runner;

    #[async_trait::async_trait]
    impl CliRunner for Runner {
        async fn run(&self, _program: &str, args: &[String]) -> Result<CliOutput> {
            assert_eq!(args[0], "sts");
            json_output(
                r#"{"Account":"333333333333","Arn":"arn:aws:iam::333333333333:user/changed"}"#,
            )
        }
    }

    let error = discover_eks_with(&Runner, "222222222222", &AwsProfile::Current)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("different account"));
}

#[tokio::test]
async fn eks_scan_warns_when_region_inventory_is_denied_and_uses_configured_region() {
    struct Runner;

    #[async_trait::async_trait]
    impl CliRunner for Runner {
        async fn run(&self, _program: &str, args: &[String]) -> Result<CliOutput> {
            let parts = args.iter().map(String::as_str).collect::<Vec<_>>();
            match parts.as_slice() {
                [
                    "sts",
                    "get-caller-identity",
                    "--profile",
                    "restricted",
                    "--output",
                    "json",
                ] => json_output(
                    r#"{"Account":"222222222222","Arn":"arn:aws:iam::222222222222:user/restricted"}"#,
                ),
                ["configure", "get", "region", "--profile", "restricted"] => {
                    json_output("eu-west-1\n")
                }
                [
                    "ec2",
                    "describe-regions",
                    "--region",
                    "us-east-1",
                    "--profile",
                    "restricted",
                    "--output",
                    "json",
                ] => Ok(CliOutput {
                    stdout: Vec::new(),
                    stderr: "access denied".into(),
                    success: false,
                }),
                [
                    "eks",
                    "list-clusters",
                    "--region",
                    "eu-west-1",
                    "--profile",
                    "restricted",
                    "--output",
                    "json",
                ] => json_output(r#"{"clusters":["payments"]}"#),
                [
                    "eks",
                    "describe-cluster",
                    "--name",
                    "payments",
                    "--region",
                    "eu-west-1",
                    "--query",
                    "cluster.tags",
                    "--profile",
                    "restricted",
                    "--output",
                    "json",
                ] => json_output("{}"),
                unexpected => panic!("unexpected AWS command: {unexpected:?}"),
            }
        }
    }

    let discovery = discover_eks_with(
        &Runner,
        "222222222222",
        &AwsProfile::Named("restricted".into()),
    )
    .await
    .unwrap();

    assert_eq!(discovery.clusters.len(), 1);
    assert!(
        discovery
            .warning
            .unwrap()
            .contains("results may be incomplete")
    );
}

#[tokio::test]
async fn eks_scan_reports_when_every_region_fails() {
    struct Runner;

    #[async_trait::async_trait]
    impl CliRunner for Runner {
        async fn run(&self, _program: &str, args: &[String]) -> Result<CliOutput> {
            let parts = args.iter().map(String::as_str).collect::<Vec<_>>();
            match parts.as_slice() {
                ["sts", "get-caller-identity", "--output", "json"] => json_output(
                    r#"{"Account":"222222222222","Arn":"arn:aws:iam::222222222222:user/test"}"#,
                ),
                ["configure", "get", "region"] => json_output("eu-west-1"),
                [
                    "ec2",
                    "describe-regions",
                    "--region",
                    "us-east-1",
                    "--output",
                    "json",
                ] => json_output(r#"{"Regions":[{"RegionName":"eu-west-1"}]}"#),
                [
                    "eks",
                    "list-clusters",
                    "--region",
                    "eu-west-1",
                    "--output",
                    "json",
                ] => Ok(CliOutput {
                    stdout: Vec::new(),
                    stderr: "access denied".into(),
                    success: false,
                }),
                unexpected => panic!("unexpected AWS command: {unexpected:?}"),
            }
        }
    }

    let error = discover_eks_with(&Runner, "222222222222", &AwsProfile::Current)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Could not inspect any AWS Regions")
    );
}

#[tokio::test]
async fn eks_scan_reports_when_region_inventory_is_denied_without_fallback_region() {
    struct Runner;

    #[async_trait::async_trait]
    impl CliRunner for Runner {
        async fn run(&self, _program: &str, args: &[String]) -> Result<CliOutput> {
            let parts = args.iter().map(String::as_str).collect::<Vec<_>>();
            match parts.as_slice() {
                [
                    "sts",
                    "get-caller-identity",
                    "--profile",
                    "empty-region",
                    "--output",
                    "json",
                ] => json_output(
                    r#"{"Account":"222222222222","Arn":"arn:aws:iam::222222222222:user/test"}"#,
                ),
                ["configure", "get", "region", "--profile", "empty-region"] => Ok(CliOutput {
                    stdout: Vec::new(),
                    stderr: String::new(),
                    success: false,
                }),
                [
                    "ec2",
                    "describe-regions",
                    "--region",
                    "us-east-1",
                    "--profile",
                    "empty-region",
                    "--output",
                    "json",
                ] => Ok(CliOutput {
                    stdout: Vec::new(),
                    stderr: "access denied".into(),
                    success: false,
                }),
                unexpected => panic!("unexpected AWS command: {unexpected:?}"),
            }
        }
    }

    let error = discover_eks_with(
        &Runner,
        "222222222222",
        &AwsProfile::Named("empty-region".into()),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("no configured Region"));
}

#[test]
fn generated_kubeconfig_pins_named_profile_in_exec_arguments() {
    let arn = "arn:aws:eks:eu-west-1:222222222222:cluster/payments";
    let yaml = format!(
        "users:\n- name: {arn}\n  user:\n    exec:\n      command: aws\n      args: [--region, eu-west-1, eks, get-token]\n      env:\n      - name: AWS_PROFILE\n        value: chosen\n"
    );

    let output = pin_profile_in_kubeconfig(&yaml, arn, "chosen").unwrap();
    let config: serde_yaml::Value = serde_yaml::from_str(&output).unwrap();
    let args = config["users"][0]["user"]["exec"]["args"]
        .as_sequence()
        .unwrap();

    assert_eq!(args[0].as_str(), Some("--profile"));
    assert_eq!(args[1].as_str(), Some("chosen"));
    assert_eq!(args[2].as_str(), Some("--region"));
}

#[tokio::test]
async fn eks_import_writes_selected_profile_and_region_to_kubeconfig() {
    struct Runner {
        kubeconfig: PathBuf,
    }

    #[async_trait::async_trait]
    impl CliRunner for Runner {
        async fn run(&self, program: &str, args: &[String]) -> Result<CliOutput> {
            assert_eq!(program, "aws");
            let parts = args.iter().map(String::as_str).collect::<Vec<_>>();
            match parts.as_slice() {
                [
                    "sts",
                    "get-caller-identity",
                    "--profile",
                    "production",
                    "--output",
                    "json",
                ] => json_output(
                    r#"{"Account":"222222222222","Arn":"arn:aws:sts::222222222222:assumed-role/admin/session"}"#,
                ),
                [
                    "eks",
                    "update-kubeconfig",
                    "--name",
                    "payments",
                    "--region",
                    "eu-west-1",
                    "--dry-run",
                    "--kubeconfig",
                    path,
                    "--profile",
                    "production",
                    "--output",
                    "json",
                ] => {
                    assert_eq!(Path::new(path), self.kubeconfig.as_path());
                    assert!(
                        std::fs::read_to_string(path)
                            .unwrap()
                            .contains("- name: existing")
                    );
                    json_output(
                        "apiVersion: v1\nkind: Config\nclusters:\n- name: existing\n  cluster:\n    server: https://existing.invalid\n- name: arn:aws:eks:eu-west-1:222222222222:cluster/payments\n  cluster:\n    server: https://example.invalid\ncontexts:\n- name: existing\n  context:\n    cluster: existing\n    user: existing\n- name: arn:aws:eks:eu-west-1:222222222222:cluster/payments\n  context:\n    cluster: arn:aws:eks:eu-west-1:222222222222:cluster/payments\n    user: arn:aws:eks:eu-west-1:222222222222:cluster/payments\nusers:\n- name: existing\n  user:\n    token: old-token\n- name: arn:aws:eks:eu-west-1:222222222222:cluster/payments\n  user:\n    exec:\n      command: aws\n      args: [--region, eu-west-1, eks, get-token]\n      env:\n      - name: AWS_PROFILE\n        value: production\n",
                    )
                }
                unexpected => panic!("unexpected AWS command: {unexpected:?}"),
            }
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let kubeconfig = directory.path().join("config");
    std::fs::write(
        &kubeconfig,
        "apiVersion: v1\nkind: Config\ncontexts:\n- name: existing\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&kubeconfig, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    add_eks_with(
        &Runner {
            kubeconfig: kubeconfig.clone(),
        },
        "222222222222",
        "eu-west-1",
        "payments",
        &AwsProfile::Named("production".into()),
        &kubeconfig,
    )
    .await
    .unwrap();

    let config: serde_yaml::Value =
        serde_yaml::from_slice(&std::fs::read(&kubeconfig).unwrap()).unwrap();
    let args = config["users"][1]["user"]["exec"]["args"]
        .as_sequence()
        .unwrap();
    assert_eq!(args[0].as_str(), Some("--profile"));
    assert_eq!(args[1].as_str(), Some("production"));
    assert_eq!(
        config["contexts"][1]["name"].as_str(),
        Some("arn:aws:eks:eu-west-1:222222222222:cluster/payments")
    );
    assert_eq!(config["contexts"][0]["name"].as_str(), Some("existing"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&kubeconfig).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
