use super::*;

pub(super) fn show_editor_window_inner(
    ui: &mut egui::Ui,
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
    #[cfg(any(test, feature = "benchmarks"))] scroll_metrics: Option<&mut YamlEditorScrollMetrics>,
) {
    let ctx = ui.ctx().clone();
    if ctx.input(|input| input.viewport().close_requested()) {
        request_close(&ctx, editor);
    }
    let search_matches = yaml_search_matches(editor);
    reconcile_search_state(editor, search_matches.as_ref().ok());

    egui::Panel::top("yaml-editor-header")
        .exact_size(TOOLBAR_HEIGHT)
        .frame(toolbar_frame())
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Edit")
                        .font(typography::section_heading())
                        .color(gray::_900),
                );
                ui.add_space(spacing::LG);
                ui.label(
                    egui::RichText::new(&editor.resource_name)
                        .font(typography::section_heading())
                        .color(gray::_900),
                );
                ui.add_space(spacing::MD);
                ui.label(
                    egui::RichText::new(resource_scope(editor))
                        .font(typography::body())
                        .color(gray::_600),
                );
                if editor.saving {
                    ui.add_space(spacing::MD);
                    status_indicator(ui, gray::_400, "Applying…");
                } else if editor.is_modified() {
                    ui.add_space(spacing::MD);
                    status_indicator(ui, status::WARNING, "Modified");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let apply_clicked = ui
                        .add_enabled_ui(editor.is_modified() && !editor.saving, |ui| {
                            TailwindButton::primary("Apply changes")
                                .show(ui)
                                .with_pointing_hand()
                                .clicked()
                        })
                        .inner;
                    if apply_clicked {
                        apply_editor(editor, commands_to_send);
                    }
                    let close_label = if editor.is_modified() {
                        "Discard"
                    } else {
                        "Close"
                    };
                    if TailwindButton::secondary(close_label)
                        .show(ui)
                        .with_pointing_hand()
                        .clicked()
                    {
                        request_close(&ctx, editor);
                    }
                    ui.add_space(spacing::MD);
                    show_search_controls(&ctx, ui, editor, &search_matches);
                });
            });
        });

    egui::Panel::bottom("yaml-editor-footer")
        .exact_size(40.0)
        .frame(toolbar_frame())
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Changes apply directly to the cluster")
                        .font(typography::body())
                        .color(gray::_600),
                );
            });
        });

    if has_diagnostics_feedback(editor) {
        egui::Panel::bottom("yaml-editor-diagnostics")
            .frame(toolbar_frame())
            .show(ui, |ui| show_diagnostics(&ctx, ui, editor));
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(surface::TERMINAL_BACKGROUND)
                .inner_margin(egui::Margin::same(spacing::LG as i8)),
        )
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
                if editor.validation_revision == 0 && !editor.edited_yaml.is_empty() {
                    refresh_local_validation(editor);
                }
                if let Some(error) = &editor.error {
                    error_strip(ui, error);
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

    maybe_request_server_validation(editor, commands_to_send);
    #[cfg(not(test))]
    if let Some(due) = editor.validation_due {
        ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
    }
    show_discard_confirmation(&ctx, editor);
}
