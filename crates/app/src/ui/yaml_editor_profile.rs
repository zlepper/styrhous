//! CPU-only benchmark support for the YAML editor.
//!
//! This profile runs the production Deployment editor in an egui context with
//! a pinned Kubernetes OpenAPI v3 response. GPU submission and Kubernetes I/O
//! are deliberately outside the benchmark.

use super::configure_egui_context;
use super::state::{ValidationState, YamlEditorSearchState, YamlEditorWindowState};
use super::yaml_editor::{YamlEditorScrollMetrics, show_editor_window_with_scroll_metrics};
use crate::api_resource::ApiResource;
use crate::resource_schema::ResourceSchema;
use crate::worker::WorkerCommandBox;
use k8s_openapi::serde_json::{self, Value};

const OPENAPI_APPS_V1: &str =
    include_str!("../../benches/fixtures/kubernetes-v1.31.0-apps-v1-openapi.json");
const VIEWPORT_SIZE: egui::Vec2 = egui::vec2(1280.0, 900.0);
const EDITOR_ID: u64 = 9_001;
const INITIAL_SCROLL_DELTA_Y: f32 = -180.0;
const DEFAULT_DOCUMENT_BYTES: &[usize] = &[10 * 1024, 100 * 1024, 1024 * 1024];

/// Reusable state for Criterion's large-Deployment editor scenarios.
pub struct YamlEditorProfile {
    context: egui::Context,
    input: egui::RawInput,
    editor: YamlEditorWindowState,
    commands: Vec<WorkerCommandBox>,
    elapsed_seconds: f64,
    scroll_metrics: YamlEditorScrollMetrics,
    next_scroll_delta_y: f32,
}

impl YamlEditorProfile {
    /// Creates a valid Deployment editor whose YAML is at least `document_bytes` long.
    pub fn with_document_bytes(document_bytes: usize) -> Result<Self, String> {
        let (schema, _) = prepared_deployment_schema()?;
        let yaml = deployment_yaml(document_bytes);
        let context = egui::Context::default();
        configure_egui_context(&context);

        Ok(Self {
            context,
            input: raw_input(0.0),
            editor: YamlEditorWindowState {
                id: EDITOR_ID,
                cluster_key: 1,
                api_resource: deployment_resource(),
                namespace: Some("benchmark".to_owned()),
                resource_name: "large-deployment".to_owned(),
                original_yaml: Some(yaml.clone()),
                edited_yaml: yaml,
                loading: false,
                saving: false,
                error: None,
                close_requested: false,
                confirm_discard: false,
                focus_requested: false,
                schema: Some(schema),
                schema_loading: false,
                diagnostics: Vec::new(),
                retained_diagnostics: Vec::new(),
                scroll_to_diagnostic: None,
                server_validation: ValidationState::Idle,
                validation_revision: 0,
                validation_due: None,
                suggestions: Vec::new(),
                completion_context: None,
                completion_cursor: None,
                suggestions_visible: false,
                suggestion_selection: 0,
                search: YamlEditorSearchState::default(),
                highlight_cache: Default::default(),
            },
            commands: Vec::new(),
            elapsed_seconds: 0.0,
            scroll_metrics: YamlEditorScrollMetrics::default(),
            next_scroll_delta_y: INITIAL_SCROLL_DELTA_Y,
        })
    }

    /// Parses the pinned OpenAPI document and selects its Deployment schema.
    ///
    /// The returned component count gives Criterion an observable result while
    /// keeping schema setup separate from interactive frame timings.
    pub fn prepare_deployment_schema() -> Result<usize, String> {
        let (schema, component_count) = prepared_deployment_schema()?;
        std::hint::black_box(schema);
        Ok(component_count)
    }

    /// The default document sizes covered by the Criterion target.
    pub const DEFAULT_DOCUMENT_BYTES: &'static [usize] = DEFAULT_DOCUMENT_BYTES;

    /// Renders one frame. Calling this once measures first-render work;
    /// callers should warm the profile before measuring steady interaction.
    pub fn run_frame(&mut self) -> usize {
        self.elapsed_seconds += 1.0 / 60.0;
        self.input.time = Some(self.elapsed_seconds);
        let mut output = self.context.run_ui(self.input.clone(), |ui| {
            show_editor_window_with_scroll_metrics(
                ui,
                &mut self.editor,
                &mut self.commands,
                Some(&mut self.scroll_metrics),
            );
        });
        output.textures_delta.clear();
        self.input.events.clear();
        self.editor.edited_yaml.len()
    }

    /// Drives the existing YAML scroll area with native-style wheel input.
    ///
    /// Directions alternate so repeated Criterion iterations cannot settle at a
    /// clamped scroll boundary and silently become idle-frame measurements.
    pub fn scroll_frame(&mut self) -> usize {
        let pointer_position = self.scroll_metrics.inner_rect.center();
        let initial_offset = self.scroll_metrics.offset.y;
        self.input.events = vec![
            egui::Event::PointerMoved(pointer_position),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, self.next_scroll_delta_y),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let result = self.run_frame();
        debug_assert!((self.scroll_metrics.offset.y - initial_offset).abs() > f32::EPSILON);
        self.next_scroll_delta_y = -self.next_scroll_delta_y;
        result
    }
}

fn prepared_deployment_schema() -> Result<(ResourceSchema, usize), String> {
    let document: Value = serde_json::from_str(OPENAPI_APPS_V1)
        .map_err(|error| format!("benchmark OpenAPI fixture is invalid: {error}"))?;
    let component_count = document
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .map_or(0, |schemas| schemas.len());
    let schema = ResourceSchema::from_openapi_document(document, &deployment_resource())
        .ok_or_else(|| "benchmark OpenAPI fixture has no apps/v1 Deployment schema".to_owned())?;
    Ok((schema, component_count))
}

fn deployment_resource() -> ApiResource {
    ApiResource {
        group: "apps".to_owned(),
        version: "v1".to_owned(),
        kind: "Deployment".to_owned(),
        name: "deployments".to_owned(),
        namespaced: true,
    }
}

fn deployment_yaml(document_bytes: usize) -> String {
    const HEADER: &str = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: large-deployment\n  namespace: benchmark\nspec:\n  replicas: 1\n  selector:\n    matchLabels:\n      app: benchmark\n  template:\n    metadata:\n      labels:\n        app: benchmark\n    spec:\n      containers:\n        - name: workload\n          image: registry.k8s.io/pause:3.10\n          env:\n";
    const ENTRY_PREFIX: &str = "            - name: BENCHMARK_VARIABLE_";
    const ENTRY_MIDDLE: &str = "\n              value: \"";

    let mut yaml = String::with_capacity(document_bytes.max(HEADER.len()));
    yaml.push_str(HEADER);
    let mut index = 0;
    let filler = "x".repeat(48);
    loop {
        let payload = format!("benchmark-value-{index:06}-{filler}");
        yaml.push_str(ENTRY_PREFIX);
        yaml.push_str(&format!("{index:06}"));
        yaml.push_str(ENTRY_MIDDLE);
        yaml.push_str(&payload);
        yaml.push_str("\"\n");
        index += 1;
        if yaml.len() >= document_bytes {
            break;
        }
    }
    yaml
}

fn raw_input(time: f64) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT_SIZE)),
        time: Some(time),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_openapi_fixture_contains_a_deployment_schema() {
        assert!(YamlEditorProfile::prepare_deployment_schema().expect("fixture is valid") > 100);
    }

    #[test]
    fn generated_deployments_parse_at_every_default_size() {
        for &document_bytes in DEFAULT_DOCUMENT_BYTES {
            let profile = YamlEditorProfile::with_document_bytes(document_bytes)
                .expect("benchmark profile initializes");

            assert!(profile.editor.edited_yaml.len() >= document_bytes);
            serde_yaml::from_str::<serde_yaml::Value>(&profile.editor.edited_yaml)
                .expect("generated Deployment YAML is valid");
        }
    }

    #[test]
    fn generated_deployment_passes_the_loaded_schema() {
        let mut profile = YamlEditorProfile::with_document_bytes(DEFAULT_DOCUMENT_BYTES[0])
            .expect("benchmark profile initializes");

        profile.run_frame();

        assert!(profile.editor.diagnostics.is_empty());
        assert!(!matches!(
            profile.editor.server_validation,
            ValidationState::Failed(_)
        ));
    }

    #[test]
    fn wheel_input_advances_the_editor_scroll_offset() {
        let mut profile = YamlEditorProfile::with_document_bytes(10 * 1024)
            .expect("benchmark profile initializes");
        profile.run_frame();
        let initial_offset = profile.scroll_metrics.offset.y;
        let initial_job = profile
            .editor
            .highlight_cache
            .stored_job()
            .expect("initial frame creates a highlighted layout job")
            .clone();
        assert!(
            profile
                .scroll_metrics
                .inner_rect
                .contains(profile.scroll_metrics.inner_rect.center())
        );

        assert_eq!(profile.scroll_frame(), profile.editor.edited_yaml.len());
        assert!(profile.scroll_metrics.offset.y > initial_offset);
        assert!(std::sync::Arc::ptr_eq(
            &initial_job,
            profile
                .editor
                .highlight_cache
                .stored_job()
                .expect("scrolling reuses the highlighted layout job")
        ));
        let downward_offset = profile.scroll_metrics.offset.y;

        profile.scroll_frame();
        assert!(profile.scroll_metrics.offset.y < downward_offset);
        assert!(std::sync::Arc::ptr_eq(
            &initial_job,
            profile
                .editor
                .highlight_cache
                .stored_job()
                .expect("scrolling back reuses the highlighted layout job")
        ));
    }

    #[test]
    fn changed_yaml_rebuilds_its_highlighted_layout_job() {
        let mut profile = YamlEditorProfile::with_document_bytes(10 * 1024)
            .expect("benchmark profile initializes");

        profile.run_frame();
        let first_job = profile
            .editor
            .highlight_cache
            .stored_job()
            .expect("first frame creates a highlighted layout job")
            .clone();
        profile.editor.edited_yaml.push_str("# changed\n");
        profile.run_frame();

        assert!(!std::sync::Arc::ptr_eq(
            &first_job,
            profile
                .editor
                .highlight_cache
                .stored_job()
                .expect("changed frame creates a highlighted layout job")
        ));
    }

    #[test]
    fn search_change_rebuilds_the_highlighted_layout_job() {
        let mut profile = YamlEditorProfile::with_document_bytes(10 * 1024)
            .expect("benchmark profile initializes");

        profile.run_frame();
        let first_job = profile
            .editor
            .highlight_cache
            .stored_job()
            .expect("first frame creates a highlighted layout job")
            .clone();
        profile.editor.search.query = "BENCHMARK_VARIABLE".to_owned();
        profile.run_frame();

        assert!(!std::sync::Arc::ptr_eq(
            &first_job,
            profile
                .editor
                .highlight_cache
                .stored_job()
                .expect("search frame creates a highlighted layout job")
        ));
    }

    #[test]
    fn regex_mode_change_rebuilds_the_highlighted_layout_job() {
        let mut profile = YamlEditorProfile::with_document_bytes(10 * 1024)
            .expect("benchmark profile initializes");
        profile.editor.search.query = r"BENCHMARK_VARIABLE_\d+".to_owned();

        profile.run_frame();
        let literal_job = profile
            .editor
            .highlight_cache
            .stored_job()
            .expect("literal search creates a highlighted layout job")
            .clone();

        profile.editor.search.regex_mode = true;
        profile.run_frame();

        assert!(
            !std::sync::Arc::ptr_eq(
                &literal_job,
                profile
                    .editor
                    .highlight_cache
                    .stored_job()
                    .expect("regex search creates a highlighted layout job")
            ),
            "switching a query with different literal and regex meanings must rebuild highlights"
        );
    }

    #[test]
    fn active_search_match_rebuilds_the_highlighted_layout_job() {
        let mut profile = YamlEditorProfile::with_document_bytes(10 * 1024)
            .expect("benchmark profile initializes");
        profile.editor.search.query = "BENCHMARK_VARIABLE".to_owned();

        profile.run_frame();
        let inactive_job = profile
            .editor
            .highlight_cache
            .stored_job()
            .expect("search frame creates a highlighted layout job")
            .clone();
        profile.editor.search.active_match = Some(0);
        profile.run_frame();
        let first_active_job = profile
            .editor
            .highlight_cache
            .stored_job()
            .expect("active search frame creates a highlighted layout job")
            .clone();

        assert!(!std::sync::Arc::ptr_eq(&inactive_job, &first_active_job));

        profile.editor.search.active_match = Some(1);
        profile.run_frame();

        assert!(!std::sync::Arc::ptr_eq(
            &first_active_job,
            profile
                .editor
                .highlight_cache
                .stored_job()
                .expect("next active search frame creates a highlighted layout job")
        ));
    }
}
