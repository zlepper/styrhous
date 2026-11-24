//! The YAML editor boards of the Harbour design: Y01 (modified, searching), Y02 (completion and
//! problems) and Y03 (apply conflict and discard dialog). Each is built from editor state and
//! the worker results the editor really receives, then driven with keys and pointer clicks.

use super::*;
use components::test_support::primary_click;

/// The board size of the main-window screens.
const BOARD: egui::Vec2 = egui::vec2(1536.0, 1024.0);

/// The UI state holding one editor, rendered the way its window renders it.
struct BoardState {
    ui: UiState,
    commands: Vec<WorkerCommandBox>,
    ctx: Option<egui::Context>,
}

const EDITOR_ID: u64 = 1;

fn board_harness(editor: YamlEditorWindowState) -> Harness<'static, BoardState> {
    let mut ui = UiState::default();
    ui.yaml_editors.insert(editor.id, editor);
    let mut harness = Harness::builder().with_size(BOARD).build_ui_state(
        |ctx, state: &mut BoardState| {
            state.ctx = Some(ctx.clone());
            let editor = state
                .ui
                .yaml_editors
                .get_mut(&EDITOR_ID)
                .expect("the board's editor stays open");
            // The harness insets its root; a window fills its viewport.
            let mut screen = ctx.new_child(egui::UiBuilder::new().max_rect(ctx.content_rect()));
            show_editor_window(&mut screen, editor, &mut state.commands);
        },
        BoardState {
            ui,
            commands: Vec::new(),
            ctx: None,
        },
    );
    components::test_support::setup_egui(&mut harness);
    // See `snapshot_editor`: let the panels measure themselves with the theme.
    harness.run_steps(1);
    harness.run();
    harness
}

fn board_editor<'a>(harness: &'a Harness<'_, BoardState>) -> &'a YamlEditorWindowState {
    &harness.state().ui.yaml_editors[&EDITOR_ID]
}

fn put_caret(harness: &mut Harness<'_, BoardState>, character_index: usize) {
    let ctx = harness.state().ctx.clone().expect("a frame has run");
    set_editor_caret(&ctx, yaml_editor_text_edit_id(EDITOR_ID), character_index);
    harness.run();
}

fn click_label(harness: &mut Harness<'_, BoardState>, label: &str) {
    let position = harness.get_by_label(label).rect().center();
    primary_click(harness, position);
    harness.run();
}

/// Take the pointer off the window, so no hover state shows in a snapshot.
fn rest_pointer(harness: &mut Harness<'_, BoardState>) {
    harness.event(egui::Event::PointerGone);
    harness.run();
}

/// The ConfigMap of boards Y01 and Y03, as stored on the cluster.
const SETTINGS_YAML: &str = "apiVersion: v1
kind: ConfigMap
metadata:
  name: settings
  namespace: kube-system
  labels:
    app.kubernetes.io/name: settings
    app.kubernetes.io/part-of: api
  annotations:
    styrhous.dev/edited-by: ada
data:
  mode: development
  log-level: info
  feature-flags: |
    widgets.featured=true
    orders.async=false
  config-map-ref: map-config
  replicas: 3
  enable-metrics: true
";

/// The ConfigMap with `mode` changed, which is what makes the editor "Modified".
fn modified_settings() -> YamlEditorWindowState {
    let mut editor = editor(SETTINGS_YAML);
    editor.edited_yaml = SETTINGS_YAML.replace("development", "production");
    editor.resource_version = "1834".into();
    editor.validation_revision = 1;
    editor
}

/// The character index at the end of the `line`th (1-based) line.
fn end_of_line(yaml: &str, line: usize) -> usize {
    yaml.lines()
        .take(line)
        .map(|line| line.chars().count() + 1)
        .sum::<usize>()
        - 1
}

/// Y01: the edited ConfigMap, validated by the cluster, with the second of three `map` matches
/// active.
#[test]
fn y01_modified_and_searching() {
    let mut editor = modified_settings();
    editor.server_validation = ValidationState::Valid;
    let end_of_mode = end_of_line(&editor.edited_yaml, 12);
    let mut harness = board_harness(editor);

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.run();
    harness.event(egui::Event::Text("map".into()));
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    put_caret(&mut harness, end_of_mode);
    rest_pointer(&mut harness);

    assert_eq!(board_editor(&harness).search.active_match, Some(1));
    harness.get_by_label("Ln 12, Col 19");
    harness.get_by_label("Valid");
    harness.get_by_label("UTF-8");
    harness.ui_harness("boards/y01_editor_modified_searching");
}

const DEPLOYMENT_YAML: &str = "apiVersion: apps/v1
kind: Deployment
metadata:
  name: api
  namespace: payments
spec:
  replicas: three
  selector:
    matchLabels:
      app: api
  template:
    metadata:
      labels:
        app: api
    spec:
      containers:
        - name: api
          image: registry.example.com/api:v1.2.3
          imagePullPolicy: If
          ports:
            - containerPort: 8080";

/// The range of `needle`'s first occurrence in `yaml`, in characters.
fn range_of(yaml: &str, needle: &str) -> SourceRange {
    let start = yaml[..yaml.find(needle).expect("the text is present")]
        .chars()
        .count();
    SourceRange {
        start,
        end: start + needle.chars().count(),
    }
}

fn problem(yaml: &str, needle: &str, line: usize, message: &str) -> YamlDiagnostic {
    YamlDiagnostic {
        path: String::new(),
        message: message.into(),
        line: Some(line),
        range: Some(range_of(yaml, needle)),
    }
}

/// Y02: two problems, the cluster still validating, and the value completion of
/// `imagePullPolicy` open with its documentation.
#[test]
fn y02_completion_and_problems() {
    let mut editor = editor(DEPLOYMENT_YAML);
    editor.api_resource = ApiResource {
        group: "apps".into(),
        version: "v1".into(),
        kind: "Deployment".into(),
        name: "deployments".into(),
        namespaced: true,
    };
    editor.namespace = Some("payments".into());
    editor.resource_name = "api".into();
    editor.original_yaml = Some(DEPLOYMENT_YAML.replace("imagePullPolicy: If", ""));
    editor.validation_revision = 1;
    editor.server_validation = ValidationState::Pending;
    editor.diagnostics = vec![
        problem(
            DEPLOYMENT_YAML,
            "three",
            7,
            "spec.replicas: expected integer, got string “three”",
        ),
        problem(
            DEPLOYMENT_YAML,
            "If\n",
            19,
            "imagePullPolicy: “If” is not one of Always, IfNotPresent, Never",
        ),
    ];
    editor.retained_diagnostics = editor.diagnostics.clone();
    editor.suggestions = ["IfNotPresent", "Always", "Never"]
        .map(|label| CompletionSuggestion {
            label: label.into(),
            type_label: Some("enum".into()),
            detail: Some(
                "Pull the image only if it isn’t already present on the node. \
                 Default when the tag isn’t `latest`."
                    .into(),
            ),
        })
        .into();
    editor.completion_context = Some(CompletionContext {
        kind: CompletionContextKind::Value,
        type_label: Some("string · enum".into()),
        description: None,
    });
    editor.suggestions_visible = true;
    let end_of_policy = end_of_line(DEPLOYMENT_YAML, 19);
    let mut harness = board_harness(editor);

    put_caret(&mut harness, end_of_policy);
    rest_pointer(&mut harness);

    harness.get_by_label("Validating against apps/v1 schema…");
    harness.get_by_label("2 problems");
    harness.ui_harness("boards/y02_editor_completion_problems");
}

/// Y03: applying was rejected because the resource changed on the cluster; "Discard and reload"
/// asks before it throws the edits away.
#[test]
fn y03_apply_conflict_and_discard_dialog() {
    let mut editor = modified_settings();
    editor.server_validation = ValidationState::Valid;
    editor.saving = true;
    let end_of_mode = end_of_line(&editor.edited_yaml, 12);
    let api_resource = editor.api_resource.clone();
    let mut harness = board_harness(editor);
    put_caret(&mut harness, end_of_mode);

    let BoardState { ui, commands, .. } = harness.state_mut();
    ResourceApplyFailed {
        editor_id: EDITOR_ID,
        cluster_key: 7,
        api_resource,
        namespace: Some("kube-system".into()),
        resource_name: "settings".into(),
        error: ResourceApiError {
            status_code: 409,
            message: "The resource changed on the cluster".into(),
            causes: Vec::new(),
        },
        current_resource_version: Some("1902".into()),
    }
    .apply(ui, commands);
    harness.run();

    harness.get_by_label("Conflict");
    click_label(&mut harness, "Discard and reload");
    assert!(board_editor(&harness).confirm_reload);
    rest_pointer(&mut harness);
    harness.get_by_label("Discard your edits?");
    harness.ui_harness("boards/y03_editor_conflict_discard");
}
