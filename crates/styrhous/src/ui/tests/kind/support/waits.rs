use crate::api_resource::ApiResource;
use crate::resource_schema::ResourceSchema;
use crate::ui::MyEguiApp;
use crate::ui::state::{
    ClusterConnectionState, ClusterLoadState, ResourceDetailHistoryEntry, ResourceWatchKey,
    ResourceWatchState, UiState, ValidationState, YamlEditorWindowState,
};
use crate::worker::{Worker, WorkerTrait};
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
fn wait_for_with_diagnostics<W: WorkerTrait, T>(
    harness: &mut Harness<MyEguiApp<W>>,
    expectation: &str,
    mut condition: impl FnMut(&MyEguiApp<W>) -> Option<T>,
    terminal_diagnostic: impl Fn(&MyEguiApp<W>) -> Option<String>,
    timeout_diagnostic: impl Fn(&MyEguiApp<W>) -> Option<String>,
    max_ms: u64,
) -> T {
    let mut frames = WaitFrames::new(max_ms);
    loop {
        let final_frame = frames.run(harness);
        let app = harness.state();
        if let Some(message) = terminal_diagnostic(app) {
            panic!("Failed while waiting for {expectation}: {message}");
        }
        if let Some(result) = condition(app) {
            return result;
        }
        if final_frame {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let diagnostic = timeout_diagnostic(harness.state())
        .map(|message| format!("\nLast observed state: {message}"))
        .unwrap_or_default();
    panic!(
        "Timed out after {max_ms}ms waiting for {expectation} (requested at {}); {frames}{diagnostic}",
        std::panic::Location::caller(),
    );
}

/// Steps the application for a bounded wait. Worker results are applied at the start
/// of a frame, so whatever arrived while the previous frame ran is only visible after
/// the next one: the last frame therefore starts after the deadline, and a result
/// delivered in time is never missed because a frame happened to run long.
struct WaitFrames {
    deadline: Instant,
    count: u32,
    slowest: Duration,
}

impl WaitFrames {
    fn new(max_ms: u64) -> Self {
        Self {
            deadline: Instant::now() + Duration::from_millis(max_ms),
            count: 0,
            slowest: Duration::ZERO,
        }
    }

    /// Runs one frame and returns whether it was the last one the wait allows.
    fn run<W: WorkerTrait>(&mut self, harness: &mut Harness<MyEguiApp<W>>) -> bool {
        let started = Instant::now();
        let final_frame = started >= self.deadline;
        harness.run_steps(1);
        self.count += 1;
        self.slowest = self.slowest.max(started.elapsed());
        final_frame
    }
}

impl std::fmt::Display for WaitFrames {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} frames ran, the slowest took {:?}",
            self.count, self.slowest
        )
    }
}

#[track_caller]
pub(in crate::ui::tests::kind) fn wait_for_harness<T>(
    harness: &mut Harness<MyEguiApp<Worker>>,
    expectation: &str,
    mut condition: impl FnMut(&mut Harness<MyEguiApp<Worker>>) -> Option<T>,
    max_ms: u64,
) -> T {
    let mut frames = WaitFrames::new(max_ms);
    loop {
        let final_frame = frames.run(harness);
        if let Some(result) = condition(harness) {
            return result;
        }
        if final_frame {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!(
        "Timed out after {max_ms}ms waiting for {expectation} (requested at {}); {frames}",
        std::panic::Location::caller(),
    );
}

/// The details of the latest failed operation in the cluster's history, to
/// stop a wait early. The API discovery outcome is not an operation failure:
/// under the parallel Kind tests the shared API server can be slow to answer
/// discovery, and the application keeps retrying those groups in the background.
pub(in crate::ui::tests::kind) fn latest_operation_failure(
    app: &MyEguiApp<Worker>,
    cluster_key: i32,
) -> Option<String> {
    app.ui_state.clusters[&cluster_key]
        .operation_history
        .iter()
        .rev()
        .find(|entry| {
            entry.outcome == crate::ui::state::OperationOutcome::Failure
                && !entry
                    .title
                    .ends_with(crate::ui::state::FAILED_API_GROUPS_TITLE_SUFFIX)
        })
        .and_then(|entry| entry.details.clone())
}

/// Waits until exactly one node has `label`, is enabled, and kept its bounds
/// for a frame. Blades slide in and tables re-lay out as watches sync, so a
/// real pointer click is only reliable on settled geometry.
#[track_caller]
pub(in crate::ui::tests::kind) fn wait_for_settled_clickable(
    harness: &mut Harness<MyEguiApp<Worker>>,
    label: &str,
    max_ms: u64,
) {
    use egui_kittest::kittest::{NodeT, Queryable};
    let mut previous_rect = None;
    wait_for_harness(
        harness,
        &format!("{label} to be enabled and settled"),
        |harness| {
            let mut nodes = harness.query_all_by_label(label);
            let node = nodes.next()?;
            if nodes.next().is_some() || node.accesskit_node().is_disabled() {
                return None;
            }
            let rect = node.rect();
            (previous_rect.replace(rect) == Some(rect)).then_some(())
        },
        max_ms,
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
        "connection={:?}, reachable={}, namespaces={:?} ({} loaded), API discovery complete={} ({} navigation entries loaded)",
        cluster.connection,
        cluster.reachable(),
        cluster.namespaces_load,
        cluster.namespaces.len(),
        cluster.api_discovery.complete,
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
        let workspace = ui_state.clusters.get(&cluster_key).map(|cluster| {
            let same_kind = |resource: &ApiResource| resource.identity() == watch_key.0.identity();
            format!(
                "; selected resource={:?}, namespaces={:?}, active watchers={:?}, all-namespaces scope={:?}, cache keys={:?}, discovery complete={} failed groups={:?}",
                cluster.selected_api_resource,
                cluster.selected_namespaces,
                cluster
                    .active_watchers
                    .iter()
                    .filter(|(resource, _)| same_kind(resource))
                    .collect::<Vec<_>>(),
                cluster.all_namespaces_watch_scopes.get(&watch_key.0),
                cluster
                    .resource_cache
                    .keys()
                    .filter(|(resource, _)| same_kind(resource))
                    .collect::<Vec<_>>(),
                cluster.api_discovery.complete,
                cluster.api_discovery.failed_groups,
            )
        });
        return Some(format!(
            "the {} watcher has not been initialized in {}{}",
            watch_key.0.kind,
            watch_key.1.as_deref().unwrap_or("cluster scope"),
            workspace.unwrap_or_default(),
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
        "editor loading={}, error={:?}, YAML loaded={}, schema loading={}, schema loaded={}, saving={}, modified={}",
        editor.loading,
        editor.error,
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
    fn a_result_delivered_by_the_deadline_is_observed_by_a_frame_after_it() {
        use crate::ui::tests::fixtures::{application_harness, oracle_resource_table_state};
        use crate::ui::tests::harness::MockUiHarnessExt;
        use crate::worker::{KubernetesResourcesReplaced, MockWorker};

        let mut harness = application_harness::<MockWorker>();
        harness.seed_ui_state(oracle_resource_table_state());
        let cluster_key = 2;
        let watch_key = (config_maps_resource(), Some("kube-system".to_owned()));
        // Delivered in time, but no frame has applied it yet.
        harness
            .state_mut()
            .worker
            .enqueue_result(KubernetesResourcesReplaced {
                cluster_key,
                api_resource: watch_key.0.clone(),
                namespace: watch_key.1.clone(),
                resources: Vec::new(),
            });

        // The deadline has already passed, so only the final frame runs.
        wait_for_with_diagnostics(
            &mut harness,
            "the delivered ConfigMap list",
            |app| {
                resource_watch(&app.ui_state, cluster_key, &watch_key)
                    .filter(|watch| watch.is_synced)
                    .map(|_| ())
            },
            |_| None,
            |_| None,
            0,
        );
    }

    #[test]
    fn an_uninitialized_watch_reports_what_the_workspace_requested() {
        let cluster_key = 7;
        let api_resource = config_maps_resource();
        let watch_key = (api_resource.clone(), Some("kube-system".to_owned()));
        let mut ui_state = UiState::default();
        let mut cluster = ClusterState::for_test(cluster_key, "kind-kind");
        cluster.selected_api_resource = Some(api_resource.clone());
        cluster.selected_namespaces = ["kube-system"].into();
        cluster
            .active_watchers
            .insert((api_resource, Some("default".to_owned())));
        ui_state.clusters.insert(cluster_key, cluster);

        let state = resource_wait_state(&ui_state, cluster_key, &watch_key)
            .expect("an uninitialized watch is described");

        assert!(
            state.starts_with("the ConfigMap watcher has not been initialized in kube-system;"),
            "{state}"
        );
        assert!(state.contains(r#"Selected({"kube-system"})"#), "{state}");
        assert!(
            state.contains(r#"active watchers=[(ApiResource { group: "", version: "v1", kind: "ConfigMap", name: "configmaps", namespaced: true }, Some("default"))]"#),
            "{state}"
        );
        assert!(state.contains("cache keys=[]"), "{state}");
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
