use super::super::super::MyEguiApp;
use super::super::fixtures::application_harness;
use crate::api_resource::ApiResource;
use crate::sorted_name::SortedName;
use crate::worker::{Worker, WorkerTrait};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use k8s_openapi::api::core::v1::{ConfigMap, Namespace, Secret};
use kube::api::Patch;
use kube::{Api, Client};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

mod waits;
pub(super) use waits::*;

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

pub(super) fn connected_kind_harness() -> (Harness<'static, MyEguiApp<Worker>>, i32) {
    let mut harness = application_harness::<Worker>();
    wait_for(
        &mut harness,
        "the kubeconfig contexts to load",
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
    wait_for_with_terminal_and_timeout_diagnostic(
        harness,
        "the Kind connection, namespaces, and Kubernetes APIs to load",
        |app| {
            app.ui_state.clusters.get(&cluster_key).and_then(|cluster| {
                (!cluster.namespaces.is_empty()
                    && (!cluster.resource_navigation.curated_entries.is_empty()
                        || !cluster.resource_navigation.other_api_groups.is_empty()))
                .then_some(())
            })
        },
        |app| cluster_load_failure(&app.ui_state, cluster_key),
        |app| cluster_load_state(&app.ui_state, cluster_key),
        10_000,
    );
}

pub(super) fn select_namespace(
    harness: &mut Harness<MyEguiApp<Worker>>,
    cluster_key: i32,
    namespace: &str,
) {
    let namespace = namespace.to_owned();
    wait_for_with_terminal_and_timeout_diagnostic(
        harness,
        &format!("namespace {namespace} to appear in the Kind namespace watch"),
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
        |app| cluster_load_failure(&app.ui_state, cluster_key),
        |app| cluster_load_state(&app.ui_state, cluster_key),
        10_000,
    );
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespace")
        .click();
    let search_label = "Search Namespace";
    wait_for_harness(
        harness,
        "the namespace search input to receive focus",
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
        &format!("the namespace search input to contain {namespace}"),
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
        &format!("namespace {namespace} to become selected"),
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
    api_resource: &ApiResource,
    namespace: Option<&str>,
) {
    wait_for_resource_watch(
        harness,
        &format!(
            "the {} watcher to synchronize in {}",
            api_resource.kind,
            namespace.unwrap_or("cluster scope")
        ),
        cluster_key,
        api_resource,
        namespace,
        |state| state.is_synced.then_some(()),
        10_000,
    );
}

pub(super) fn open_resource_detail<W: WorkerTrait>(
    harness: &mut Harness<MyEguiApp<W>>,
    cluster_key: i32,
    resource_name: &str,
    namespace: Option<&str>,
) -> u64 {
    // Resource synchronization can change the table during the frame which observes it.
    // Render once more before resolving the pointer target so the click uses settled geometry.
    harness.run_steps(1);
    let previous_history_entry = harness
        .state()
        .ui_state
        .global_blades
        .navigator()
        .and_then(|navigator| navigator.current().resource_detail())
        .map(|entry| (entry.cluster_key, entry.history_entry_id));
    harness
        .get_by_label(&format!("Open details for {resource_name}"))
        .click();
    harness.run_steps(1);
    harness
        .state()
        .ui_state
        .global_blades
        .navigator()
        .and_then(|navigator| navigator.current().resource_detail())
        .filter(|entry| {
            entry.cluster_key == cluster_key
                && entry.resource_name == resource_name
                && entry.namespace.as_deref() == namespace
                && Some((entry.cluster_key, entry.history_entry_id)) != previous_history_entry
        })
        .map(|entry| entry.history_entry_id)
        .unwrap_or_else(|| {
            let diagnostic = resource_detail_state(
                &harness.state().ui_state,
                cluster_key,
                resource_name,
                namespace,
                None,
            )
            .unwrap_or_else(|| "resource inspector state is unavailable".to_owned());
            panic!("Failed to open the {resource_name} resource inspector: {diagnostic}")
        })
}

pub(super) fn wait_for_data_editor(
    harness: &mut Harness<MyEguiApp<Worker>>,
    cluster_key: i32,
    history_entry_id: u64,
    data_key: &str,
) {
    wait_for_with_diagnostic(
        harness,
        &format!("the resource data editor to load key {data_key}"),
        |app| {
            current_resource_detail(&app.ui_state, cluster_key, history_entry_id)
                .and_then(|entry| entry.data_editor.as_ref())
                .filter(|editor| editor.draft_values.contains_key(data_key))
                .map(|_| ())
        },
        |app| {
            current_resource_detail(&app.ui_state, cluster_key, history_entry_id)
                .and_then(|entry| entry.detail_error.as_ref())
                .map(|error| {
                    format!("Resource detail watch failed while loading data editor: {error}")
                })
        },
        10_000,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::fixtures::oracle_resource_table_state;
    use crate::ui::tests::harness::MockUiHarnessExt;
    use crate::worker::{KubernetesResourcesReplaced, MockWorker};

    #[test]
    fn opening_resource_detail_resolves_the_pointer_target_after_a_resource_update() {
        let mut harness = application_harness::<MockWorker>();
        harness.seed_ui_state(oracle_resource_table_state());
        let cluster_key = 2;
        let namespace = "kube-system";
        let resource_name = "coredns-66bc5c9577-ffw2s";
        let api_resource = harness.state().ui_state.clusters[&cluster_key]
            .selected_api_resource
            .clone()
            .expect("Pod resource should be selected");
        let watch_key = (api_resource.clone(), Some(namespace.to_owned()));
        let mut resources = harness.state().ui_state.clusters[&cluster_key].resource_cache
            [&watch_key]
            .resources
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut inserted = resources
            .first()
            .cloned()
            .expect("fixture should contain resources");
        inserted.uid = "fixture-inserted-before-target".to_owned();
        inserted.name = "aaa-inserted-before-target".to_owned();
        resources.push(inserted);

        harness
            .state_mut()
            .worker
            .enqueue_result(KubernetesResourcesReplaced {
                cluster_key,
                api_resource,
                namespace: Some(namespace.to_owned()),
                resources,
            });

        let history_entry_id =
            open_resource_detail(&mut harness, cluster_key, resource_name, Some(namespace));

        let entry =
            current_resource_detail(&harness.state().ui_state, cluster_key, history_entry_id)
                .expect("the intended Pod inspector should open");
        assert_eq!(entry.resource_name, resource_name);
    }
}
