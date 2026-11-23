use super::*;

pub(super) fn refresh_local_validation(editor: &mut YamlEditorWindowState) {
    editor.validation_revision += 1;
    editor.validation_due = None;
    editor.server_validation = ValidationState::Idle;
    editor.diagnostics.clear();
    match &editor.schema {
        Some(schema) => match schema.validate_yaml(&editor.edited_yaml) {
            Ok(diagnostics) => {
                editor.diagnostics = diagnostics;
                if editor.diagnostics.is_empty() {
                    editor.validation_due = Some(Instant::now() + VALIDATION_DEBOUNCE);
                }
            }
            Err(message) if message.starts_with("Unable to compile the Kubernetes schema:") => {
                // A malformed or unsupported OpenAPI extension must not prevent the API server
                // from validating the document. Treat the local schema as unavailable.
                editor.server_validation = ValidationState::Failed(message);
                editor.validation_due = Some(Instant::now() + VALIDATION_DEBOUNCE);
            }
            Err(message) => editor.diagnostics.push(YamlDiagnostic {
                range: yaml_error_range(&editor.edited_yaml, &message),
                line: yaml_error_line(&message),
                path: String::new(),
                message,
            }),
        },
        None => match serde_yaml::from_str::<serde_yaml::Value>(&editor.edited_yaml) {
            Ok(_) => editor.validation_due = Some(Instant::now() + VALIDATION_DEBOUNCE),
            Err(error) => editor.diagnostics.push(YamlDiagnostic {
                range: error.location().and_then(|location| {
                    SourceRange::at_yaml_location(
                        &editor.edited_yaml,
                        location.line(),
                        location.column(),
                    )
                }),
                line: error.location().map(|location| location.line()),
                path: String::new(),
                message: error.to_string(),
            }),
        },
    }
    if !editor.diagnostics.is_empty() {
        editor.retained_diagnostics = editor.diagnostics.clone();
    }
}

pub(super) fn yaml_error_line(message: &str) -> Option<usize> {
    message
        .rsplit_once(" at ")
        .and_then(|(_, location)| location.split_once(':'))
        .and_then(|(line, _)| line.parse().ok())
}

pub(super) fn yaml_error_range(yaml: &str, message: &str) -> Option<SourceRange> {
    let (line, column) = message.rsplit_once(" at ")?.1.split_once(':')?;
    SourceRange::at_yaml_location(yaml, line.parse().ok()?, column.parse().ok()?)
}

pub(super) fn maybe_request_server_validation(
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    if editor
        .validation_due
        .is_some_and(|due| due <= Instant::now())
    {
        editor.validation_due = None;
        editor.server_validation = ValidationState::Pending;
        commands_to_send.push(Box::new(ValidateResourceYaml {
            editor_id: editor.id,
            revision: editor.validation_revision,
            cluster_key: editor.cluster_key,
            api_resource: editor.api_resource.clone(),
            namespace: editor.namespace.clone(),
            resource_name: editor.resource_name.clone(),
            original_yaml: editor.original_yaml.clone().unwrap_or_default(),
            yaml: editor.edited_yaml.clone(),
            resource_version: editor.resource_version.clone(),
            resource_uid: editor.resource_uid.clone(),
        }));
    }
}

/// What the toolbar says about validation: the words and whether work is
/// still in progress.
pub(super) fn validation_status(editor: &YamlEditorWindowState) -> Option<(&'static str, bool)> {
    let (diagnostics, showing_retained_diagnostics) = diagnostics_to_display(editor);
    if showing_retained_diagnostics {
        return Some(("Validating updated YAML…", true));
    }
    if !diagnostics.is_empty() {
        return None;
    }
    match &editor.server_validation {
        ValidationState::Pending => Some(("Validating with cluster…", true)),
        ValidationState::Valid => Some(("Validated by cluster", false)),
        ValidationState::Failed(_) => None,
        ValidationState::Idle => editor
            .schema_loading
            .then_some(("Loading Kubernetes schema…", true)),
    }
}

/// The 1-based line and column of the editor's caret.
pub(super) fn cursor_position(
    ctx: &egui::Context,
    editor: &YamlEditorWindowState,
) -> Option<(usize, usize)> {
    let state =
        egui::widgets::text_edit::TextEditState::load(ctx, yaml_editor_text_edit_id(editor.id))?;
    let index = state.cursor.char_range()?.primary.index.0;
    let before = &editor.edited_yaml[..byte_index(&editor.edited_yaml, index)];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |line| line.chars().count())
        + 1;
    Some((line, column))
}

fn problems_open_id(editor: &YamlEditorWindowState) -> egui::Id {
    egui::Id::new(("yaml-editor-problems-open", editor.id))
}

/// Whether the Problems panel is open (it starts open).
fn problems_open(ctx: &egui::Context, editor: &YamlEditorWindowState) -> bool {
    ctx.data(|data| data.get_temp::<bool>(problems_open_id(editor)))
        .unwrap_or(true)
}

/// Open or close the Problems panel (the status bar's problem count).
pub(super) fn toggle_problems(ctx: &egui::Context, editor: &YamlEditorWindowState) {
    let open = problems_open(ctx, editor);
    ctx.data_mut(|data| data.insert_temp(problems_open_id(editor), !open));
}

/// The Problems panel: the diagnostics to display; clicking one selects its
/// range. Diagnostics kept while the updated YAML validates are dimmed and
/// cannot be followed.
pub(super) fn show_diagnostics(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
) {
    let (diagnostics, showing_retained_diagnostics) = diagnostics_to_display(editor);
    let problems: Vec<Problem<'_>> = diagnostics
        .iter()
        .map(|diagnostic| Problem {
            severity: Severity::Error,
            line: diagnostic.line,
            message: &diagnostic.message,
        })
        .collect();
    let open_id = problems_open_id(editor);
    let mut open = ctx
        .data(|data| data.get_temp::<bool>(open_id))
        .unwrap_or(true);
    let response = ProblemsPanel::new(("yaml-editor", editor.id), &problems)
        .stale(showing_retained_diagnostics)
        .row_tooltip(if showing_retained_diagnostics {
            "Validating the updated YAML before this diagnostic can be located"
        } else {
            "Jump to the highlighted YAML location"
        })
        .show(ui, &mut open);
    ctx.data_mut(|data| data.insert_temp(open_id, open));
    let range_to_focus = response
        .clicked
        .and_then(|index| diagnostics[index].range.clone());
    if let Some(range) = range_to_focus {
        focus_diagnostic(ctx, editor, range);
    }
}

pub(super) fn diagnostics_to_display(editor: &YamlEditorWindowState) -> (&[YamlDiagnostic], bool) {
    let validation_in_progress = editor.validation_due.is_some()
        || matches!(editor.server_validation, ValidationState::Pending);
    if editor.diagnostics.is_empty()
        && validation_in_progress
        && !editor.retained_diagnostics.is_empty()
    {
        (&editor.retained_diagnostics, true)
    } else {
        (&editor.diagnostics, false)
    }
}

/// Where a pending request to focus the editor is kept: the pass it was
/// made in and the range to select.
fn pending_focus_id(editor_id: u64) -> egui::Id {
    egui::Id::new(("yaml-editor-pending-focus", editor_id))
}

/// Focus the editor if a diagnostic asked for it in an earlier pass.
pub(super) fn apply_pending_focus(ctx: &egui::Context, editor_id: u64) {
    let id = pending_focus_id(editor_id);
    let requested = ctx.data(|data| data.get_temp::<(u64, usize, usize)>(id));
    if let Some((pass, start, end)) = requested
        && pass < ctx.cumulative_pass_nr()
    {
        ctx.data_mut(|data| data.remove::<(u64, usize, usize)>(id));
        select_range(ctx, editor_id, start, end);
    }
}

/// Focus the editor with `start..end` selected.
fn select_range(ctx: &egui::Context, editor_id: u64, start: usize, end: usize) {
    let text_edit_id = yaml_editor_text_edit_id(editor_id);
    ctx.memory_mut(|memory| memory.request_focus(text_edit_id));
    let mut state =
        egui::widgets::text_edit::TextEditState::load(ctx, text_edit_id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(start),
        CCursor::new(end),
    )));
    state.store(ctx, text_edit_id);
}

pub(super) fn focus_diagnostic(
    ctx: &egui::Context,
    editor: &mut YamlEditorWindowState,
    range: SourceRange,
) {
    select_range(ctx, editor.id, range.start, range.end);
    // The click that chose the diagnostic landed outside the editor, which
    // makes the editor give up focus (and its selection) later in this
    // pass; select the range again next pass.
    let pending = (ctx.cumulative_pass_nr(), range.start, range.end);
    ctx.data_mut(|data| data.insert_temp(pending_focus_id(editor.id), pending));
    editor.scroll_to_diagnostic = Some(range);
    ctx.request_repaint();
}

pub(super) fn apply_editor(
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    editor.saving = true;
    editor.error = None;
    commands_to_send.push(Box::new(ApplyResourceYaml {
        editor_id: editor.id,
        cluster_key: editor.cluster_key,
        api_resource: editor.api_resource.clone(),
        namespace: editor.namespace.clone(),
        resource_name: editor.resource_name.clone(),
        original_yaml: editor.original_yaml.clone().unwrap_or_default(),
        yaml: editor.edited_yaml.clone(),
        resource_version: editor.resource_version.clone(),
        resource_uid: editor.resource_uid.clone(),
    }));
}

pub(super) fn request_close(ctx: &egui::Context, editor: &mut YamlEditorWindowState) {
    if editor.is_modified() {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        editor.confirm_discard = true;
    } else {
        editor.close_requested = true;
    }
}

pub(super) fn show_discard_confirmation(ctx: &egui::Context, editor: &mut YamlEditorWindowState) {
    if !editor.confirm_discard {
        return;
    }
    match (ConfirmationDialog {
        id: egui::Id::new(("discard-yaml-changes", editor.id)),
        icon: components::icons::Icon::ExclamationTriangle,
        title: "Discard changes?",
        message: "Your unsaved edits will be lost.".into(),
        unavailable_message: None,
        cancel_label: "Keep editing",
        confirm_label: "Discard changes",
        kind: ConfirmationDialogKind::Destructive,
        confirm_enabled: true,
        warning: None,
        acknowledgement: None,
    })
    .show(ctx)
    {
        ConfirmationDialogAction::Confirm => {
            editor.close_requested = true;
            editor.confirm_discard = false;
        }
        ConfirmationDialogAction::Cancel => editor.confirm_discard = false,
        ConfirmationDialogAction::None => {}
    }
}
