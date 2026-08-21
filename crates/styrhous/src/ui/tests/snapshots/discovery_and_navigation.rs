//! Cluster discovery and resource-navigation scenarios.

use super::*;

mod owner_navigation;

#[test]
fn settings_blade_shows_invalid_custom_template_after_save() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();

    let settings_position = harness.get_by_label("Settings").rect().center();
    primary_click(&mut harness, settings_position);
    harness.run();
    let application_settings_position = harness
        .get_by_label(OPEN_APPLICATION_SETTINGS)
        .rect()
        .center();
    primary_click(&mut harness, application_settings_position);
    harness.run();
    let custom_launcher_position = harness
        .get_by_role_and_label(egui::accesskit::Role::RadioButton, "Custom launcher")
        .rect()
        .center();
    primary_click(&mut harness, custom_launcher_position);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Command template")
        .click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("alacritty".into()));
    harness.run();
    assert_eq!(
        harness
            .state()
            .ui_state
            .terminal_settings_blade()
            .unwrap()
            .draft
            .custom_template,
        Some("alacritty".into())
    );
    let save_position = harness.get_by_label("Save changes").rect().center();
    primary_click(&mut harness, save_position);
    harness.run();

    assert_eq!(
        harness
            .state()
            .ui_state
            .terminal_settings_blade()
            .unwrap()
            .error
            .as_deref(),
        Some("The launcher template must contain exactly one {command} placeholder.")
    );
    harness.get_by_label("Command template needs attention");
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "terminal/settings_blade_shows_invalid_custom_template_after_save/settings_terminal_launcher_invalid",
    ));
}

#[test]
fn settings_home_navigates_to_cluster_discovery_and_shows_candidates() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();

    open_settings(&mut harness);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "settings/settings_home_navigates_to_cluster_discovery_and_shows_candidates/settings_home",
    ));

    open_cluster_discovery(&mut harness);
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ManagedClusterDiscoveryUpdated {
            tools: ClusterDiscoveryTools {
                azure_cli: true,
                tailscale: true,
            },
            aks_clusters: vec![
                AvailableAksCluster {
                    name: "payments-prod".into(),
                    location: "westeurope".into(),
                    resource_group: "platform".into(),
                    tags: BTreeMap::new(),
                    subscription_id: "development-platform".into(),
                    subscription_name: "Platform Engineering".into(),
                    tenant_name: "Acme Development".into(),
                    tenant_default_domain: "development.example".into(),
                    configured: false,
                },
                AvailableAksCluster {
                    name: "payments-staging".into(),
                    location: "westeurope".into(),
                    resource_group: "platform".into(),
                    tags: BTreeMap::new(),
                    subscription_id: "development-sandbox".into(),
                    subscription_name: "Sandbox".into(),
                    tenant_name: "Acme Development".into(),
                    tenant_default_domain: "development.example".into(),
                    configured: true,
                },
                AvailableAksCluster {
                    name: "catalog-prod".into(),
                    location: "northeurope".into(),
                    resource_group: "workload".into(),
                    tags: BTreeMap::new(),
                    subscription_id: "northwind-production".into(),
                    subscription_name: "Production".into(),
                    tenant_name: "Northwind Operations".into(),
                    tenant_default_domain: "northwind.example".into(),
                    configured: false,
                },
                AvailableAksCluster {
                    name: "catalog-dev".into(),
                    location: "eastus".into(),
                    resource_group: "workload".into(),
                    tags: BTreeMap::new(),
                    subscription_id: "northwind-development".into(),
                    subscription_name: "Development".into(),
                    tenant_name: "Northwind Operations".into(),
                    tenant_default_domain: "northwind.example".into(),
                    configured: false,
                },
                AvailableAksCluster {
                    name: "observability".into(),
                    location: "westeurope".into(),
                    resource_group: "platform".into(),
                    tags: BTreeMap::new(),
                    subscription_id: "northwind-shared".into(),
                    subscription_name: "Shared Services".into(),
                    tenant_name: "Northwind Operations".into(),
                    tenant_default_domain: "northwind.example".into(),
                    configured: false,
                },
            ],
            tailscale_clusters: vec![
                AvailableTailscaleCluster {
                    host_name: "k8s-prod".into(),
                    dns_name: "k8s-prod.tailnet.ts.net".into(),
                    online: true,
                    configured: true,
                },
                AvailableTailscaleCluster {
                    host_name: "k8s-staging".into(),
                    dns_name: "k8s-staging.tailnet.ts.net".into(),
                    online: false,
                    configured: false,
                },
                AvailableTailscaleCluster {
                    host_name: "edge-cluster".into(),
                    dns_name: "edge-cluster.tailnet.ts.net".into(),
                    online: true,
                    configured: false,
                },
            ],
            azure_error: None,
            azure_warning: Some(
                "Could not inspect 1 subscription. Refresh discovery to retry.".into(),
            ),
            tailscale_error: None,
        }) as WorkerResultBox);
    harness.run_steps(2);

    assert_eq!(harness.get_all_by_label("Add to kubeconfig").count(), 6);
    assert_eq!(harness.get_all_by_label("Already in kubeconfig").count(), 2);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "settings/settings_home_navigates_to_cluster_discovery_and_shows_candidates/cluster_discovery",
    ));

    let add_aks_position = harness
        .get_all_by_label("Add to kubeconfig")
        .next()
        .expect("the first AKS row is addable")
        .rect()
        .center();
    primary_click(&mut harness, add_aks_position);
    harness.run_steps(2);
    assert_eq!(harness.get_all_by_label("Adding…").count(), 1);
    assert_eq!(harness.get_all_by_label("Add to kubeconfig").count(), 5);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "settings/settings_home_navigates_to_cluster_discovery_and_shows_candidates/importing_candidate",
    ));
    let command = harness
        .state()
        .worker
        .commands
        .iter()
        .find_map(command_is::<AddAksCluster>)
        .expect("adding AKS emits its worker command");
    assert_eq!(command.subscription_id, "development-platform");
    assert_eq!(command.resource_group, "platform");
    assert_eq!(command.cluster_name, "payments-prod");
}

#[test]
fn cluster_discovery_shows_install_guidance_when_cli_tools_are_unavailable() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();

    open_settings(&mut harness);
    open_cluster_discovery(&mut harness);
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ManagedClusterDiscoveryUpdated {
            tools: ClusterDiscoveryTools {
                azure_cli: false,
                tailscale: false,
            },
            aks_clusters: Vec::new(),
            tailscale_clusters: Vec::new(),
            azure_error: None,
            azure_warning: None,
            tailscale_error: None,
        }) as WorkerResultBox);
    harness.run_steps(2);

    harness.get_by_label("Azure CLI is not installed");
    harness.get_by_label("Install Azure CLI and sign in with `az login` to discover AKS clusters.");
    harness.get_by_label("Tailscale is not installed");
    harness.get_by_label("Install and sign in to Tailscale to discover Kubernetes API proxies.");
    assert_eq!(harness.query_all_by_label("Add to kubeconfig").count(), 0);
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "settings/cluster_discovery_shows_install_guidance_when_cli_tools_are_unavailable/cli_tools_unavailable",
    ));
}

#[test]
fn resource_name_opens_and_closes_a_live_detail_inspector() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();
    harness.get_by_label("Apps & Containers").click_accesskit();
    harness.run();

    let name = "coredns-66bc5c9577-ffw2s";
    let resource_position = harness
        .get_by_label(&format!("Open details for {name}"))
        .rect()
        .center();
    primary_click(&mut harness, resource_position);
    harness.run_steps(1);

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .is_some_and(|command| {
                    command.cluster_key == 2
                        && command.resource_name == name
                        && command.resource_uid == "fixture-0"
                        && command.history_entry_id == 1
                })),
        "commands: {:?}",
        harness.state().worker.commands
    );

    let pods = fixture_api_resource("core", "Pod", "pods");
    harness
        .state_mut()
        .worker
        .results
        .push_back(Box::new(ResourceDetailUpdated {
            cluster_key: 2,
            history_entry_id: 1,
            detail: Box::new(ResourceDetail {
                api_resource: pods,
                name: name.into(),
                namespace: Some("kube-system".into()),
                uid: "fixture-0".into(),
                resource_version: "1".into(),
                is_deleting: false,
                finalizers: Vec::new(),
                creation_timestamp: None,
                owners: Vec::new(),
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                payload: ResourceDetailPayload::Generic,
            }),
        }) as WorkerResultBox);
    harness.run();
    assert!(
        harness
            .state()
            .ui_state
            .global_blades
            .navigator()
            .and_then(|navigator| navigator.current().resource_detail())
            .and_then(|entry| entry.detail.as_ref())
            .is_some()
    );
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 1.0);
    harness.get_by_label("Close blade").click_accesskit();
    harness.run_steps(2);

    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_some(),
        "the inspector remains present while its close animation is in progress"
    );
    harness
        .ctx
        .global_style_mut(|style| style.animation_time = 0.0);
    harness.run();

    assert!(
        harness.state().ui_state.clusters[&2]
            .resource_detail_panel
            .is_none()
    );
    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StopResourceDetailWatch>()
                .is_some_and(|command| command.cluster_key == 2))
    );
}
#[test]
fn clicking_a_pod_node_in_the_resource_table_opens_the_node_inspector() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    let pod = cluster
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .expect("Pod watch fixture exists")
        .resources
        .values_mut()
        .next()
        .expect("Pod fixture exists");
    pod.cells.insert(
        NODE_COLUMN.into(),
        CellValue::Text("kind-control-plane".into()),
    );
    harness.state_mut().ui_state = state;
    harness.run();

    let node_position = harness
        .get_by_label("Open details for Node kind-control-plane")
        .rect()
        .center();
    primary_click(&mut harness, node_position);
    harness.run_steps(1);

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .is_some_and(|command| {
                    command.api_resource == crate::resource_handlers::node::api_resource()
                        && command.namespace.is_none()
                        && command.resource_name == "kind-control-plane"
                        && command.resource_uid == "kind-control-plane"
                }))
    );
}
#[test]
fn clicking_a_controller_owner_in_the_resource_table_opens_its_inspector() {
    let mut harness = application_harness::<MockWorker>();
    let mut state = oracle_resource_table_state();
    let pods = fixture_api_resource("core", "Pod", "pods");
    let replica_set = fixture_api_resource("apps", "ReplicaSet", "replicasets");
    let cluster = state.clusters.get_mut(&2).expect("kind fixture exists");
    cluster.resource_navigation =
        build_resource_navigation(vec![pods.clone(), replica_set.clone()]);
    let pod = cluster
        .resource_cache
        .get_mut(&(pods, Some("kube-system".into())))
        .expect("Pod watch fixture exists")
        .resources
        .values_mut()
        .next()
        .expect("Pod fixture exists");
    pod.controller_owner = Some(ResourceOwner {
        api_version: "apps/v1".into(),
        kind: "ReplicaSet".into(),
        name: "api-7b948f".into(),
        uid: "replicaset-uid".into(),
        controller: true,
    });
    harness.state_mut().ui_state = state;
    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(
        "resource_tables/controller_owner_link_opens_its_inspector/controller_owner_link",
    ));

    let owner_position = harness
        .get_by_label("Open details for ReplicaSet / api-7b948f")
        .rect()
        .center();
    primary_click(&mut harness, owner_position);
    harness.run_steps(1);

    assert!(
        harness
            .state()
            .worker
            .commands
            .last()
            .is_some_and(|command| command
                .as_ref()
                .as_any()
                .downcast_ref::<StartResourceDetailWatch>()
                .is_some_and(|command| {
                    command.api_resource == replica_set
                        && command.namespace.as_deref() == Some("kube-system")
                        && command.resource_name == "api-7b948f"
                        && command.resource_uid == "replicaset-uid"
                }))
    );
}
