use crate::api_resource::ApiResource;
use crate::resource_schema::ResourceSchema;
use crate::ui::MyEguiApp;
use crate::ui::state::{
    ClusterConnectionState, ClusterLoadState, ResourceDetailHistoryEntry, ResourceWatchKey,
    ResourceWatchState, UiState, ValidationState, YamlEditorWindowState,
};
use crate::worker::Worker;
use egui_kittest::Harness;
use std::time::{Duration, Instant};

const KUBERNETES_REQUEST_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(1);

fn kubernetes_request_timeout(remaining: Duration) -> Duration {
    remaining.min(KUBERNETES_REQUEST_ATTEMPT_TIMEOUT)
}

#[track_caller]
pub(in crate::ui::tests::kind) fn wait_for<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    expectation: &str,
    condition: impl FnMut(&MyEguiApp<Worker>) -> Option<T>,
    max_ms: u64,
) -> T {
    wait_for_with_diagnostics(harness, expectation, condition, |_| None, |_| None, max_ms)
}

#[track_caller]
pub(in crate::ui::tests::kind) fn wait_for_with_diagnostic<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    expectation: &str,
    condition: impl FnMut(&MyEguiApp<Worker>) -> Option<T>,
    diagnostic: impl Fn(&MyEguiApp<Worker>) -> Option<String>,
    max_ms: u64,
) -> T {
    wait_for_with_diagnostics(
        harness,
        expectation,
        condition,
        diagnostic,
        |_| None,
        max_ms,
    )
}

#[track_caller]
pub(in crate::ui::tests::kind) fn wait_for_with_terminal_and_timeout_diagnostic<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    expectation: &str,
    condition: impl FnMut(&MyEguiApp<Worker>) -> Option<T>,
    terminal_diagnostic: impl Fn(&MyEguiApp<Worker>) -> Option<String>,
    timeout_diagnostic: impl Fn(&MyEguiApp<Worker>) -> Option<String>,
    max_ms: u64,
) -> T {
    wait_for_with_diagnostics(
        harness,
        expectation,
        condition,
        terminal_diagnostic,
        timeout_diagnostic,
        max_ms,
    )
}

#[track_caller]
fn wait_for_with_diagnostics<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    expectation: &str,
    mut condition: impl FnMut(&MyEguiApp<Worker>) -> Option<T>,
    terminal_diagnostic: impl Fn(&MyEguiApp<Worker>) -> Option<String>,
    timeout_diagnostic: impl Fn(&MyEguiApp<Worker>) -> Option<String>,
    max_ms: u64,
) -> T {
    let start = Instant::now();
    while start.elapsed().as_millis() < max_ms as u128 {
        harness.run_steps(1);
        let app = harness.state();
        if let Some(message) = terminal_diagnostic(app) {
            panic!("Failed while waiting for {expectation}: {message}");
        }
        if let Some(result) = condition(app) {
            return result;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let diagnostic = timeout_diagnostic(harness.state())
        .map(|message| format!("\nLast observed state: {message}"))
        .unwrap_or_default();
    panic!(
        "Timed out after {max_ms}ms waiting for {expectation} (requested at {}){diagnostic}",
        std::panic::Location::caller(),
    );
}

#[track_caller]
pub(in crate::ui::tests::kind) fn wait_for_harness<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    expectation: &str,
    mut condition: impl FnMut(&mut Harness<MyEguiApp<Worker>>) -> Option<T>,
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
        "Timed out after {max_ms}ms waiting for {expectation} (requested at {})",
        std::panic::Location::caller(),
    );
}

#[track_caller]
pub(in crate::ui::tests::kind) fn wait_for_kubernetes<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    expectation: &str,
    condition: impl FnMut(Duration) -> Result<Option<T>, KubernetesWaitError>,
    max_ms: u64,
) -> T {
    wait_for_kubernetes_with_diagnostic(harness, expectation, condition, |_| None, max_ms)
}

#[derive(Debug)]
pub(in crate::ui::tests::kind) enum KubernetesWaitError {
    Api(kube::Error),
    RequestTimedOut(Duration),
}

impl std::fmt::Display for KubernetesWaitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(error) => write!(formatter, "{error:#}"),
            Self::RequestTimedOut(timeout) => {
                write!(formatter, "request did not complete within {timeout:?}")
            }
        }
    }
}

pub(in crate::ui::tests::kind) fn kubernetes_request<T>(
    runtime: &tokio::runtime::Runtime,
    timeout: Duration,
    request: impl std::future::Future<Output = Result<T, kube::Error>>,
) -> Result<T, KubernetesWaitError> {
    match runtime.block_on(async { tokio::time::timeout(timeout, request).await }) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => Err(KubernetesWaitError::Api(error)),
        Err(_) => Err(KubernetesWaitError::RequestTimedOut(timeout)),
    }
}

pub(in crate::ui::tests::kind) fn kubernetes_object_absent<T>(
    result: Result<T, KubernetesWaitError>,
) -> Result<Option<()>, KubernetesWaitError> {
    match result {
        Err(KubernetesWaitError::Api(kube::Error::Api(response))) if response.code == 404 => {
            Ok(Some(()))
        }
        Err(error) => Err(error),
        Ok(_) => Ok(None),
    }
}

#[track_caller]
pub(in crate::ui::tests::kind) fn wait_for_kubernetes_with_diagnostic<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    expectation: &str,
    mut condition: impl FnMut(Duration) -> Result<Option<T>, KubernetesWaitError>,
    terminal_diagnostic: impl Fn(&MyEguiApp<Worker>) -> Option<String>,
    max_ms: u64,
) -> T {
    let deadline = Instant::now() + Duration::from_millis(max_ms);
    let mut last_api_error = None;
    loop {
        harness.run_steps(1);
        let app = harness.state();
        if let Some(message) = terminal_diagnostic(app) {
            panic!("Failed while waiting for {expectation}: {message}");
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        let request_timeout = kubernetes_request_timeout(remaining);
        match condition(request_timeout) {
            Ok(Some(result)) => return result,
            Ok(None) => last_api_error = None,
            Err(error) => {
                last_api_error = Some(error.to_string());
            }
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        std::thread::sleep(Duration::from_millis(50).min(remaining));
    }
    let diagnostic = last_api_error
        .map(|error| {
            format!("\nLast observed state: the latest Kubernetes API request failed: {error}")
        })
        .unwrap_or_default();
    panic!(
        "Timed out after {max_ms}ms waiting for {expectation} (requested at {}){diagnostic}",
        std::panic::Location::caller(),
    );
}

pub(in crate::ui::tests::kind) fn cluster_load_failure(
    ui_state: &UiState,
    cluster_key: i32,
) -> Option<String> {
    let cluster = ui_state.clusters.get(&cluster_key)?;
    match &cluster.connection {
        ClusterConnectionState::Failed(error) => {
            return Some(format!("cluster connection failed: {error}"));
        }
        ClusterConnectionState::Disconnected => {
            return Some("cluster became disconnected".to_owned());
        }
        ClusterConnectionState::Connecting | ClusterConnectionState::Connected => {}
    }
    if let ClusterLoadState::Failed(error) = &cluster.namespaces_load {
        return Some(format!("namespace discovery failed: {error}"));
    }
    if let ClusterLoadState::Failed(error) = &cluster.api_resources_load {
        return Some(format!("Kubernetes API discovery failed: {error}"));
    }
    None
}

pub(in crate::ui::tests::kind) fn cluster_load_state(
    ui_state: &UiState,
    cluster_key: i32,
) -> Option<String> {
    let Some(cluster) = ui_state.clusters.get(&cluster_key) else {
        return Some(format!("cluster {cluster_key} is no longer present"));
    };
    Some(format!(
        "connection={:?}, namespaces={:?} ({} loaded), APIs={:?} ({} navigation entries loaded)",
        cluster.connection,
        cluster.namespaces_load,
        cluster.namespaces.len(),
        cluster.api_resources_load,
        cluster.resource_navigation.curated_entries.len()
            + cluster.resource_navigation.other_api_groups.len(),
    ))
}

pub(in crate::ui::tests::kind) fn resource_detail_state(
    ui_state: &UiState,
    cluster_key: i32,
    resource_name: &str,
    namespace: Option<&str>,
    history_entry_id: Option<u64>,
) -> Option<String> {
    let Some(navigator) = ui_state.global_blades.navigator() else {
        return Some(format!(
            "no global blade is open; resource detail panel present={}",
            ui_state
                .clusters
                .get(&cluster_key)
                .is_some_and(|cluster| cluster.resource_detail_panel.is_some())
        ));
    };
    let Some(entry) = navigator.current().resource_detail() else {
        return Some("the current global blade is not a resource inspector".to_owned());
    };
    Some(format!(
        "current inspector is {} in cluster {} and {} with history entry {} (expected {} in cluster {} and {}{})",
        entry.resource_name,
        entry.cluster_key,
        entry.namespace.as_deref().unwrap_or("cluster scope"),
        entry.history_entry_id,
        resource_name,
        cluster_key,
        namespace.unwrap_or("cluster scope"),
        history_entry_id.map_or_else(String::new, |id| format!(" with history entry {id}")),
    ))
}

pub(in crate::ui::tests::kind) fn current_resource_detail(
    ui_state: &UiState,
    cluster_key: i32,
    history_entry_id: u64,
) -> Option<&ResourceDetailHistoryEntry> {
    ui_state
        .global_blades
        .navigator()?
        .current()
        .resource_detail()
        .filter(|entry| {
            entry.cluster_key == cluster_key && entry.history_entry_id == history_entry_id
        })
}

fn resource_watch<'a>(
    ui_state: &'a UiState,
    cluster_key: i32,
    watch_key: &ResourceWatchKey,
) -> Option<&'a ResourceWatchState> {
    ui_state
        .clusters
        .get(&cluster_key)
        .and_then(|cluster| cluster.resource_cache.get(watch_key))
}

fn resource_watch_failure(
    ui_state: &UiState,
    cluster_key: i32,
    watch_key: &ResourceWatchKey,
) -> Option<String> {
    resource_watch(ui_state, cluster_key, watch_key)
        .and_then(|watch| watch.error.as_ref())
        .map(|error| {
            format!(
                "{} watcher failed in {}: {error}",
                watch_key.0.kind,
                watch_key.1.as_deref().unwrap_or("cluster scope")
            )
        })
}

fn resource_wait_failure(
    ui_state: &UiState,
    cluster_key: i32,
    watch_key: &ResourceWatchKey,
) -> Option<String> {
    cluster_load_failure(ui_state, cluster_key)
        .or_else(|| resource_watch_failure(ui_state, cluster_key, watch_key))
}

fn resource_wait_state(
    ui_state: &UiState,
    cluster_key: i32,
    watch_key: &ResourceWatchKey,
) -> Option<String> {
    let Some(watch) = resource_watch(ui_state, cluster_key, watch_key) else {
        return Some(format!(
            "the {} watcher has not been initialized in {}",
            watch_key.0.kind,
            watch_key.1.as_deref().unwrap_or("cluster scope")
        ));
    };
    Some(format!(
        "{} watcher synchronized={}, resources={}, error={:?}",
        watch_key.0.kind,
        watch.is_synced,
        watch.resources.len(),
        watch.error,
    ))
}

pub(in crate::ui::tests::kind) fn wait_for_resource_watch<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    expectation: &str,
    cluster_key: i32,
    api_resource: &ApiResource,
    namespace: Option<&str>,
    mut condition: impl FnMut(&ResourceWatchState) -> Option<T>,
    max_ms: u64,
) -> T {
    let watch_key = (api_resource.clone(), namespace.map(str::to_owned));
    wait_for_with_terminal_and_timeout_diagnostic(
        harness,
        expectation,
        |app| resource_watch(&app.ui_state, cluster_key, &watch_key).and_then(&mut condition),
        |app| resource_wait_failure(&app.ui_state, cluster_key, &watch_key),
        |app| resource_wait_state(&app.ui_state, cluster_key, &watch_key),
        max_ms,
    )
}

fn yaml_editor<'a>(
    ui_state: &'a UiState,
    resource_name: &str,
) -> Option<&'a YamlEditorWindowState> {
    ui_state
        .yaml_editors
        .values()
        .find(|editor| editor.resource_name == resource_name)
}

fn yaml_editor_request_failure(ui_state: &UiState, resource_name: &str) -> Option<String> {
    let editor = yaml_editor(ui_state, resource_name)?;
    editor
        .error
        .as_ref()
        .map(|error| format!("YAML request failed for {resource_name}: {error}"))
}

fn yaml_editor_schema_failure(ui_state: &UiState, resource_name: &str) -> Option<String> {
    let editor = yaml_editor(ui_state, resource_name)?;
    match &editor.server_validation {
        ValidationState::Failed(error) if editor.schema.is_none() && !editor.schema_loading => {
            Some(format!(
                "OpenAPI schema request failed for {resource_name}: {error}"
            ))
        }
        _ => None,
    }
}

fn yaml_editor_state(ui_state: &UiState, resource_name: &str) -> Option<String> {
    let Some(editor) = yaml_editor(ui_state, resource_name) else {
        return Some(format!("the {resource_name} editor was never opened"));
    };
    Some(format!(
        "editor loading={}, YAML loaded={}, schema loading={}, schema loaded={}, saving={}, modified={}",
        editor.loading,
        editor.original_yaml.is_some(),
        editor.schema_loading,
        editor.schema.is_some(),
        editor.saving,
        editor.is_modified(),
    ))
}

pub(in crate::ui::tests::kind) fn wait_for_yaml_editor(
    harness: &mut Harness<MyEguiApp<Worker>>,
    resource_name: &str,
    max_ms: u64,
) -> u64 {
    wait_for_with_diagnostics(
        harness,
        &format!("the {resource_name} YAML editor to load"),
        |app| {
            yaml_editor(&app.ui_state, resource_name)
                .filter(|editor| !editor.loading && editor.original_yaml.is_some())
                .map(|editor| editor.id)
        },
        |app| yaml_editor_request_failure(&app.ui_state, resource_name),
        |app| yaml_editor_state(&app.ui_state, resource_name),
        max_ms,
    )
}

pub(in crate::ui::tests::kind) fn wait_for_yaml_editor_with_schema(
    harness: &mut Harness<MyEguiApp<Worker>>,
    resource_name: &str,
    max_ms: u64,
) -> (ResourceSchema, String) {
    wait_for_with_diagnostics(
        harness,
        &format!("the {resource_name} YAML editor and OpenAPI schema to load"),
        |app| {
            yaml_editor(&app.ui_state, resource_name)
                .filter(|editor| !editor.loading && editor.original_yaml.is_some())
                .and_then(|editor| {
                    editor
                        .schema
                        .clone()
                        .map(|schema| (schema, editor.edited_yaml.clone()))
                })
        },
        |app| {
            yaml_editor_request_failure(&app.ui_state, resource_name)
                .or_else(|| yaml_editor_schema_failure(&app.ui_state, resource_name))
        },
        |app| yaml_editor_state(&app.ui_state, resource_name),
        max_ms,
    )
}

pub(in crate::ui::tests::kind) fn wait_for_yaml_editor_saved(
    harness: &mut Harness<MyEguiApp<Worker>>,
    resource_name: &str,
    max_ms: u64,
) {
    wait_for_with_diagnostics(
        harness,
        &format!("the {resource_name} YAML editor to finish saving"),
        |app| {
            yaml_editor(&app.ui_state, resource_name)
                .filter(|editor| {
                    !editor.loading
                        && editor.original_yaml.is_some()
                        && !editor.is_modified()
                        && !editor.saving
                })
                .map(|_| ())
        },
        |app| yaml_editor_request_failure(&app.ui_state, resource_name),
        |app| yaml_editor_state(&app.ui_state, resource_name),
        max_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::state::ClusterState;

    fn config_maps_resource() -> ApiResource {
        ApiResource {
            group: String::new(),
            version: "v1".to_owned(),
            kind: "ConfigMap".to_owned(),
            name: "configmaps".to_owned(),
            namespaced: true,
        }
    }

    #[test]
    fn cluster_and_resource_watch_diagnostics_report_terminal_failures() {
        let cluster_key = 7;
        let namespace = Some("default".to_owned());
        let api_resource = config_maps_resource();
        let watch_key = (api_resource, namespace);
        let mut ui_state = UiState::default();
        ui_state.clusters.insert(
            cluster_key,
            ClusterState::for_test(cluster_key, "kind-kind"),
        );

        ui_state.clusters.get_mut(&cluster_key).unwrap().connection =
            ClusterConnectionState::Failed("TLS handshake failed".to_owned());
        assert_eq!(
            cluster_load_failure(&ui_state, cluster_key).as_deref(),
            Some("cluster connection failed: TLS handshake failed")
        );
        assert_eq!(
            resource_wait_failure(&ui_state, cluster_key, &watch_key).as_deref(),
            Some("cluster connection failed: TLS handshake failed")
        );

        let cluster = ui_state.clusters.get_mut(&cluster_key).unwrap();
        cluster.connection = ClusterConnectionState::Connected;
        cluster.resource_cache.insert(
            watch_key.clone(),
            ResourceWatchState {
                error: Some("watch stream closed".to_owned()),
                ..Default::default()
            },
        );
        assert_eq!(
            resource_wait_failure(&ui_state, cluster_key, &watch_key).as_deref(),
            Some("ConfigMap watcher failed in default: watch stream closed")
        );
    }

    #[test]
    fn yaml_editor_diagnostics_report_yaml_and_schema_failures() {
        let mut ui_state = UiState::default();
        let mut commands = Vec::new();
        ui_state.open_yaml_editor(
            &egui::Context::default(),
            7,
            config_maps_resource(),
            Some("default".to_owned()),
            "settings".to_owned(),
            &mut commands,
        );

        let editor = ui_state.yaml_editors.get_mut(&1).unwrap();
        editor.error = Some("YAML fetch was denied".to_owned());
        assert_eq!(
            yaml_editor_request_failure(&ui_state, "settings").as_deref(),
            Some("YAML request failed for settings: YAML fetch was denied")
        );

        let editor = ui_state.yaml_editors.get_mut(&1).unwrap();
        editor.error = None;
        editor.schema_loading = false;
        editor.server_validation = ValidationState::Failed("discovery failed".to_owned());
        assert_eq!(yaml_editor_request_failure(&ui_state, "settings"), None);
        assert_eq!(
            yaml_editor_schema_failure(&ui_state, "settings").as_deref(),
            Some("OpenAPI schema request failed for settings: discovery failed")
        );
        assert_eq!(
            yaml_editor_state(&ui_state, "missing").as_deref(),
            Some("the missing editor was never opened")
        );
    }

    #[test]
    fn kubernetes_requests_are_bounded_by_the_remaining_wait() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let timeout = Duration::from_millis(10);
        let started = Instant::now();
        let result = kubernetes_request(&runtime, timeout, async {
            std::future::pending::<Result<(), kube::Error>>().await
        });

        assert!(matches!(
            result,
            Err(KubernetesWaitError::RequestTimedOut(actual)) if actual == timeout
        ));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "request timeout should enforce the supplied deadline"
        );
    }

    #[test]
    fn kubernetes_polling_caps_each_attempt_to_keep_the_harness_responsive() {
        assert_eq!(
            kubernetes_request_timeout(Duration::from_secs(10)),
            Duration::from_secs(1)
        );
        assert_eq!(
            kubernetes_request_timeout(Duration::from_millis(250)),
            Duration::from_millis(250)
        );
    }
}
