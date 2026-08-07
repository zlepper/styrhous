use super::super::MyEguiApp;
use super::fixtures::application_harness;
use crate::api_resource::ApiResource;
use crate::worker::Worker;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn wait_for<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    condition: impl Fn(&MyEguiApp<Worker>) -> Option<T>,
    max_ms: u64,
) -> T {
    let start = std::time::Instant::now();
    while start.elapsed().as_millis() < max_ms as u128 {
        harness.run();
        if let Some(result) = condition(harness.state()) {
            return result;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("Timed out after {max_ms}ms waiting for UI state");
}

fn connected_kind_harness() -> (Harness<'static, MyEguiApp<Worker>>, i32) {
    let mut harness = application_harness::<Worker>();
    wait_for(
        &mut harness,
        |app| (!app.ui_state.clusters.is_empty()).then_some(()),
        5_000,
    );
    harness.get_by_label("kind-kind").click();
    harness.run();
    let cluster_key = harness
        .state()
        .ui_state
        .selected_cluster
        .expect("Kind cluster should be selected after click");
    (harness, cluster_key)
}

fn wait_for_cluster_data(harness: &mut Harness<MyEguiApp<Worker>>, cluster_key: i32) {
    wait_for(
        harness,
        |app| {
            app.ui_state.clusters.get(&cluster_key).and_then(|cluster| {
                (!cluster.namespaces.is_empty()
                    && (!cluster.resource_navigation.curated_sections.is_empty()
                        || !cluster.resource_navigation.other_api_groups.is_empty()))
                .then_some(())
            })
        },
        10_000,
    );
}

fn select_namespace(harness: &mut Harness<MyEguiApp<Worker>>, namespace: &str) {
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    harness.run();
    harness.get_by_label(namespace).click();
    harness.run();
}

fn select_resource(
    harness: &mut Harness<MyEguiApp<Worker>>,
    section: &str,
    resource_name: &str,
) -> ApiResource {
    harness.get_by_label(section).click_accesskit();
    harness.run();
    harness.run();
    harness.get_by_label(resource_name).click_accesskit();
    harness.run();
    let cluster_key = harness
        .state()
        .ui_state
        .selected_cluster
        .expect("cluster is selected");
    harness.state().ui_state.clusters[&cluster_key]
        .selected_api_resource
        .clone()
        .expect("resource should be selected")
}

fn wait_for_resource_sync(
    harness: &mut Harness<MyEguiApp<Worker>>,
    cluster_key: i32,
    api_resource: ApiResource,
    namespace: &str,
) {
    let namespace = namespace.to_owned();
    wait_for(
        harness,
        move |app| {
            app.ui_state.clusters.get(&cluster_key).and_then(|cluster| {
                cluster
                    .resource_cache
                    .get(&(api_resource.clone(), namespace.clone()))
                    .filter(|state| state.is_synced)
                    .map(|_| ())
            })
        },
        10_000,
    );
}

/// Snapshot test for the configured local Kind cluster.
#[test]
fn test_real_cluster_connection() {
    let mut harness = application_harness::<Worker>();
    for _ in 0..10 {
        harness.run();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    harness.snapshot("real_clusters");
}

/// Integration test for a real resource watcher using accessibility interactions.
#[test]
fn test_resource_watcher_integration() {
    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, "kube-system");
    assert!(
        harness.state().ui_state.clusters[&cluster_key]
            .selected_namespaces
            .contains("kube-system")
    );

    let pods_resource = select_resource(&mut harness, "Apps & Containers", "pods");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        pods_resource.clone(),
        "kube-system",
    );

    let resources = &harness.state().ui_state.clusters[&cluster_key].resource_cache
        [&(pods_resource, "kube-system".to_owned())]
        .resources;
    assert!(!resources.is_empty(), "Kind should have kube-system pods");
    assert!(
        resources
            .values()
            .any(|resource| resource.name.starts_with("coredns")),
        "Kind should have a CoreDNS pod"
    );
    harness.snapshot("integration_resource_table");
}

/// Creates a ConfigMap, edits it through the UI, and then deletes it through the UI.
#[test]
fn test_resource_actions_integration() {
    use k8s_openapi::api::core::v1::ConfigMap;
    use kube::{Api, Client};
    use std::collections::BTreeMap;

    let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let test_configmap_name = "test-cm-integration".to_owned();
    let client = runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create kube client")
    });
    let configmaps: Api<ConfigMap> = Api::namespaced(client, "default");
    runtime.block_on(async {
        let _ = configmaps
            .delete(&test_configmap_name, &Default::default())
            .await;
        configmaps
            .create(
                &Default::default(),
                &ConfigMap {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(test_configmap_name.clone()),
                        namespace: Some("default".to_owned()),
                        ..Default::default()
                    },
                    data: Some(BTreeMap::from([(
                        String::from("key1"),
                        String::from("original-value"),
                    )])),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create test ConfigMap");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, "default");
    let configmaps_resource = select_resource(&mut harness, "Config", "configmaps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        configmaps_resource.clone(),
        "default",
    );
    assert!(
        harness.state().ui_state.clusters[&cluster_key].resource_cache
            [&(configmaps_resource.clone(), "default".to_owned())]
            .resources
            .values()
            .any(|resource| resource.name == test_configmap_name)
    );

    for _ in 0..3 {
        harness.run();
    }
    let actions_label = format!("More actions for {test_configmap_name}");
    harness.get_by_label(&actions_label).click_accesskit();
    harness.run();
    harness.get_by_label("Edit YAML").click_accesskit();
    harness.run();
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .yaml_panel
                .as_ref()
                .filter(|panel| panel.resource_name == test_configmap_name)
                .map(|_| ())
        },
        5_000,
    );

    let yaml_panel = harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&cluster_key)
        .and_then(|cluster| cluster.yaml_panel.as_mut())
        .expect("YAML panel should be open");
    yaml_panel.edited_yaml = yaml_panel
        .edited_yaml
        .replace("original-value", "edited-value");
    harness.run();
    harness.get_by_label("Save YAML").click();
    harness.run();
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .yaml_panel
                .is_none()
                .then_some(())
        },
        5_000,
    );

    let configmap = runtime.block_on(async {
        configmaps
            .get(&test_configmap_name)
            .await
            .expect("ConfigMap should be updated")
    });
    assert_eq!(
        configmap
            .data
            .as_ref()
            .and_then(|data| data.get("key1"))
            .map(String::as_str),
        Some("edited-value")
    );

    for _ in 0..5 {
        harness.run();
    }
    harness.get_by_label(&actions_label).click_accesskit();
    harness.run();
    harness.get_by_label("Delete").click_accesskit();
    harness.run();
    assert!(
        harness.state().ui_state.clusters[&cluster_key]
            .pending_delete
            .as_ref()
            .is_some_and(|pending| pending.resource_name == test_configmap_name)
    );

    let confirm_delete_label = format!("Delete {test_configmap_name}");
    harness
        .get_by_label(&confirm_delete_label)
        .click_accesskit();
    harness.run();
    wait_for(
        &mut harness,
        |app| {
            let resources = &app.ui_state.clusters[&cluster_key].resource_cache
                [&(configmaps_resource.clone(), "default".to_owned())]
                .resources;
            (!resources
                .values()
                .any(|resource| resource.name == test_configmap_name))
            .then_some(())
        },
        10_000,
    );
    assert!(
        runtime
            .block_on(async { configmaps.get(&test_configmap_name).await })
            .is_err()
    );
}
