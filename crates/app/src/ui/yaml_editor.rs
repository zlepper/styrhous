use super::state::{UiState, YamlEditorWindowState};
use crate::worker::WorkerCommand;
use components::colors::{TABLE_BORDER, TOOLBAR_BACKGROUND, gray};
use components::design::{spacing, status, surface, typography};
use components::{
    ConfirmationDialog, ConfirmationDialogAction, ConfirmationDialogKind, PointingHand,
    TailwindButton,
};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};

const TOOLBAR_HEIGHT: f32 = 52.0;

pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
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
            |window_ctx, _| show_editor_window(window_ctx, editor, commands_to_send),
        );
    }
    ui_state
        .yaml_editors
        .retain(|_, editor| !editor.close_requested);
}

fn show_editor_window(
    ctx: &egui::Context,
    editor: &mut YamlEditorWindowState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    if ctx.input(|input| input.viewport().close_requested()) {
        request_close(ctx, editor);
    }

    egui::TopBottomPanel::top("yaml-editor-header")
        .exact_height(TOOLBAR_HEIGHT)
        .frame(toolbar_frame())
        .show(ctx, |ui| {
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
                        request_close(ctx, editor);
                    }
                });
            });
        });

    egui::TopBottomPanel::bottom("yaml-editor-footer")
        .exact_height(40.0)
        .frame(toolbar_frame())
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Changes apply directly to the cluster")
                        .font(typography::body())
                        .color(gray::_600),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("⌘↵ Apply")
                            .font(typography::metadata())
                            .color(gray::_600),
                    );
                });
            });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(surface::TERMINAL_BACKGROUND)
                .inner_margin(egui::Margin::same(spacing::LG as i8)),
        )
        .show(ctx, |ui| {
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
                if let Some(error) = &editor.error {
                    error_strip(ui, error);
                } else if editor.is_modified() {
                    warning_strip(ui);
                }
                show_code_editor(ctx, ui, editor);
            }
        });

    if editor.is_ready()
        && editor.is_modified()
        && !editor.saving
        && ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter))
    {
        apply_editor(editor, commands_to_send);
    }
    show_discard_confirmation(ctx, editor);
}

fn toolbar_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(TOOLBAR_BACKGROUND)
        .stroke(egui::Stroke::new(1.0, TABLE_BORDER))
        .inner_margin(egui::Margin::symmetric(
            spacing::XL as i8,
            spacing::SM as i8,
        ))
}

fn resource_scope(editor: &YamlEditorWindowState) -> String {
    editor.namespace.as_deref().map_or_else(
        || format!("{} · Cluster-wide", editor.api_resource.kind),
        |namespace| format!("{} · {namespace}", editor.api_resource.kind),
    )
}

fn status_indicator(ui: &mut egui::Ui, color: egui::Color32, label: &str) {
    ui.label(
        egui::RichText::new("●")
            .font(typography::body())
            .color(color),
    );
    ui.label(
        egui::RichText::new(label)
            .font(typography::body())
            .color(gray::_600),
    );
}

fn warning_strip(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(47, 35, 12))
        .stroke(egui::Stroke::new(1.0, status::WARNING))
        .inner_margin(egui::Margin::symmetric(
            spacing::LG as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(
                    "Unsaved changes — apply to update the resource in the cluster.",
                )
                .font(typography::body())
                .color(egui::Color32::from_rgb(253, 230, 138)),
            );
        });
    ui.add_space(spacing::SM);
}

fn error_strip(ui: &mut egui::Ui, error: &str) {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(69, 10, 10))
        .stroke(egui::Stroke::new(1.0, status::DANGER))
        .inner_margin(egui::Margin::symmetric(
            spacing::LG as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(error)
                    .font(typography::body())
                    .color(egui::Color32::from_rgb(254, 202, 202)),
            );
        });
    ui.add_space(spacing::SM);
}

fn editor_error(ui: &mut egui::Ui, error: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(
            egui::RichText::new(error)
                .font(typography::body())
                .color(egui::Color32::from_rgb(254, 202, 202)),
        );
    });
}

fn show_code_editor(_ctx: &egui::Context, ui: &mut egui::Ui, editor: &mut YamlEditorWindowState) {
    let line_count = editor.edited_yaml.lines().count().max(1);
    egui::ScrollArea::both()
        .id_salt(("yaml-editor-scroll", editor.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(36.0);
                    for line in 1..=line_count {
                        ui.label(
                            egui::RichText::new(line.to_string())
                                .font(typography::monospace())
                                .color(gray::_500),
                        );
                    }
                });
                ui.separator();
                let theme = CodeTheme::dark(typography::MONOSPACE_SIZE);
                let mut layouter =
                    |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
                        let mut layout_job =
                            highlight(ui.ctx(), ui.style(), &theme, buffer.as_str(), "yaml");
                        layout_job.wrap.max_width = wrap_width;
                        ui.fonts_mut(|fonts| fonts.layout_job(layout_job))
                    };
                ui.add(
                    egui::TextEdit::multiline(&mut editor.edited_yaml)
                        .id_salt(("yaml-editor-text", editor.id))
                        .font(typography::monospace())
                        .code_editor()
                        .text_color(gray::_100)
                        .background_color(surface::TERMINAL_BACKGROUND)
                        .desired_width(f32::INFINITY)
                        .desired_rows(line_count)
                        .layouter(&mut layouter),
                );
            });
        });
}

fn apply_editor(editor: &mut YamlEditorWindowState, commands_to_send: &mut Vec<WorkerCommand>) {
    editor.saving = true;
    editor.error = None;
    commands_to_send.push(WorkerCommand::ApplyResourceYaml {
        editor_id: editor.id,
        cluster_key: editor.cluster_key,
        api_resource: editor.api_resource.clone(),
        namespace: editor.namespace.clone(),
        resource_name: editor.resource_name.clone(),
        yaml: editor.edited_yaml.clone(),
    });
}

fn request_close(ctx: &egui::Context, editor: &mut YamlEditorWindowState) {
    if editor.is_modified() {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        editor.confirm_discard = true;
    } else {
        editor.close_requested = true;
    }
}

fn show_discard_confirmation(ctx: &egui::Context, editor: &mut YamlEditorWindowState) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_resource::ApiResource;
    use egui_kittest::Harness;

    #[test]
    fn yaml_highlighting_uses_the_yaml_language() {
        let ctx = egui::Context::default();
        let theme = CodeTheme::dark(typography::MONOSPACE_SIZE);
        let job = highlight(
            &ctx,
            &egui::Style::default(),
            &theme,
            "kind: ConfigMap",
            "yaml",
        );

        assert!(!job.sections.is_empty());
    }

    #[test]
    fn yaml_editor_clean_snapshot() {
        snapshot_editor(
            editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings"),
            "yaml_editor/clean",
        );
    }

    #[test]
    fn yaml_editor_modified_snapshot() {
        let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
        editor.edited_yaml.push_str("\ndata:\n  mode: development");
        snapshot_editor(editor, "yaml_editor/modified");
    }

    #[test]
    fn yaml_editor_apply_error_snapshot() {
        let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
        editor.edited_yaml.push_str("\ndata:\n  mode: development");
        editor.error = Some("The Kubernetes API rejected this resource".into());
        snapshot_editor(editor, "yaml_editor/apply_error");
    }

    #[test]
    fn yaml_editor_discard_confirmation_snapshot() {
        let mut editor = editor("apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings");
        editor.edited_yaml.push_str("\ndata:\n  mode: development");
        editor.confirm_discard = true;
        snapshot_editor(editor, "yaml_editor/discard_confirmation");
    }

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
        }
    }

    fn snapshot_editor(editor: YamlEditorWindowState, name: &str) {
        let confirm_discard = editor.confirm_discard;
        let mut editor = editor;
        editor.confirm_discard = false;
        let mut harness = Harness::builder().build_state(
            |ctx, state: &mut SnapshotState| {
                show_editor_window(ctx, &mut state.editor, &mut state.commands);
            },
            SnapshotState {
                editor,
                commands: Vec::new(),
            },
        );
        components::test_support::setup_egui(&mut harness);
        harness.state_mut().editor.confirm_discard = confirm_discard;
        harness.run();
        harness.snapshot(name);
    }

    struct SnapshotState {
        editor: YamlEditorWindowState,
        commands: Vec<WorkerCommand>,
    }
}
