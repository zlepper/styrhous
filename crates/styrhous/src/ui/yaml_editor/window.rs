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
    let theme = ui.tokens();
    let search_matches = yaml_search_matches(editor);
    reconcile_search_state(editor, search_matches.as_ref().ok());

    egui::Panel::top("yaml-editor-header")
        .exact_size(TOOLBAR_HEIGHT)
        .frame(toolbar_frame(&theme))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // The title and the scope share the title's baseline.
                let baseline = type_scale::TITLE
                    .centred_baseline(ui.available_height(), ui.ctx().pixels_per_point());
                type_scale::TITLE.label_on_baseline(ui, "Edit", theme.text.primary, baseline);
                ui.add_space(space::S16);
                type_scale::TITLE.label_on_baseline(
                    ui,
                    &editor.resource_name,
                    theme.text.primary,
                    baseline,
                );
                ui.add_space(space::S12);
                type_scale::BODY.label_on_baseline(
                    ui,
                    &resource_scope(editor),
                    theme.text.secondary,
                    baseline,
                );
                if editor.saving {
                    ui.add_space(space::S12);
                    status_indicator(ui, theme.status.neutral, "Applying…");
                } else if editor.is_modified() {
                    ui.add_space(space::S12);
                    status_indicator(ui, theme.warning.fill, "Modified");
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
                    ui.add_space(space::S12);
                    show_search_controls(&ctx, ui, editor, &search_matches);
                });
            });
        });

    egui::Panel::bottom("yaml-editor-footer")
        .exact_size(40.0)
        .frame(toolbar_frame(&theme))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Changes apply directly to the cluster")
                        .font(type_scale::BODY.font_id(ui.ctx()))
                        .color(theme.text.secondary),
                );
            });
        });

    if has_diagnostics_feedback(editor) {
        egui::Panel::bottom("yaml-editor-diagnostics")
            .frame(toolbar_frame(&theme))
            .show(ui, |ui| show_diagnostics(&ctx, ui, editor));
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme.code.surface)
                .inner_margin(egui::Margin::same(space::S16 as i8)),
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
