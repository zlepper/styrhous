use super::*;
use crate::api_resource::ApiResource;
use crate::resource_schema::{
    CompletionContext, CompletionContextKind, CompletionSuggestion, ResourceSchema,
};
use crate::worker::ResourceApiError;
use components::test_support::{HarnessSnapshotOptions, UiHarnessSnapshot};
use egui_kittest::{Harness, kittest::Queryable};
use k8s_openapi::serde_json::json;

const COMPLETION_POPUP_MAX_WIDTH: f32 = COMPLETION_LIST_MAX_WIDTH + 2.0 * spacing::MD;
const COMPLETION_POPUP_MAX_HEIGHT: f32 =
    COMPLETION_POPUP_CHROME_HEIGHT + COMPLETION_LIST_MAX_HEIGHT;

mod basic;
mod completion_basics;
mod completion_navigation;
mod diagnostics;
mod windows;

fn editor(yaml: &str) -> YamlEditorWindowState {
    YamlEditorWindowState {
        id: 1,
        cluster_key: 7,
        api_resource: ApiResource {
            group: "core".into(),
            version: "v1".into(),
            kind: "ConfigMap".into(),
            name: "configmaps".into(),
            namespaced: true,
        },
        namespace: Some("kube-system".into()),
        resource_name: "settings".into(),
        original_yaml: Some(yaml.into()),
        edited_yaml: yaml.into(),
        loading: false,
        saving: false,
        error: None,
        close_requested: false,
        confirm_discard: false,
        focus_requested: false,
        schema: None,
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
        search: Default::default(),
        highlight_cache: Default::default(),
    }
}

fn diagnostic_editor() -> YamlEditorWindowState {
    let yaml = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings";
    let start = yaml
        .find("settings")
        .map(|index| yaml[..index].chars().count())
        .expect("diagnostic text is present");
    let mut editor = editor(yaml);
    editor.validation_revision = 1;
    editor.diagnostics = vec![YamlDiagnostic {
        path: "/metadata/name".into(),
        message: "\"settings\" is not an allowed value".into(),
        line: Some(4),
        range: Some(SourceRange {
            start,
            end: start + "settings".chars().count(),
        }),
    }];
    editor.retained_diagnostics = editor.diagnostics.clone();
    editor
}

fn snapshot_editor(editor: YamlEditorWindowState, name: &str) {
    let confirm_discard = editor.confirm_discard;
    let mut editor = editor;
    editor.confirm_discard = false;
    // Snapshot rendering can take longer than the server-validation
    // debounce when the suite is running in parallel. Keep ordinary
    // fixtures in their explicitly clean state; validation-specific
    // fixtures set a non-idle state before reaching this helper.
    if matches!(editor.server_validation, ValidationState::Idle)
        && editor.diagnostics.is_empty()
        && !editor.schema_loading
        && editor.validation_due.is_none()
    {
        editor.validation_revision = editor.validation_revision.max(1);
        editor.validation_due = None;
    }
    let mut harness = Harness::builder().build_ui_state(
        |ctx, state: &mut SnapshotState| {
            show_editor_window(ctx, &mut state.editor, &mut state.commands);
        },
        SnapshotState {
            editor,
            commands: Vec::new(),
            ctx: None,
        },
    );
    components::test_support::setup_egui(&mut harness);
    harness.state_mut().editor.confirm_discard = confirm_discard;
    harness.run();
    harness.ui_harness(name);
}

struct SnapshotState {
    editor: YamlEditorWindowState,
    commands: Vec<WorkerCommandBox>,
    ctx: Option<egui::Context>,
}

fn editor_harness(editor: YamlEditorWindowState) -> Harness<'static, SnapshotState> {
    let mut harness = Harness::builder().build_ui_state(
        |ctx, state: &mut SnapshotState| {
            state.ctx = Some(ctx.clone());
            show_editor_window(ctx, &mut state.editor, &mut state.commands);
        },
        SnapshotState {
            editor,
            commands: Vec::new(),
            ctx: None,
        },
    );
    components::test_support::setup_egui(&mut harness);
    harness.run();
    harness
}

fn many_suggestions(count: usize) -> Vec<CompletionSuggestion> {
    (0..count)
        .map(|index| CompletionSuggestion {
            label: format!("field-{index:03}"),
            type_label: Some("string".into()),
            detail: Some(format!("Documentation for field {index:03}.")),
        })
        .collect()
}

fn snapshot_completion_at_focused_caret(yaml: &str, name: &str) {
    let mut editor = editor(yaml);
    editor.suggestions = many_suggestions(3);
    editor.suggestions_visible = true;
    editor.validation_revision = 1;
    let mut harness = editor_harness(editor);
    let ctx = harness
        .state()
        .ctx
        .clone()
        .expect("editor context is available");
    let text_edit_id = yaml_editor_text_edit_id(harness.state().editor.id);
    set_editor_caret(&ctx, text_edit_id, yaml.chars().count());

    harness.run();
    harness.ui_harness(HarnessSnapshotOptions::one_pixel(name));
}

fn set_editor_caret(ctx: &egui::Context, id: egui::Id, character_index: usize) {
    ctx.memory_mut(|memory| memory.request_focus(id));
    let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id)
        .expect("text editor state is available");
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(character_index))));
    state.store(ctx, id);
}

fn editor_caret(ctx: &egui::Context, id: egui::Id) -> usize {
    egui::widgets::text_edit::TextEditState::load(ctx, id)
        .and_then(|state| state.cursor.char_range())
        .expect("editor caret is available")
        .primary
        .index
        .into()
}
