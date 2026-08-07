use super::super::MyEguiApp;
use super::super::state::ClusterConnectionState;
use super::fixtures::application_harness;
use crate::api_resource::ApiResource;
use crate::sorted_name::SortedName;
use crate::worker::Worker;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use k8s_openapi::api::core::v1::{ConfigMap, Namespace};
use kube::{Api, Client};
use std::collections::BTreeMap;

const WATCHER_CONFIGMAP_NAME: &str = "resource-watcher";
const ACTIONS_CONFIGMAP_NAME: &str = "resource-actions";

struct IntegrationConfigMap {
    runtime: tokio::runtime::Runtime,
    namespaces: Api<Namespace>,
    configmaps: Api<ConfigMap>,
    namespace: String,
    name: String,
}

impl IntegrationConfigMap {
    fn create(test_name: &str, name: &str, value: &str) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        let client = runtime.block_on(async {
            Client::try_default()
                .await
                .expect("Failed to create Kubernetes client")
        });
        let namespaces: Api<Namespace> = Api::all(client.clone());
        let namespace = format!(
            "kdui-it-{test_name}-{}-{:x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after the Unix epoch")
                .as_nanos()
        );
        runtime.block_on(async {
            namespaces
                .create(
                    &Default::default(),
                    &Namespace {
                        metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                            name: Some(namespace.clone()),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .await
                .expect("Failed to create integration namespace");
        });

        let configmaps: Api<ConfigMap> = Api::namespaced(client, &namespace);
        runtime.block_on(async {
            configmaps
                .create(
                    &Default::default(),
                    &ConfigMap {
                        metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                            name: Some(name.to_owned()),
                            namespace: Some(namespace.clone()),
                            ..Default::default()
                        },
                        data: Some(BTreeMap::from([(String::from("key1"), value.to_owned())])),
                        ..Default::default()
                    },
                )
                .await
                .expect("Failed to create integration ConfigMap");
        });

        Self {
            runtime,
            namespaces,
            configmaps,
            namespace,
            name: name.to_owned(),
        }
    }
}

impl Drop for IntegrationConfigMap {
    fn drop(&mut self) {
        let _ = self.runtime.block_on(async {
            self.namespaces
                .delete(&self.namespace, &Default::default())
                .await
        });
    }
}

fn wait_for<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    condition: impl Fn(&MyEguiApp<Worker>) -> Option<T>,
    max_ms: u64,
) -> T {
    let start = std::time::Instant::now();
    while start.elapsed().as_millis() < max_ms as u128 {
        harness.run_steps(1);
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
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "kind-kind")
        .click();
    harness.run_steps(1);
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
    harness.run_steps(1);
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

/// Verifies that the worker can connect to Kind and discover cluster data.
#[test]
fn test_real_cluster_connection() {
    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);

    let cluster = &harness.state().ui_state.clusters[&cluster_key];
    assert_eq!(cluster.name, "kind-kind");
    assert!(matches!(
        cluster.connection,
        ClusterConnectionState::Connected(_)
    ));
    assert!(cluster.namespaces.contains_key(&SortedName::new("default")));
    assert!(
        !cluster.resource_navigation.curated_sections.is_empty()
            || !cluster.resource_navigation.other_api_groups.is_empty(),
        "Kind should advertise Kubernetes API resources"
    );
}

/// Integration test for a real resource watcher using accessibility interactions.
#[test]
fn test_resource_watcher_integration() {
    let fixture =
        IntegrationConfigMap::create("resource-watcher", WATCHER_CONFIGMAP_NAME, "watcher-value");
    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, &fixture.namespace);
    assert!(
        harness.state().ui_state.clusters[&cluster_key]
            .selected_namespaces
            .contains(&fixture.namespace)
    );

    let configmaps_resource = select_resource(&mut harness, "Config", "configmaps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        configmaps_resource.clone(),
        &fixture.namespace,
    );

    let resources = &harness.state().ui_state.clusters[&cluster_key].resource_cache
        [&(configmaps_resource, fixture.namespace.clone())]
        .resources;
    assert!(
        resources
            .values()
            .any(|resource| resource.name == fixture.name),
        "resource watcher should report the integration ConfigMap"
    );
}

/// Creates a ConfigMap, edits it through the UI, and then deletes it through the UI.
#[test]
fn test_resource_actions_integration() {
    let fixture =
        IntegrationConfigMap::create("resource-actions", ACTIONS_CONFIGMAP_NAME, "original-value");
    let test_configmap_name = fixture.name.clone();
    let runtime = &fixture.runtime;
    let configmaps = &fixture.configmaps;

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, &fixture.namespace);
    let configmaps_resource = select_resource(&mut harness, "Config", "configmaps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        configmaps_resource.clone(),
        &fixture.namespace,
    );
    assert!(
        harness.state().ui_state.clusters[&cluster_key].resource_cache
            [&(configmaps_resource.clone(), fixture.namespace.clone())]
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
                [&(configmaps_resource.clone(), fixture.namespace.clone())]
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
