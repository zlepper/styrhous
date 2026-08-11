use super::super::MyEguiApp;
use super::super::state::ClusterConnectionState;
use super::fixtures::application_harness;
use crate::api_resource::ApiResource;
use crate::resource_table::{READY_COLUMN, STATUS_COLUMN};
use crate::sorted_name::SortedName;
use crate::worker::Worker;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{ConfigMap, Namespace, Secret};
use k8s_openapi::api::core::v1::{Container, Pod, PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::Patch;
use kube::{Api, Client};
use std::collections::BTreeMap;
use std::process::Command;
use std::time::{Duration, Instant};

const WATCHER_CONFIGMAP_NAME: &str = "resource-watcher";
const ACTIONS_CONFIGMAP_NAME: &str = "resource-actions";
const ACTIONS_SECRET_NAME: &str = "resource-secret-actions";
const TEST_NAMESPACE_PREFIX: &str = "kdui-it-";
const TEST_FINALIZER: &str = "tests.kubernetes-dev-ui/finalizer";
const FIXTURE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(30);

struct IntegrationNamespaceFixture {
    runtime: tokio::runtime::Runtime,
    namespaces: Api<Namespace>,
    configmaps: Api<ConfigMap>,
    secrets: Api<Secret>,
    namespace: String,
    name: String,
}

impl IntegrationNamespaceFixture {
    fn create(test_name: &str, name: &str, value: &str) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        let client = runtime.block_on(async {
            Client::try_default()
                .await
                .expect("Failed to create Kubernetes client")
        });
        let namespaces: Api<Namespace> = Api::all(client.clone());
        let namespace = format!(
            "{TEST_NAMESPACE_PREFIX}{test_name}-{}-{:x}",
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

        let configmaps: Api<ConfigMap> = Api::namespaced(client.clone(), &namespace);
        let secrets: Api<Secret> = Api::namespaced(client, &namespace);
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
            secrets,
            namespace,
            name: name.to_owned(),
        }
    }
}

/// Updates an existing Secret value through the inspector without exposing its
/// plaintext until the test explicitly operates on the editor state.
#[test]
fn test_secret_inspector_actions_integration() {
    let fixture = IntegrationNamespaceFixture::create(
        "resource-secret-actions",
        "secret-actions-anchor",
        "unused",
    );
    let test_secret_name = ACTIONS_SECRET_NAME.to_owned();
    let runtime = &fixture.runtime;
    let secrets = &fixture.secrets;
    runtime.block_on(async {
        secrets
            .create(
                &Default::default(),
                &Secret {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(test_secret_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    data: Some(BTreeMap::from([(
                        "password".to_owned(),
                        k8s_openapi::ByteString(b"original-secret".to_vec()),
                    )])),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create integration Secret");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let secrets_resource = select_resource(&mut harness, "Config", "Secrets");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        secrets_resource.clone(),
        &fixture.namespace,
    );
    for _ in 0..3 {
        harness.run_steps(1);
    }
    harness
        .get_by_label(&format!("Open details for {test_secret_name}"))
        .click();
    harness.run_steps(1);
    wait_for_data_editor(&mut harness, cluster_key, "password");
    harness
        .state_mut()
        .ui_state
        .clusters
        .get_mut(&cluster_key)
        .expect("selected cluster should exist")
        .resource_detail_panel
        .as_mut()
        .and_then(|panel| panel.data_editor.as_mut())
        .expect("Secret detail editor should be available")
        .draft_values
        .insert("password".to_owned(), "updated-secret".to_owned());
    harness.run_steps(1);
    harness.get_by_label("Save data").click_accesskit();
    harness.run_steps(1);
    wait_for(
        &mut harness,
        |_| {
            runtime
                .block_on(async { secrets.get(&test_secret_name).await })
                .ok()
                .filter(|secret| {
                    secret
                        .data
                        .as_ref()
                        .and_then(|data| data.get("password"))
                        .is_some_and(|value| value.0 == b"updated-secret")
                })
                .map(|_| ())
        },
        10_000,
    );
}

#[test]
fn test_kind_setup_purges_leftover_test_namespaces() {
    let fixture = IntegrationNamespaceFixture::create("setup-purge", "stuck", "unused");
    fixture.runtime.block_on(async {
        fixture
            .configmaps
            .patch(
                &fixture.name,
                &Default::default(),
                &Patch::Merge(&k8s_openapi::serde_json::json!({
                    "metadata": { "finalizers": [TEST_FINALIZER] }
                })),
            )
            .await
            .expect("Failed to add finalizer to purge-test ConfigMap");
    });

    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("app manifest directory should be two levels below the workspace root");
    let status = Command::new("bash")
        .arg("scripts/ensure-kind-cluster.sh")
        .current_dir(workspace_root)
        .status()
        .expect("Failed to invoke Kind setup script");
    assert!(status.success(), "Kind setup script should succeed");

    let namespace_exists = fixture.runtime.block_on(async {
        !matches!(
            fixture.namespaces.get(&fixture.namespace).await,
            Err(kube::Error::Api(error)) if error.code == 404
        )
    });
    assert!(
        !namespace_exists,
        "Kind setup script should delete leftover integration namespaces"
    );
}

impl Drop for IntegrationNamespaceFixture {
    fn drop(&mut self) {
        let cleanup_result = self.runtime.block_on(async {
            // A failure in the force-delete test can otherwise strand this fixture's
            // ConfigMap (and therefore its namespace) in Terminating.
            let _ = self
                .configmaps
                .patch(
                    &self.name,
                    &Default::default(),
                    &Patch::Merge(&k8s_openapi::serde_json::json!({
                        "metadata": { "finalizers": [] }
                    })),
                )
                .await;
            if let Err(error) = self
                .namespaces
                .delete(&self.namespace, &Default::default())
                .await
                && !matches!(&error, kube::Error::Api(response) if response.code == 404)
            {
                return Err(error.into());
            }

            let deadline = Instant::now() + FIXTURE_CLEANUP_TIMEOUT;
            loop {
                match self.namespaces.get(&self.namespace).await {
                    Err(kube::Error::Api(error)) if error.code == 404 => return Ok(()),
                    Ok(_) if Instant::now() < deadline => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Ok(_) => anyhow::bail!(
                        "Timed out waiting for integration namespace {} to be deleted",
                        self.namespace
                    ),
                    Err(error) => return Err(error.into()),
                }
            }
        });
        if let Err(error) = cleanup_result {
            if std::thread::panicking() {
                eprintln!(
                    "Failed to clean up integration namespace {}: {error:#}",
                    self.namespace
                );
            } else {
                panic!(
                    "Failed to clean up integration namespace {}: {error:#}",
                    self.namespace
                );
            }
        }
    }
}

fn wait_for<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    condition: impl Fn(&MyEguiApp<Worker>) -> Option<T>,
    max_ms: u64,
) -> T {
    wait_for_with_diagnostic(harness, condition, |_| None, max_ms)
}

fn wait_for_with_diagnostic<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    condition: impl Fn(&MyEguiApp<Worker>) -> Option<T>,
    diagnostic: impl Fn(&MyEguiApp<Worker>) -> Option<String>,
    max_ms: u64,
) -> T {
    wait_for_harness(
        harness,
        |harness| {
            let app = harness.state();
            if let Some(message) = diagnostic(app) {
                panic!("{message}");
            }
            condition(app)
        },
        max_ms,
    )
}

fn wait_for_harness<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    condition: impl Fn(&mut Harness<MyEguiApp<Worker>>) -> Option<T>,
    max_ms: u64,
) -> T {
    let start = Instant::now();
    while start.elapsed().as_millis() < max_ms as u128 {
        harness.run_steps(1);
        if let Some(result) = condition(harness) {
            return result;
        }
        std::thread::sleep(Duration::from_millis(50));
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
                    && (!cluster.resource_navigation.curated_entries.is_empty()
                        || !cluster.resource_navigation.other_api_groups.is_empty()))
                .then_some(())
            })
        },
        10_000,
    );
}

fn select_namespace(harness: &mut Harness<MyEguiApp<Worker>>, cluster_key: i32, namespace: &str) {
    let namespace = namespace.to_owned();
    wait_for(
        harness,
        |app| {
            app.ui_state
                .clusters
                .get(&cluster_key)
                .filter(|cluster| {
                    cluster
                        .namespaces
                        .contains_key(&SortedName::new(&namespace))
                })
                .map(|_| ())
        },
        10_000,
    );
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    wait_for_harness(
        harness,
        |harness| harness.query_by_label(&namespace).map(|_| ()),
        10_000,
    );
    harness.get_by_label(&namespace).click();
    wait_for(
        harness,
        |app| {
            app.ui_state
                .clusters
                .get(&cluster_key)
                .filter(|cluster| cluster.selected_namespaces.contains(&namespace))
                .map(|_| ())
        },
        10_000,
    );
}

fn wait_for_data_editor(
    harness: &mut Harness<MyEguiApp<Worker>>,
    cluster_key: i32,
    data_key: &str,
) {
    wait_for_with_diagnostic(
        harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .resource_detail_panel
                .as_ref()
                .and_then(|panel| panel.data_editor.as_ref())
                .filter(|editor| editor.draft_values.contains_key(data_key))
                .map(|_| ())
        },
        |app| {
            app.ui_state.clusters[&cluster_key]
                .resource_detail_panel
                .as_ref()
                .and_then(|panel| panel.detail_error.as_ref())
                .map(|error| {
                    format!("Resource detail watch failed while loading data editor: {error}")
                })
        },
        10_000,
    );
}

fn select_resource(
    harness: &mut Harness<MyEguiApp<Worker>>,
    section: &str,
    resource_name: &str,
) -> ApiResource {
    harness.get_by_label(section).click_accesskit();
    harness.run_steps(1);
    harness.run_steps(1);
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
                    .get(&(api_resource.clone(), Some(namespace.clone())))
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
        ClusterConnectionState::Connected
    ));
    assert!(cluster.namespaces.contains_key(&SortedName::new("default")));
    assert!(
        !cluster.resource_navigation.curated_entries.is_empty()
            || !cluster.resource_navigation.other_api_groups.is_empty(),
        "Kind should advertise Kubernetes API resources"
    );
}

/// Integration test for a real resource watcher using accessibility interactions.
#[test]
fn test_resource_watcher_integration() {
    let fixture = IntegrationNamespaceFixture::create(
        "resource-watcher",
        WATCHER_CONFIGMAP_NAME,
        "watcher-value",
    );
    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    assert!(
        harness.state().ui_state.clusters[&cluster_key]
            .selected_namespaces
            .contains(&fixture.namespace)
    );

    let configmaps_resource = select_resource(&mut harness, "Config", "Config Maps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        configmaps_resource.clone(),
        &fixture.namespace,
    );

    let resources = &harness.state().ui_state.clusters[&cluster_key].resource_cache
        [&(configmaps_resource, Some(fixture.namespace.clone()))]
        .resources;
    assert!(
        resources
            .values()
            .any(|resource| resource.name == fixture.name),
        "resource watcher should report the integration ConfigMap"
    );
}

/// Verifies that the inspector follows the real Deployment -> ReplicaSet -> Pod
/// ownership chain without relying on table-cache data.
#[test]
fn test_managed_resource_inspector_integration() {
    let fixture =
        IntegrationNamespaceFixture::create("managed-resource-inspector", "anchor", "unused");
    let deployment_name = "managed-resource-inspector".to_owned();
    let runtime = &fixture.runtime;
    let client = runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let deployments: Api<Deployment> = Api::namespaced(client, &fixture.namespace);
    runtime.block_on(async {
        deployments
            .create(
                &Default::default(),
                &Deployment {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(deployment_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    spec: Some(DeploymentSpec {
                        replicas: Some(1),
                        selector: LabelSelector {
                            match_labels: Some(BTreeMap::from([(
                                "app".to_owned(),
                                deployment_name.clone(),
                            )])),
                            ..Default::default()
                        },
                        template: PodTemplateSpec {
                            metadata: Some(
                                k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                                    labels: Some(BTreeMap::from([(
                                        "app".to_owned(),
                                        deployment_name.clone(),
                                    )])),
                                    ..Default::default()
                                },
                            ),
                            spec: Some(PodSpec {
                                containers: vec![Container {
                                    name: "pause".to_owned(),
                                    image: Some("registry.k8s.io/pause:3.10".to_owned()),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }),
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create Deployment");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, "Apps & Containers", "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        deployments_resource,
        &fixture.namespace,
    );
    harness
        .get_by_label(&format!("Open details for {deployment_name}"))
        .click();
    harness.run_steps(1);

    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(15) {
        harness.run_steps(1);
        if let Some(panel) = harness.state().ui_state.clusters[&cluster_key]
            .resource_detail_panel
            .as_ref()
            && panel
                .managed_resources
                .iter()
                .any(|resource| resource.api_resource.kind == "ReplicaSet")
            && panel.managed_resources.iter().any(|resource| {
                resource.api_resource.kind == "Pod"
                    && panel
                        .managed_resources
                        .iter()
                        .any(|parent| matches!(
                            &resource.association,
                            crate::resource_detail::ManagedResourceAssociation::ControllerOwnerUid(owner_uid)
                                if parent.uid == *owner_uid
                        ))
            })
        {
            let replica_set = panel
                .managed_resources
                .iter()
                .find(|resource| resource.api_resource.kind == "ReplicaSet")
                .expect("managed ReplicaSet should be present");
            assert!(
                replica_set.cells.contains_key(READY_COLUMN),
                "managed ReplicaSet should include the Ready table value"
            );
            let pod = panel
                .managed_resources
                .iter()
                .find(|resource| resource.api_resource.kind == "Pod")
                .expect("managed Pod should be present");
            assert!(
                pod.cells.contains_key(STATUS_COLUMN),
                "managed Pod should include the Status table value"
            );
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let panel = harness.state().ui_state.clusters[&cluster_key]
        .resource_detail_panel
        .as_ref();
    panic!("Timed out waiting for managed resources: {panel:#?}");
}

/// Verifies that a Node inspector watches Pods cluster-wide and shows the Pods
/// scheduled to the selected Node through the shared inspector table path.
#[test]
fn test_node_inspector_lists_scheduled_pods_integration() {
    let fixture = IntegrationNamespaceFixture::create("node-inspector", "anchor", "unused");
    let pod_name = "node-inspector-pod".to_owned();
    let client = fixture.runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let pods: Api<Pod> = Api::namespaced(client, &fixture.namespace);
    fixture.runtime.block_on(async {
        pods.create(
            &Default::default(),
            &Pod {
                metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                    name: Some(pod_name.clone()),
                    namespace: Some(fixture.namespace.clone()),
                    ..Default::default()
                },
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "pause".to_owned(),
                        image: Some("registry.k8s.io/pause:3.10".to_owned()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to create integration Pod");
    });
    let node_name = fixture.runtime.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Some(node_name) = pods
                    .get(&pod_name)
                    .await
                    .expect("Failed to get integration Pod")
                    .spec
                    .and_then(|spec| spec.node_name)
                {
                    return node_name;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("Timed out waiting for Kubernetes to assign the integration Pod")
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    harness.get_by_label("Nodes").click_accesskit();
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .resource_cache
                .get(&(crate::resource_handlers::node::api_resource(), None))
                .filter(|watch| watch.is_synced)
                .map(|_| ())
        },
        10_000,
    );
    let node_position = harness
        .get_by_label(&format!("Open details for {node_name}"))
        .rect()
        .center();
    harness.event(egui::Event::PointerMoved(node_position));
    harness.event(egui::Event::PointerButton {
        pos: node_position,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    });
    harness.event(egui::Event::PointerButton {
        pos: node_position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::default(),
    });
    harness.run_steps(1);

    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .resource_detail_panel
                .as_ref()
                .and_then(|panel| {
                    panel
                        .managed_resources
                        .iter()
                        .find(|resource| resource.name == pod_name)
                        .filter(|resource| {
                            resource.namespace.as_deref() == Some(&fixture.namespace)
                        })
                        .map(|_| ())
                })
        },
        15_000,
    );
}

/// Creates a ConfigMap, edits it through the UI, and then deletes it through the UI.
#[test]
fn test_resource_actions_integration() {
    let fixture = IntegrationNamespaceFixture::create(
        "resource-actions",
        ACTIONS_CONFIGMAP_NAME,
        "original-value",
    );
    let test_configmap_name = fixture.name.clone();
    let runtime = &fixture.runtime;
    let configmaps = &fixture.configmaps;

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let configmaps_resource = select_resource(&mut harness, "Config", "Config Maps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        configmaps_resource.clone(),
        &fixture.namespace,
    );
    assert!(
        harness.state().ui_state.clusters[&cluster_key].resource_cache
            [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
            .resources
            .values()
            .any(|resource| resource.name == test_configmap_name)
    );

    for _ in 0..3 {
        harness.run_steps(1);
    }
    let actions_label = format!("More actions for {test_configmap_name}");
    harness.get_by_label(&actions_label).click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Edit").click_accesskit();
    harness.run_steps(1);
    wait_for(
        &mut harness,
        |app| {
            app.ui_state
                .yaml_editors
                .values()
                .find(|editor| {
                    editor.resource_name == test_configmap_name
                        && !editor.loading
                        && editor.original_yaml.is_some()
                })
                .map(|_| ())
        },
        5_000,
    );

    let yaml_editor = harness
        .state_mut()
        .ui_state
        .yaml_editors
        .values_mut()
        .find(|editor| editor.resource_name == test_configmap_name)
        .expect("YAML editor should be open");
    yaml_editor.edited_yaml = yaml_editor
        .edited_yaml
        .replace("original-value", "edited-value");
    harness.run_steps(1);
    harness.get_by_label("Apply changes").click();
    harness.run_steps(1);
    wait_for(
        &mut harness,
        |app| {
            app.ui_state
                .yaml_editors
                .values()
                .find(|editor| editor.resource_name == test_configmap_name)
                .is_some_and(|editor| {
                    !editor.loading
                        && editor.original_yaml.is_some()
                        && !editor.is_modified()
                        && !editor.saving
                })
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
        harness.run_steps(1);
    }
    harness.get_by_label(&actions_label).click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Delete").click_accesskit();
    harness.run_steps(1);
    assert!(
        harness.state().ui_state.clusters[&cluster_key]
            .pending_delete
            .as_ref()
            .is_some_and(|pending| pending.resource_name == test_configmap_name)
    );

    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_delete
                .as_ref()
                .filter(|pending| pending.confirmation_available_at <= std::time::Instant::now())
                .map(|_| ())
        },
        5_000,
    );

    let confirm_delete_label = format!("Delete {test_configmap_name}");
    harness
        .get_by_label(&confirm_delete_label)
        .click_accesskit();
    harness.run_steps(1);
    wait_for(
        &mut harness,
        |app| {
            let resources = &app.ui_state.clusters[&cluster_key].resource_cache
                [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
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

/// Deletes two independently selected ConfigMaps through the bulk action.
#[test]
fn test_bulk_resource_delete_integration() {
    let fixture = IntegrationNamespaceFixture::create("bulk-delete", "bulk-delete-a", "first");
    let second_name = "bulk-delete-b".to_owned();
    let runtime = &fixture.runtime;
    let configmaps = &fixture.configmaps;
    runtime.block_on(async {
        configmaps
            .create(
                &Default::default(),
                &ConfigMap {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(second_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    data: Some(BTreeMap::from([(
                        String::from("key1"),
                        String::from("second"),
                    )])),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create second integration ConfigMap");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let configmaps_resource = select_resource(&mut harness, "Config", "Config Maps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        configmaps_resource.clone(),
        &fixture.namespace,
    );
    wait_for(
        &mut harness,
        |app| {
            let resources = &app.ui_state.clusters[&cluster_key].resource_cache
                [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
                .resources;
            (resources
                .values()
                .any(|resource| resource.name == fixture.name)
                && resources
                    .values()
                    .any(|resource| resource.name == second_name))
            .then_some(())
        },
        10_000,
    );

    harness.get_by_label("Select row 1").click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Select row 2").click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Delete selected").click_accesskit();
    harness.run_steps(1);
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_bulk_delete
                .as_ref()
                .filter(|pending| pending.confirmation_available_at <= std::time::Instant::now())
                .map(|_| ())
        },
        5_000,
    );
    harness.get_by_label("Delete 2 resources").click_accesskit();
    harness.run_steps(1);
    wait_for(
        &mut harness,
        |app| {
            let resources = &app.ui_state.clusters[&cluster_key].resource_cache
                [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
                .resources;
            (!resources
                .values()
                .any(|resource| resource.name == fixture.name || resource.name == second_name))
            .then_some(())
        },
        10_000,
    );
    assert!(
        runtime
            .block_on(async { configmaps.get(&fixture.name).await })
            .is_err()
    );
    assert!(
        runtime
            .block_on(async { configmaps.get(&second_name).await })
            .is_err()
    );
}

/// Removes a deliberately stuck ConfigMap's finalizer through the guarded UI action.
#[test]
fn test_force_delete_resource_with_finalizer_integration() {
    let fixture =
        IntegrationNamespaceFixture::create("force-delete", "force-delete-stuck", "value");
    let resource_name = fixture.name.clone();
    let runtime = &fixture.runtime;
    let configmaps = &fixture.configmaps;
    runtime.block_on(async {
        configmaps
            .patch(
                &resource_name,
                &Default::default(),
                &Patch::Merge(&k8s_openapi::serde_json::json!({
                    "metadata": { "finalizers": [TEST_FINALIZER] }
                })),
            )
            .await
            .expect("ConfigMap finalizer should be added");
        configmaps
            .delete(&resource_name, &Default::default())
            .await
            .expect("ConfigMap deletion should be accepted");
        let configmap = configmaps
            .get(&resource_name)
            .await
            .expect("Finalizer should keep ConfigMap present");
        assert!(configmap.metadata.deletion_timestamp.is_some());
        assert!(
            configmap
                .metadata
                .finalizers
                .as_ref()
                .is_some_and(|finalizers| finalizers == &[TEST_FINALIZER])
        );
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let configmaps_resource = select_resource(&mut harness, "Config", "Config Maps");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        configmaps_resource.clone(),
        &fixture.namespace,
    );
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key].resource_cache
                [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
                .resources
                .values()
                .find(|resource| resource.name == resource_name)
                .filter(|resource| resource.can_force_delete())
                .map(|_| ())
        },
        10_000,
    );

    harness
        .get_by_label(&format!("More actions for {resource_name}"))
        .click_accesskit();
    harness.run_steps(1);
    harness
        .get_by_label("Force delete (remove finalizers)")
        .click_accesskit();
    harness.run_steps(1);
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_force_delete
                .as_ref()
                .filter(|pending| pending.confirmation_available_at <= std::time::Instant::now())
                .map(|_| ())
        },
        5_000,
    );
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::TextInput,
            &format!("Type {resource_name} to acknowledge that you are bypassing cleanup:"),
        )
        .click();
    harness.run_steps(1);
    harness
        .input_mut()
        .events
        .push(egui::Event::Text(resource_name.clone()));
    harness.run_steps(1);
    harness.get_by_label("Remove finalizers").click_accesskit();
    harness.run_steps(1);

    wait_for(
        &mut harness,
        |app| {
            (!app.ui_state.clusters[&cluster_key].resource_cache
                [&(configmaps_resource.clone(), Some(fixture.namespace.clone()))]
                .resources
                .values()
                .any(|resource| resource.name == resource_name))
            .then_some(())
        },
        10_000,
    );
    assert!(
        runtime
            .block_on(async { configmaps.get(&resource_name).await })
            .is_err()
    );
}

/// Fetches the live Deployment OpenAPI schema from Kind and verifies completion inside an
/// existing `spec.selector.matchLabels` key after it has been partially edited.
#[test]
fn test_deployment_match_labels_completion_integration() {
    let fixture = IntegrationNamespaceFixture::create("deployment-completion", "anchor", "unused");
    let deployment_name = "deployment-completion".to_owned();
    let client = fixture.runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let deployments: Api<Deployment> = Api::namespaced(client, &fixture.namespace);
    fixture.runtime.block_on(async {
        deployments
            .create(
                &Default::default(),
                &Deployment {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(deployment_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    spec: Some(DeploymentSpec {
                        replicas: Some(1),
                        selector: LabelSelector {
                            match_labels: Some(BTreeMap::from([(
                                "app".to_owned(),
                                deployment_name.clone(),
                            )])),
                            ..Default::default()
                        },
                        template: PodTemplateSpec {
                            metadata: Some(
                                k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                                    labels: Some(BTreeMap::from([(
                                        "app".to_owned(),
                                        deployment_name.clone(),
                                    )])),
                                    ..Default::default()
                                },
                            ),
                            spec: Some(PodSpec {
                                containers: vec![Container {
                                    name: "pause".to_owned(),
                                    image: Some("registry.k8s.io/pause:3.10".to_owned()),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }),
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create integration Deployment");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, "Apps & Containers", "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        deployments_resource,
        &fixture.namespace,
    );

    let actions_label = format!("More actions for {deployment_name}");
    harness.get_by_label(&actions_label).click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Edit").click_accesskit();
    harness.run_steps(1);
    let (schema, yaml) = wait_for(
        &mut harness,
        |app| {
            app.ui_state
                .yaml_editors
                .values()
                .find(|editor| {
                    editor.resource_name == deployment_name
                        && !editor.loading
                        && editor.original_yaml.is_some()
                })
                .and_then(|editor| {
                    editor
                        .schema
                        .clone()
                        .map(|schema| (schema, editor.edited_yaml.clone()))
                })
        },
        10_000,
    );

    let key_start = yaml
        .find("matchLabels")
        .expect("live Deployment YAML includes spec.selector.matchLabels");
    let mut partial_yaml = yaml;
    partial_yaml.replace_range(key_start..key_start + "matchLabels".len(), "match");
    let cursor = partial_yaml[..key_start + "match".len()].chars().count();
    let suggestions = schema.completion_at(&partial_yaml, cursor).suggestions;

    assert_eq!(
        suggestions
            .first()
            .map(|suggestion| suggestion.label.as_str()),
        Some("matchLabels"),
        "suggestions: {suggestions:#?}\npartial YAML:\n{partial_yaml}"
    );

    let affinity_yaml = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: deployment-completion
spec:
  template:
    spec:
      affinity:
        podAntiAffinity:
          preferredDuringSchedulingIgnoredDuringExecution:
          - podAffinityTerm:
              labelSelector:
                matchExpressions:
                - key: k8s-app
                  operator: I"#;
    let suggestions = schema
        .completion_at(affinity_yaml, affinity_yaml.len())
        .suggestions;
    assert_eq!(
        suggestions
            .first()
            .map(|suggestion| suggestion.label.as_str()),
        Some("In"),
        "suggestions: {suggestions:#?}\nYAML:\n{affinity_yaml}"
    );
}

/// Opens the installed CoreDNS Deployment through the real editor and checks the completion
/// context at every mapping key in its live YAML.
#[test]
fn test_coredns_deployment_property_completion_integration() {
    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, "kube-system");
    let deployments_resource = select_resource(&mut harness, "Apps & Containers", "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        deployments_resource,
        "kube-system",
    );

    harness
        .get_by_label("More actions for coredns")
        .click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Edit").click_accesskit();
    harness.run_steps(1);
    let (schema, yaml) = wait_for(
        &mut harness,
        |app| {
            app.ui_state
                .yaml_editors
                .values()
                .find(|editor| {
                    editor.resource_name == "coredns"
                        && !editor.loading
                        && editor.original_yaml.is_some()
                })
                .and_then(|editor| {
                    editor
                        .schema
                        .clone()
                        .map(|schema| (schema, editor.edited_yaml.clone()))
                })
        },
        10_000,
    );

    let failures = yaml_mapping_key_positions(&yaml)
        .into_iter()
        .filter_map(|(line, key, cursor)| {
            let completion = schema.completion_at(&yaml, cursor);
            completion
                .context
                .is_none()
                .then_some((line, key, completion.suggestions))
        })
        .collect::<Vec<_>>();

    assert!(
        failures.is_empty(),
        "each CoreDNS mapping key should resolve to a schema completion context:\n{failures:#?}\nYAML:\n{yaml}"
    );
}

fn yaml_mapping_key_positions(yaml: &str) -> Vec<(usize, String, usize)> {
    let mut line_start = 0;
    let mut positions = Vec::new();
    for (line_number, line) in yaml.lines().enumerate() {
        let leading_whitespace = line.len() - line.trim_start().len();
        let line_after_indent = &line[leading_whitespace..];
        let (dash_prefix, mapping) = line_after_indent
            .strip_prefix("- ")
            .map_or((0, line_after_indent), |mapping| (2, mapping));
        if let Some((key, _)) = mapping.split_once(':')
            && !key.is_empty()
            && key.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            let cursor = line_start + leading_whitespace + dash_prefix + key.len();
            positions.push((line_number + 1, key.to_owned(), cursor));
        }
        line_start += line.len() + 1;
    }
    positions
}

/// Verifies that the Deployment action patches the pod template annotation used
/// by `kubectl rollout restart` against a real Kubernetes API server.
#[test]
fn test_deployment_rollout_restart_integration() {
    let fixture =
        IntegrationNamespaceFixture::create("deployment-rollout-restart", "anchor", "unused");
    let deployment_name = "restartable-deployment".to_owned();
    let runtime = &fixture.runtime;
    let client = runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let deployments: Api<Deployment> = Api::namespaced(client, &fixture.namespace);
    runtime.block_on(async {
        deployments
            .create(
                &Default::default(),
                &Deployment {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(deployment_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    spec: Some(DeploymentSpec {
                        replicas: Some(1),
                        selector: LabelSelector {
                            match_labels: Some(BTreeMap::from([(
                                "app".to_owned(),
                                deployment_name.clone(),
                            )])),
                            ..Default::default()
                        },
                        template: PodTemplateSpec {
                            metadata: Some(
                                k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                                    labels: Some(BTreeMap::from([(
                                        "app".to_owned(),
                                        deployment_name.clone(),
                                    )])),
                                    ..Default::default()
                                },
                            ),
                            spec: Some(PodSpec {
                                containers: vec![Container {
                                    name: "pause".to_owned(),
                                    image: Some("registry.k8s.io/pause:3.10".to_owned()),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }),
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create Deployment");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, "Apps & Containers", "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        deployments_resource,
        &fixture.namespace,
    );
    for _ in 0..3 {
        harness.run_steps(1);
    }
    let actions_label = format!("More actions for {deployment_name}");
    harness.get_by_label(&actions_label).click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Restart rollout").click_accesskit();
    // The action menu and confirmation dialog use the same label while the
    // menu blade animates out. Advance its bounded transition before querying
    // the confirmation button.
    harness.run_steps(2);
    harness.get_by_label("Restart rollout").click_accesskit();
    harness.run_steps(1);

    wait_for(
        &mut harness,
        |_| {
            runtime
                .block_on(async { deployments.get(&deployment_name).await })
                .ok()
                .and_then(|deployment| {
                    deployment
                        .spec
                        .and_then(|spec| spec.template.metadata)
                        .and_then(|metadata| metadata.annotations)
                        .and_then(|annotations| {
                            annotations
                                .get("kubectl.kubernetes.io/restartedAt")
                                .cloned()
                        })
                })
                .filter(|timestamp| {
                    time::OffsetDateTime::parse(
                        timestamp,
                        &time::format_description::well_known::Rfc3339,
                    )
                    .is_ok()
                })
                .map(|_| ())
        },
        10_000,
    );
}

/// Verifies that the generic Scale action uses the discovered Deployment scale endpoint.
#[test]
fn test_resource_scale_integration() {
    let fixture = IntegrationNamespaceFixture::create("resource-scale", "anchor", "unused");
    let deployment_name = "scalable-deployment".to_owned();
    let runtime = &fixture.runtime;
    let client = runtime.block_on(async {
        Client::try_default()
            .await
            .expect("Failed to create Kubernetes client")
    });
    let deployments: Api<Deployment> = Api::namespaced(client, &fixture.namespace);
    runtime.block_on(async {
        deployments
            .create(
                &Default::default(),
                &Deployment {
                    metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                        name: Some(deployment_name.clone()),
                        namespace: Some(fixture.namespace.clone()),
                        ..Default::default()
                    },
                    spec: Some(DeploymentSpec {
                        replicas: Some(1),
                        selector: LabelSelector {
                            match_labels: Some(BTreeMap::from([(
                                "app".to_owned(),
                                deployment_name.clone(),
                            )])),
                            ..Default::default()
                        },
                        template: PodTemplateSpec {
                            metadata: Some(
                                k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                                    labels: Some(BTreeMap::from([(
                                        "app".to_owned(),
                                        deployment_name.clone(),
                                    )])),
                                    ..Default::default()
                                },
                            ),
                            spec: Some(PodSpec {
                                containers: vec![Container {
                                    name: "pause".to_owned(),
                                    image: Some("registry.k8s.io/pause:3.10".to_owned()),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }),
                        },
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to create Deployment");
    });

    let (mut harness, cluster_key) = connected_kind_harness();
    wait_for_cluster_data(&mut harness, cluster_key);
    select_namespace(&mut harness, cluster_key, &fixture.namespace);
    let deployments_resource = select_resource(&mut harness, "Apps & Containers", "Deployments");
    wait_for_resource_sync(
        &mut harness,
        cluster_key,
        deployments_resource,
        &fixture.namespace,
    );
    for _ in 0..3 {
        harness.run_steps(1);
    }
    let actions_label = format!("More actions for {deployment_name}");
    harness.get_by_label(&actions_label).click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Scale").click_accesskit();
    wait_for(
        &mut harness,
        |app| {
            app.ui_state.clusters[&cluster_key]
                .pending_scale
                .as_ref()
                .map(|_| ())
        },
        10_000,
    );
    harness
        .get_by_label("Increase desired replicas")
        .click_accesskit();
    harness.run_steps(1);
    harness.get_by_label("Update scale").click_accesskit();
    harness.run_steps(1);

    wait_for(
        &mut harness,
        |_| {
            runtime
                .block_on(async { deployments.get(&deployment_name).await })
                .ok()
                .and_then(|deployment| deployment.spec.and_then(|spec| spec.replicas))
                .filter(|replicas| *replicas == 2)
                .map(|_| ())
        },
        10_000,
    );
}
