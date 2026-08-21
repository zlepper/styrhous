use super::state::{
    UiState, ValidationState, YamlEditorHighlightCache, YamlEditorHighlightCacheKey,
    YamlEditorWindowState, api_error_message, diagnostics_from_api_error, set_editor_diagnostics,
};
use crate::resource_schema::{
    CompletionContext, CompletionContextKind, SourceRange, YamlDiagnostic,
};
use crate::worker::{
    ApplyResourceYaml, ResourceApplyCompleted, ResourceApplyFailed, ResourceSchemaLoadFailed,
    ResourceSchemaLoaded, ResourceYamlApplyCommandFailed, ResourceYamlFetchFailed,
    ResourceYamlFetched, ResourceYamlValidated, ResourceYamlValidationCommandFailed,
    ResourceYamlValidationFailed, ValidateResourceYaml, WorkerCommandBox, WorkerResult,
};
use components::colors::{TABLE_BORDER, TOOLBAR_BACKGROUND, gray, indigo};
use components::design::{search, spacing, status, surface, typography};
use components::{
    ConfirmationDialog, ConfirmationDialogAction, ConfirmationDialogKind, PointingHand,
    SEARCH_NAVIGATION_BUTTON_SIZE, TailwindButton, TailwindSearchInput, icons,
    search_navigation_button,
};
use egui::text::{CCursor, CCursorRange};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};
use std::collections::HashSet;
use std::ops::Range;
use std::time::{Duration, Instant};

const TOOLBAR_HEIGHT: f32 = 52.0;
const VALIDATION_DEBOUNCE: Duration = Duration::from_millis(500);
const COMPLETION_LIST_MIN_WIDTH: f32 = 240.0;
const COMPLETION_LIST_MAX_WIDTH: f32 = 280.0;
const COMPLETION_DOCUMENTATION_WIDTH: f32 = 280.0;
const COMPLETION_ROW_HEIGHT: f32 = 26.0;
const COMPLETION_LIST_MAX_HEIGHT: f32 = 260.0;
const COMPLETION_POPUP_CHROME_HEIGHT: f32 = 68.0;
const DIAGNOSTIC_ROW_HEIGHT: f32 = 21.0;
const DIAGNOSTIC_LIST_MAX_HEIGHT: f32 = 6.0 * DIAGNOSTIC_ROW_HEIGHT;
const SEARCH_CONTROL_WIDTH: f32 = 212.0;

impl WorkerResult for ResourceYamlFetchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(editor) = ui.yaml_editors.get_mut(&self.editor_id) {
            editor.loading = false;
            editor.error = Some(self.error);
        }
    }
}

impl WorkerResult for ResourceSchemaLoadFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(editor) = ui.yaml_editors.get_mut(&self.editor_id) {
            editor.schema_loading = false;
            editor.server_validation = ValidationState::Failed(self.error);
        }
    }
}

impl WorkerResult for ResourceYamlApplyCommandFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(editor) = ui.yaml_editors.get_mut(&self.editor_id) {
            editor.saving = false;
            editor.error = Some(self.error);
        }
    }
}

impl WorkerResult for ResourceYamlValidationCommandFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(editor) = ui.yaml_editors.get_mut(&self.editor_id)
            && editor.validation_revision == self.revision
        {
            editor.server_validation = ValidationState::Failed(self.error);
        }
    }
}

impl WorkerResult for ResourceYamlFetched {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceYamlFetched {
            editor_id,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            yaml,
        } = self;
        if let Some(editor) = ui.yaml_editors.get_mut(&editor_id)
            && editor.resource_matches(cluster_key, &api_resource, &namespace, &resource_name)
        {
            editor.original_yaml = Some(yaml.clone());
            editor.edited_yaml = yaml;
            editor.loading = false;
            editor.error = None;
        }
    }
}

impl WorkerResult for ResourceSchemaLoaded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceSchemaLoaded {
            editor_id,
            cluster_key,
            api_resource,
            schema,
        } = self;
        ui.resource_schemas
            .insert((cluster_key, api_resource.clone()), schema.clone());
        if let Some(editor) = ui.yaml_editors.get_mut(&editor_id)
            && editor.cluster_key == cluster_key
            && editor.api_resource == api_resource
        {
            editor.schema = Some(schema);
            editor.schema_loading = false;
            editor.validation_revision = 0;
        }
    }
}

impl WorkerResult for ResourceYamlValidated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceYamlValidated {
            editor_id,
            revision,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
        } = self;
        if let Some(editor) = ui.yaml_editors.get_mut(&editor_id)
            && editor.validation_revision == revision
            && editor.resource_matches(cluster_key, &api_resource, &namespace, &resource_name)
        {
            editor.server_validation = ValidationState::Valid;
        }
    }
}

impl WorkerResult for ResourceYamlValidationFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceYamlValidationFailed {
            editor_id,
            revision,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            error,
        } = self;
        if let Some(editor) = ui.yaml_editors.get_mut(&editor_id)
            && editor.validation_revision == revision
            && editor.resource_matches(cluster_key, &api_resource, &namespace, &resource_name)
        {
            let message = api_error_message(&error);
            let diagnostics = diagnostics_from_api_error(&error, &editor.edited_yaml);
            editor.server_validation = ValidationState::Failed(message);
            set_editor_diagnostics(editor, diagnostics);
        }
    }
}

impl WorkerResult for ResourceApplyCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceApplyCompleted {
            editor_id,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
        } = self;
        if let Some(editor) = ui.yaml_editors.get_mut(&editor_id)
            && editor.resource_matches(cluster_key, &api_resource, &namespace, &resource_name)
        {
            editor.original_yaml = Some(editor.edited_yaml.clone());
            editor.saving = false;
            editor.error = None;
        }
    }
}

impl WorkerResult for ResourceApplyFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceApplyFailed {
            editor_id,
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            error,
        } = self;
        if let Some(editor) = ui.yaml_editors.get_mut(&editor_id)
            && editor.resource_matches(cluster_key, &api_resource, &namespace, &resource_name)
        {
            editor.saving = false;
            editor.error = Some(api_error_message(&error));
            let diagnostics = diagnostics_from_api_error(&error, &editor.edited_yaml);
            set_editor_diagnostics(editor, diagnostics);
        }
    }
}

pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let ids = ui_state.yaml_editors.keys().copied().collect::<Vec<_>>();
    for id in ids {
        let Some(editor) = ui_state.yaml_editors.get_mut(&id) else {
            continue;
        };
        let viewport_id = egui::ViewportId::from_hash_of(("yaml-editor-window", id));
        if editor.focus_requested {
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
            editor.focus_requested = false;
        }
        let title = format!("Edit · {}", editor.resource_name);
        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(crate::DEFAULT_NATIVE_WINDOW_SIZE)
                .with_min_inner_size(crate::MIN_NATIVE_WINDOW_SIZE),
            |window_ui, _| show_editor_window(window_ui, editor, commands_to_send),
        );
    }
    ui_state
        .yaml_editors
        .retain(|_, editor| !editor.close_requested);
}

pub(super) fn show_editor_window(
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    #[cfg(any(test, feature = "benchmarks"))]
    show_editor_window_inner(ui, editor, commands_to_send, None);
    #[cfg(not(any(test, feature = "benchmarks")))]
    show_editor_window_inner(ui, editor, commands_to_send);
}

#[cfg(any(test, feature = "benchmarks"))]
pub(super) fn show_editor_window_with_scroll_metrics(
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
    scroll_metrics: Option<&mut YamlEditorScrollMetrics>,
) {
    show_editor_window_inner(ui, editor, commands_to_send, scroll_metrics);
}

#[cfg(any(test, feature = "benchmarks"))]
#[derive(Debug, Clone, Copy)]
pub(super) struct YamlEditorScrollMetrics {
    pub(super) offset: egui::Vec2,
    pub(super) inner_rect: egui::Rect,
}

#[cfg(any(test, feature = "benchmarks"))]
impl Default for YamlEditorScrollMetrics {
    fn default() -> Self {
        Self {
            offset: egui::Vec2::ZERO,
            inner_rect: egui::Rect::NOTHING,
        }
    }
}

mod completion;
mod diagnostics;
mod editor;
mod layout;
#[path = "yaml_editor/search.rs"]
mod search_controls;
mod window;

use completion::*;
use diagnostics::*;
use editor::*;
use layout::*;
use search_controls::*;
use window::*;

#[cfg(test)]
mod tests;
