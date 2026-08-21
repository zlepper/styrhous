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
            yaml: editor.edited_yaml.clone(),
        }));
    }
}

pub(super) fn show_diagnostics(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
) {
    let (diagnostics, showing_retained_diagnostics) = diagnostics_to_display(editor);
    if let ValidationState::Failed(message) = &editor.server_validation {
        error_strip(ui, message);
    }
    if diagnostics.is_empty() {
        match &editor.server_validation {
            ValidationState::Pending => {
                status_indicator(ui, gray::_400, "Validating with cluster…")
            }
            ValidationState::Valid => status_indicator(ui, status::SUCCESS, "Validated by cluster"),
            ValidationState::Failed(_) => {}
            ValidationState::Idle => {
                if editor.schema_loading {
                    status_indicator(ui, gray::_400, "Loading Kubernetes schema…");
                }
            }
        }
        return;
    }
    let mut range_to_focus = None;
    egui::CollapsingHeader::new(format!("{} diagnostics", diagnostics.len()))
        .default_open(true)
        .show(ui, |ui| {
            if showing_retained_diagnostics {
                status_indicator(ui, gray::_400, "Validating updated YAML…");
            }
            components::scroll::vertical()
                .id_salt(("yaml-editor-diagnostic-list", editor.id))
                .max_height(DIAGNOSTIC_LIST_MAX_HEIGHT)
                .min_scrolled_height(DIAGNOSTIC_LIST_MAX_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for diagnostic in diagnostics {
                        let location = diagnostic
                            .line
                            .map(|line| format!("Line {line}: "))
                            .unwrap_or_default();
                        let button = egui::Button::new(
                            egui::RichText::new(format!("{location}{}", diagnostic.message))
                                .font(typography::metadata())
                                .color(status::DANGER),
                        )
                        .frame(false);
                        let response = if showing_retained_diagnostics {
                            ui.add_enabled(false, button).on_hover_text(
                                "Validating the updated YAML before this diagnostic can be located",
                            )
                        } else {
                            ui.add(button)
                                .with_pointing_hand()
                                .on_hover_text("Jump to the highlighted YAML location")
                        };
                        if !showing_retained_diagnostics && response.clicked() {
                            range_to_focus = diagnostic.range.clone();
                        }
                    }
                });
        });
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

pub(super) fn focus_diagnostic(
    ctx: &egui::Context,
    editor: &mut YamlEditorWindowState,
    range: SourceRange,
) {
    let text_edit_id = yaml_editor_text_edit_id(editor.id);
    ctx.memory_mut(|memory| memory.request_focus(text_edit_id));
    let mut state =
        egui::widgets::text_edit::TextEditState::load(ctx, text_edit_id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(range.start),
        CCursor::new(range.end),
    )));
    state.store(ctx, text_edit_id);
    editor.scroll_to_diagnostic = Some(range);
    ctx.request_repaint();
}

pub(super) fn has_diagnostics_feedback(editor: &YamlEditorWindowState) -> bool {
    !diagnostics_to_display(editor).0.is_empty()
        || editor.schema_loading
        || !matches!(editor.server_validation, ValidationState::Idle)
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
        yaml: editor.edited_yaml.clone(),
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
        eyebrow: "Unsaved changes",
        title: "Discard changes?",
        message: "Your unsaved edits will be lost.",
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
