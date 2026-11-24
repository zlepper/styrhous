use super::*;

pub(super) fn refresh_local_validation(editor: &mut YamlEditorWindowState) {
    editor.validation_revision += 1;
    editor.revalidate = false;
    editor.validation_due = None;
    editor.server_validation = ValidationState::Idle;
    editor.diagnostics.clear();
    match &editor.schema {
        Some(schema) => match schema.validate_yaml(&editor.edited_yaml) {
            Ok(diagnostics) => {
                editor.diagnostics = diagnostics;
                if editor.diagnostics.is_empty() {
                    editor.validation_due = Some(ValidationDue::Debounce);
                }
            }
            Err(message) if message.starts_with("Unable to compile the Kubernetes schema:") => {
                // A malformed or unsupported OpenAPI extension must not prevent the API server
                // from validating the document. Treat the local schema as unavailable.
                editor.server_validation = ValidationState::Failed(message);
                editor.validation_due = Some(ValidationDue::Debounce);
            }
            Err(message) => editor.diagnostics.push(YamlDiagnostic {
                range: yaml_error_range(&editor.edited_yaml, &message),
                line: yaml_error_line(&message),
                path: String::new(),
                message,
            }),
        },
        None => match serde_yaml::from_str::<serde_yaml::Value>(&editor.edited_yaml) {
            Ok(_) => editor.validation_due = Some(ValidationDue::Debounce),
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

/// Start the debounce of an edit that armed it, from `now` (egui's frame time in seconds).
pub(super) fn start_validation_debounce(editor: &mut YamlEditorWindowState, now: f64) {
    if editor.validation_due == Some(ValidationDue::Debounce) {
        editor.validation_due = Some(ValidationDue::At(now + VALIDATION_DEBOUNCE.as_secs_f64()));
    }
}

/// Send the server validation once its debounce has elapsed at `now` (egui's frame time).
pub(super) fn maybe_request_server_validation(
    editor: &mut YamlEditorWindowState,
    now: f64,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    if editor
        .validation_due
        .is_some_and(|due| matches!(due, ValidationDue::At(at) if at <= now))
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

/// What the toolbar says while validation is still in progress.
pub(super) fn validation_activity(editor: &YamlEditorWindowState) -> Option<String> {
    if diagnostics_to_display(editor).1 {
        return Some("Validating updated YAML…".into());
    }
    match &editor.server_validation {
        ValidationState::Pending => Some(format!(
            "Validating against {} schema…",
            api_version(&editor.api_resource)
        )),
        ValidationState::Idle if editor.schema_loading => Some("Loading Kubernetes schema…".into()),
        _ => None,
    }
}

/// Whether the cluster has validated the edited document, which has no problems.
pub(super) fn is_valid(editor: &YamlEditorWindowState) -> bool {
    editor.server_validation == ValidationState::Valid
        && diagnostics_to_display(editor).0.is_empty()
}

/// The `apiVersion` of the resource: `v1`, `apps/v1`.
fn api_version(api_resource: &crate::api_resource::ApiResource) -> String {
    match api_resource.kube_group() {
        "" => api_resource.version.clone(),
        group => format!("{group}/{}", api_resource.version),
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
    // The panel sits under a divider of its own, apart from the code surface.
    let divider = egui::Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width(), size::HAIRLINE),
    );
    ui.painter()
        .rect_filled(divider, 0, ui.tokens().divider.base);
    ui.add_space(size::HAIRLINE);
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
    let active_id = egui::Id::new(("yaml-editor-active-problem", editor.id));
    let active = ctx
        .data(|data| data.get_temp::<usize>(active_id))
        .unwrap_or(0)
        .min(problems.len().saturating_sub(1));
    let response = ProblemsPanel::new(("yaml-editor", editor.id), &problems)
        .spacing(
            size::PROBLEMS_PADDING_EDITOR,
            size::PROBLEMS_LINE_COLUMN_EDITOR,
        )
        .active((!problems.is_empty()).then_some(active))
        .stale(showing_retained_diagnostics)
        .row_tooltip(if showing_retained_diagnostics {
            "Validating the updated YAML before this diagnostic can be located"
        } else {
            "Jump to the highlighted YAML location"
        })
        .show(ui, &mut open);
    ctx.data_mut(|data| data.insert_temp(open_id, open));
    if let Some(clicked) = response.clicked {
        ctx.data_mut(|data| data.insert_temp(active_id, clicked));
    }
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
    match confirm_discarding(
        ctx,
        egui::Id::new(("discard-yaml-changes", editor.id)),
        "Discard changes?",
        Prose::new().plain("Your unsaved edits will be lost."),
        "Discard changes",
    ) {
        ConfirmationDialogAction::Confirm => {
            editor.close_requested = true;
            editor.confirm_discard = false;
        }
        ConfirmationDialogAction::Cancel => editor.confirm_discard = false,
        ConfirmationDialogAction::None => {}
    }
}

/// The destructive "throw the edits away" confirmation both dialogs share.
fn confirm_discarding(
    ctx: &egui::Context,
    id: egui::Id,
    title: &str,
    message: Prose<'_>,
    confirm_label: &str,
) -> ConfirmationDialogAction {
    ConfirmationDialog {
        id,
        icon: icons::Icon::ExclamationTriangle,
        title,
        message,
        unavailable_message: None,
        cancel_label: "Keep editing",
        confirm_label,
        kind: ConfirmationDialogKind::Destructive,
        confirm_enabled: true,
        warning: None,
        acknowledgement: None,
    }
    .show(ctx)
}

/// The banner under the toolbar that says the resource changed on the cluster since the editor
/// opened it, with the two ways out: keep the edits elsewhere, or start over from the cluster.
pub(super) fn show_conflict_banner(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
) {
    let Some(conflict) = &editor.conflict else {
        return;
    };
    let versions = match &conflict.current_resource_version {
        Some(current) => format!("{} → {current}", editor.resource_version),
        None => editor.resource_version.clone(),
    };
    let message = format!(
        "This {} changed on the cluster after you opened it (resource version {versions}). \
         Your edits can’t be applied.",
        editor.api_resource.kind
    );
    let buttons = [
        BannerButton {
            label: "Copy my edits",
            icon: Some(icons::Icon::ClipboardDocument),
            variant: ButtonVariant::Secondary,
        },
        BannerButton {
            label: "Discard and reload",
            icon: None,
            variant: ButtonVariant::DangerQuiet,
        },
    ];
    match Banner::new(Tone::Danger, &message)
        .buttons(&buttons)
        .show(ui)
        .button
    {
        Some(0) => ctx.copy_text(editor.edited_yaml.clone()),
        Some(1) => editor.confirm_reload = true,
        _ => {}
    }
}

/// Confirm throwing the edits away for the version on the cluster, and fetch that version.
pub(super) fn show_reload_confirmation(
    ctx: &egui::Context,
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    if !editor.confirm_reload {
        return;
    }
    match confirm_discarding(
        ctx,
        egui::Id::new(("reload-yaml-changes", editor.id)),
        "Discard your edits?",
        Prose::new()
            .plain("Your changes to ")
            .strong(editor.resource_name.as_str())
            .plain(" will be lost and the latest version from the cluster will be loaded."),
        "Discard and reload",
    ) {
        ConfirmationDialogAction::Confirm => reload_editor(editor, commands_to_send),
        ConfirmationDialogAction::Cancel => editor.confirm_reload = false,
        ConfirmationDialogAction::None => {}
    }
}

/// Drop the edits and fetch the resource again; the fetched document is validated like a
/// freshly opened one.
pub(super) fn reload_editor(
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    editor.confirm_reload = false;
    // The conflict stays until the fetched version replaces the document, so a failed fetch
    // cannot re-enable Apply over the stale version.
    editor.error = None;
    editor.loading = true;
    editor.revalidate = true;
    // Results for the document being dropped must not match the fetched one.
    editor.validation_revision += 1;
    editor.validation_due = None;
    editor.server_validation = ValidationState::Idle;
    editor.diagnostics.clear();
    editor.retained_diagnostics.clear();
    editor.suggestions_visible = false;
    commands_to_send.push(Box::new(GetResourceYaml {
        editor_id: editor.id,
        cluster_key: editor.cluster_key,
        api_resource: editor.api_resource.clone(),
        namespace: editor.namespace.clone(),
        resource_name: editor.resource_name.clone(),
    }));
}

#[cfg(test)]
mod status_tests {
    use super::super::tests::editor;
    use super::*;

    #[test]
    fn valid_needs_the_cluster_to_have_validated_a_document_without_problems() {
        let mut editor = editor("kind: ConfigMap");
        for state in [
            ValidationState::Idle,
            ValidationState::Pending,
            ValidationState::Failed("no".into()),
        ] {
            editor.server_validation = state;
            assert!(!is_valid(&editor));
        }
        editor.server_validation = ValidationState::Valid;
        assert!(is_valid(&editor));
        editor.diagnostics = vec![YamlDiagnostic {
            range: None,
            line: Some(1),
            path: String::new(),
            message: "bad".into(),
        }];
        assert!(!is_valid(&editor));
    }

    #[test]
    fn the_toolbar_names_the_api_version_being_validated_against() {
        let mut editor = editor("kind: ConfigMap");
        assert_eq!(validation_activity(&editor), None);
        editor.server_validation = ValidationState::Pending;
        assert_eq!(
            validation_activity(&editor).as_deref(),
            Some("Validating against v1 schema…"),
            "the core group is just the version"
        );
        editor.api_resource.group = "apps".into();
        editor.api_resource.version = "v1".into();
        assert_eq!(
            validation_activity(&editor).as_deref(),
            Some("Validating against apps/v1 schema…")
        );
        editor.server_validation = ValidationState::Idle;
        editor.schema_loading = true;
        assert_eq!(
            validation_activity(&editor).as_deref(),
            Some("Loading Kubernetes schema…")
        );
    }
}
