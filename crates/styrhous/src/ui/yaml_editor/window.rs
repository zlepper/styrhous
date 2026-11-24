use super::*;

pub(super) fn show_editor_window_inner(
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
    #[cfg(any(test, feature = "benchmarks"))] scroll_metrics: Option<&mut YamlEditorScrollMetrics>,
) {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|input| input.time);
    if ctx.input(|input| input.viewport().close_requested()) {
        request_close(&ctx, editor);
    }
    let theme = ui.tokens();
    let search_matches = yaml_search_matches(editor);
    reconcile_search_state(editor, search_matches.as_ref().ok());

    egui::Panel::top("yaml-editor-header")
        .show_separator_line(false)
        .exact_size(
            size::HEADER_BAR
                + size::TOOLBAR
                + if editor.conflict.is_some() {
                    size::BANNER
                } else {
                    0.0
                },
        )
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let scope = resource_scope(editor);
            let status = if editor.saving {
                Some(("Applying…", Tone::Neutral))
            } else if editor.conflict.is_some() {
                Some(("Conflict", Tone::Danger))
            } else if editor.is_modified() {
                Some(("Modified", Tone::Warning))
            } else {
                None
            };
            let mut header = WindowHeader::new(
                ("yaml-editor", editor.id),
                icons::Icon::PencilSquare,
                &editor.resource_name,
            )
            .meta(&scope);
            if let Some((text, tone)) = status {
                header = header.status(text, tone);
            }
            let (close, apply) = header
                .show(ui, |ui| {
                    let close_label = if editor.is_modified() {
                        "Discard"
                    } else {
                        "Close"
                    };
                    let close = Button::secondary(close_label).show(ui).clicked();
                    let apply = Button::primary("Apply changes")
                        .enabled(
                            editor.is_modified() && !editor.saving && editor.conflict.is_none(),
                        )
                        .disabled_reason(if editor.saving {
                            "Applying the changes…"
                        } else if editor.conflict.is_some() {
                            "The resource changed on the cluster"
                        } else {
                            "No changes to apply"
                        })
                        .show(ui)
                        .clicked();
                    (close, apply)
                })
                .inner;
            if apply {
                apply_editor(editor, commands_to_send);
            }
            if close {
                request_close(&ctx, editor);
            }
            let validation = validation_activity(editor);
            Toolbar::new(("yaml-editor", editor.id)).show(
                ui,
                |ui| show_search_controls(&ctx, ui, editor, &search_matches),
                |ui| {
                    if let Some(text) = &validation {
                        toolbar_activity(ui, text);
                    }
                },
            );
            show_conflict_banner(&ctx, ui, editor);
        });

    egui::Panel::bottom("yaml-editor-footer")
        .show_separator_line(false)
        .exact_size(size::STATUS_BAR)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let problem_count = diagnostics_to_display(editor).0.len();
            let position = cursor_position(&ctx, editor);
            let toggled = StatusBar::new(("yaml-editor", editor.id))
                .show(
                    ui,
                    |ui| {
                        status_icon(ui, icons::Icon::InformationCircle);
                        status_text(ui, "Changes apply directly to the cluster");
                    },
                    |ui| {
                        let toggled =
                            problem_count > 0 && problems_toggle(ui, problem_count).clicked();
                        if is_valid(editor) {
                            status_ok(ui, "Valid");
                        }
                        if let Some((line, column)) = position {
                            status_number(ui, &format!("Ln {line}, Col {column}"));
                        }
                        status_text(ui, "YAML");
                        status_text(ui, "UTF-8");
                        toggled
                    },
                )
                .1;
            if toggled {
                toggle_problems(&ctx, editor);
            }
        });

    if !diagnostics_to_display(editor).0.is_empty() {
        egui::Panel::bottom("yaml-editor-diagnostics")
            .show_separator_line(false)
            .frame(egui::Frame::NONE)
            .show(ui, |ui| show_diagnostics(&ctx, ui, editor));
    }

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme.code.surface))
        .show(ui, |ui| {
            if editor.loading {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading…");
                });
            } else if editor.original_yaml.is_none() {
                editor_error(
                    ui,
                    editor.error.as_deref().unwrap_or("Unable to load resource"),
                );
            } else {
                if (editor.validation_revision == 0 || editor.revalidate)
                    && !editor.edited_yaml.is_empty()
                {
                    refresh_local_validation(editor);
                }
                if let Some(error) = &editor.error {
                    error_strip(ui, error);
                }
                if let ValidationState::Failed(message) = &editor.server_validation {
                    error_strip(ui, message);
                }
                if show_code_editor(
                    &ctx,
                    ui,
                    editor,
                    search_matches.as_ref().ok(),
                    #[cfg(any(test, feature = "benchmarks"))]
                    scroll_metrics,
                ) {
                    refresh_local_validation(editor);
                }
            }
        });

    start_validation_debounce(editor, now);
    maybe_request_server_validation(editor, now, commands_to_send);
    if let Some(ValidationDue::At(due)) = editor.validation_due {
        ctx.request_repaint_after_secs((due - now).max(0.0) as f32);
    }
    show_discard_confirmation(&ctx, editor);
    show_reload_confirmation(&ctx, editor, commands_to_send);
}
