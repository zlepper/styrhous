use super::super::super::MyEguiApp;
use super::super::fixtures::application_harness;
use crate::api_resource::ApiResource;
use crate::sorted_name::SortedName;
use crate::worker::Worker;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use k8s_openapi::api::core::v1::{ConfigMap, Namespace, Secret};
use kube::api::Patch;
use kube::{Api, Client};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const TEST_NAMESPACE_PREFIX: &str = "styrhous-it-";
const FIXTURE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct IntegrationNamespaceFixture {
    pub(super) runtime: tokio::runtime::Runtime,
    namespaces: Api<Namespace>,
    pub(super) configmaps: Api<ConfigMap>,
    pub(super) secrets: Api<Secret>,
    pub(super) namespace: String,
    pub(super) name: String,
}

impl IntegrationNamespaceFixture {
    pub(super) fn create(test_name: &str, name: &str, value: &str) -> Self {
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

impl Drop for IntegrationNamespaceFixture {
    fn drop(&mut self) {
        let cleanup_result = self.runtime.block_on(async {
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

#[track_caller]
pub(super) fn wait_for<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    condition: impl Fn(&MyEguiApp<Worker>) -> Option<T>,
    max_ms: u64,
) -> T {
    wait_for_with_diagnostic(harness, condition, |_| None, max_ms)
}

#[track_caller]
pub(super) fn wait_for_with_diagnostic<T>(
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

#[track_caller]
pub(super) fn wait_for_harness<T>(
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
    panic!(
        "Timed out after {max_ms}ms waiting for UI state (requested at {})",
        std::panic::Location::caller(),
    );
}

pub(super) fn connected_kind_harness() -> (Harness<'static, MyEguiApp<Worker>>, i32) {
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

pub(super) fn wait_for_cluster_data(harness: &mut Harness<MyEguiApp<Worker>>, cluster_key: i32) {
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

pub(super) fn select_namespace(
    harness: &mut Harness<MyEguiApp<Worker>>,
    cluster_key: i32,
    namespace: &str,
) {
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
    let search_label = "Search Namespace";
    wait_for_harness(
        harness,
        |harness| {
            harness
                .query_by_role_and_label(egui::accesskit::Role::TextInput, search_label)
                .filter(|input| input.is_focused())
                .map(|_| ())
        },
        10_000,
    );
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, search_label)
        .type_text(&namespace);
    wait_for_harness(
        harness,
        |harness| {
            harness
                .query_by_role_and_label(egui::accesskit::Role::TextInput, search_label)
                .filter(|input| input.value().as_deref() == Some(namespace.as_str()))
                .map(|_| ())
        },
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

pub(super) fn select_resource(
    harness: &mut Harness<MyEguiApp<Worker>>,
    section: &str,
    resource_name: &str,
) -> ApiResource {
    harness.get_by_label(section).click_accesskit();
    harness.run_steps(2);
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

pub(super) fn wait_for_resource_sync(
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

pub(super) fn wait_for_data_editor(
    harness: &mut Harness<MyEguiApp<Worker>>,
    cluster_key: i32,
    data_key: &str,
) {
    wait_for_with_diagnostic(
        harness,
        |app| {
            app.ui_state
                .global_blades
                .navigator()
                .and_then(|navigator| navigator.current().resource_detail())
                .filter(|entry| entry.cluster_key == cluster_key)
                .and_then(|entry| entry.data_editor.as_ref())
                .filter(|editor| editor.draft_values.contains_key(data_key))
                .map(|_| ())
        },
        |app| {
            app.ui_state
                .global_blades
                .navigator()
                .and_then(|navigator| navigator.current().resource_detail())
                .filter(|entry| entry.cluster_key == cluster_key)
                .and_then(|entry| entry.detail_error.as_ref())
                .map(|error| {
                    format!("Resource detail watch failed while loading data editor: {error}")
                })
        },
        10_000,
    );
}
