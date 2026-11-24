use super::*;
use components::tooltip::TooltipExt;

pub(super) fn show_resource_data(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    editor: Option<&mut super::super::state::ResourceDataEditorState>,
    pending_action: &mut Option<ResourceAction>,
) {
    let Some(editor) = editor else {
        return;
    };
    match &detail.payload {
        ResourceDetailPayload::ConfigMap(config_map) => {
            show_config_map_data(ui, config_map, editor, pending_action)
        }
        ResourceDetailPayload::Secret(secret) => {
            show_secret_data(ui, secret, editor, pending_action)
        }
        ResourceDetailPayload::Generic
        | ResourceDetailPayload::Diagnostic(_)
        | ResourceDetailPayload::Pod(_)
        | ResourceDetailPayload::Node(_) => {}
    }
}

pub(super) fn show_config_map_data(
    ui: &mut egui::Ui,
    config_map: &ConfigMapDetail,
    editor: &mut super::super::state::ResourceDataEditorState,
    pending_action: &mut Option<ResourceAction>,
) {
    let theme = ui.tokens();
    components::inspector_section_heading(
        ui,
        "Data",
        Some(&format!(
            "{} entries · {}",
            config_map.data.len(),
            if config_map.immutable {
                "Immutable"
            } else {
                "Mutable"
            }
        )),
    );
    if config_map.data.is_empty() {
        detail_message_card(ui, |ui| {
            ui.label(egui::RichText::new("No text data entries.").color(theme.text.muted));
        });
    }
    for key in config_map.data.keys() {
        let value = editor
            .draft_values
            .get(key)
            .expect("typed data detail and editor keys remain in sync")
            .clone();
        data_entry(
            ui,
            key,
            None,
            |ui| {
                if Button::secondary(format!("Copy {key}"))
                    .size(ButtonSize::Sm)
                    .show(ui)
                    .clicked()
                {
                    ui.ctx().copy_text(value);
                }
            },
            |ui| data_value_editor(ui, key, editor, config_map.immutable),
        );
    }
    data_save_controls(ui, editor, config_map.immutable, pending_action);
    ui.add_space(16.0);
}

pub(super) fn show_secret_data(
    ui: &mut egui::Ui,
    secret: &SecretDetail,
    editor: &mut super::super::state::ResourceDataEditorState,
    pending_action: &mut Option<ResourceAction>,
) {
    let theme = ui.tokens();
    components::inspector_section_heading(
        ui,
        "Data",
        Some(&format!(
            "{} entries · {} · {}",
            secret.data.len(),
            secret.type_,
            if secret.immutable {
                "Immutable"
            } else {
                "Mutable"
            }
        )),
    );
    if secret.data.is_empty() {
        detail_message_card(ui, |ui| {
            ui.label(egui::RichText::new("No data entries.").color(theme.text.muted));
        });
    }
    for (key, value) in &secret.data {
        let revealed = editor.revealed_secret_keys.contains(key);
        let mut visibility_toggled = false;
        let copy_value = (revealed && value.text.is_some())
            .then(|| editor.draft_values.get(key).cloned())
            .flatten();
        data_entry(
            ui,
            key,
            Some(value.byte_len),
            |ui| {
                if let Some(copy_value) = copy_value.as_ref()
                    && Button::secondary(format!("Copy {key}"))
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                {
                    ui.ctx().copy_text(copy_value.clone());
                }
                if value.text.is_some()
                    && Button::secondary(if revealed { "Hide" } else { "Reveal" })
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                {
                    visibility_toggled = true;
                }
            },
            |ui| match value.text.as_ref() {
                Some(_) if revealed => data_value_editor(ui, key, editor, secret.immutable),
                Some(_) => secret_value_mask(ui),
                None => unavailable_secret_value(ui),
            },
        );
        if visibility_toggled {
            if revealed {
                editor.revealed_secret_keys.remove(key);
            } else {
                editor.revealed_secret_keys.insert(key.clone());
            }
        }
    }
    data_save_controls(ui, editor, secret.immutable, pending_action);
    ui.add_space(16.0);
}

pub(super) fn detail_item_card(
    ui: &mut egui::Ui,
    add_header: impl FnOnce(&mut egui::Ui),
    add_content: impl FnOnce(&mut egui::Ui),
) {
    Card::new().padding(CARD_CONTENT_PADDING).show(ui, |ui| {
        add_header(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        add_content(ui);
    });
}

pub(super) fn detail_message_card(ui: &mut egui::Ui, add_content: impl FnOnce(&mut egui::Ui)) {
    Card::new()
        .padding(CARD_CONTENT_PADDING)
        .show(ui, add_content);
}

pub(super) fn data_entry(
    ui: &mut egui::Ui,
    key: &str,
    byte_len: Option<usize>,
    add_action: impl FnOnce(&mut egui::Ui),
    add_value: impl FnOnce(&mut egui::Ui),
) {
    let theme = ui.tokens();
    detail_item_card(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(key)
                        .monospace()
                        .strong()
                        .color(theme.text.primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    add_action(ui);
                    if let Some(byte_len) = byte_len {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("{byte_len} bytes"))
                                .font(type_scale::META.font_id(ui.ctx()))
                                .color(theme.text.muted),
                        );
                    }
                });
            });
        },
        add_value,
    );
    ui.add_space(8.0);
}

pub(super) fn data_value_editor(
    ui: &mut egui::Ui,
    key: &str,
    editor: &mut super::super::state::ResourceDataEditorState,
    immutable: bool,
) {
    let value = editor
        .draft_values
        .get_mut(key)
        .expect("typed data detail and editor keys remain in sync");
    let response = TextArea::new(value)
        .id_salt(("resource-data-value", key))
        .accessibility_label(format!("Value for {key}"))
        .monospace()
        .min_rows(3)
        .enabled(!immutable && !editor.saving)
        .show(ui);
    if immutable {
        response.disabled_tooltip("This resource's data is immutable.");
    }
}

pub(super) fn secret_value_mask(ui: &mut egui::Ui) {
    let theme = ui.tokens();
    egui::Frame::new()
        .fill(theme.components.field_bg)
        .stroke(stroke::subtle_divider(&theme))
        .corner_radius(egui::CornerRadius::same(radius::KEY))
        .inner_margin(egui::Margin::symmetric(
            (space::S8 + space::S4) as i8,
            space::S8 as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("••••••••••••")
                    .monospace()
                    .color(theme.text.secondary),
            );
        });
}

pub(super) fn unavailable_secret_value(ui: &mut egui::Ui) {
    let theme = ui.tokens();
    egui::Frame::new()
        .fill(theme.components.field_bg)
        .stroke(stroke::subtle_divider(&theme))
        .corner_radius(egui::CornerRadius::same(radius::KEY))
        .inner_margin(egui::Margin::symmetric(
            (space::S8 + space::S4) as i8,
            space::S8 as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Binary data")
                    .strong()
                    .color(theme.text.secondary),
            );
            ui.label(
                egui::RichText::new("This value cannot be edited in the inspector.")
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(theme.text.secondary),
            );
        });
}

pub(super) fn data_save_controls(
    ui: &mut egui::Ui,
    editor: &mut super::super::state::ResourceDataEditorState,
    immutable: bool,
    pending_action: &mut Option<ResourceAction>,
) {
    let theme = ui.tokens();
    if let Some(error) = &editor.save_error {
        ui.colored_label(theme.danger.text, error);
        ui.add_space(space::S8);
    }
    if immutable {
        ui.label(
            egui::RichText::new("Data is immutable and cannot be edited.").color(theme.text.muted),
        );
        return;
    }
    let (expected_values, updated_values) = editor.changed_values();
    let save_clicked = ui
        .horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                Button::primary(if editor.saving {
                    "Saving…"
                } else {
                    "Save data"
                })
                .size(ButtonSize::Sm)
                .enabled(!editor.saving && !updated_values.is_empty())
                .show(ui)
            })
            .inner
        })
        .inner
        .clicked();
    if save_clicked && pending_action.is_none() {
        editor.saving = true;
        editor.save_error = None;
        *pending_action = Some(ResourceAction::SaveData {
            expected_values,
            updated_values,
        });
    }
}

pub(super) fn show_data_conflict_dialog(
    ctx: &egui::Context,
    editor: Option<&mut super::super::state::ResourceDataEditorState>,
) {
    let Some(editor) = editor else {
        return;
    };
    if editor.pending_external_values.is_none() {
        return;
    }
    use components::design::theme::Tone;
    use components::icons::Icon;
    use components::{Dialog, Prose};
    let response = Dialog::new(
        egui::Id::new("resource-data-conflict-dialog"),
        Icon::ArrowPath,
        Tone::Warning,
        "Data changed on cluster",
    )
    .description(
        Prose::new()
            .plain("This resource changed on the cluster while you have unsaved data edits."),
    )
    .show(
        ctx,
        |_| {},
        |ui| {
            let keep = Button::primary("Keep my edits").show(ui).clicked();
            let use_cluster = Button::secondary("Use cluster version").show(ui).clicked();
            (use_cluster, keep)
        },
    );
    // Escape or a click on the scrim keeps the edits: the non-destructive choice.
    let (use_external, keep) = response.inner;
    let keep_local = keep || response.dismissed;
    if use_external {
        editor.use_external_values();
    } else if keep_local {
        editor.keep_local_edits();
    }
}
