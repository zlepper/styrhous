use super::*;

pub(super) fn show_detail(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    usage: PodUsageDisplay<'_>,
    node_usage: NodeUsageDisplay<'_>,
    pending_action: &mut Option<ResourceAction>,
) {
    show_generic_summary(ui, detail);
    ui.add_space(13.0);
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        show_pod_summary(ui, detail, pod, pending_action);
        ui.add_space(13.0);
        show_pod_detail(ui, pod, usage);
    } else if let ResourceDetailPayload::Node(node) = &detail.payload {
        show_node_detail(ui, node, node_usage);
    } else if let ResourceDetailPayload::Diagnostic(diagnostic) = &detail.payload {
        show_diagnostic_detail(ui, diagnostic);
    }
}

pub(super) fn show_node_detail(ui: &mut egui::Ui, node: &NodeDetail, usage: NodeUsageDisplay<'_>) {
    let pod_cidrs = if node.pod_cidrs.is_empty() {
        "-".to_owned()
    } else {
        node.pod_cidrs.join(", ")
    };
    let taints = if node.taints.is_empty() {
        "None".to_owned()
    } else {
        node.taints.join(", ")
    };
    InspectorDetails::show_titled_properties(
        ui,
        "Spec",
        &[DetailRow::new([
            DetailCell::status(
                "Scheduling",
                if node.unschedulable {
                    "Scheduling disabled"
                } else {
                    "Schedulable"
                },
                if node.unschedulable {
                    DetailTone::Warning
                } else {
                    DetailTone::Success
                },
            ),
            DetailCell::new("Provider ID", node.provider_id.as_deref().unwrap_or("-")).copyable(),
            DetailCell::new("Pod CIDRs", pod_cidrs.as_str()).copyable(),
            DetailCell::new("Taints", taints.as_str()).copyable(),
        ])],
    );
    ui.add_space(13.0);
    show_node_usage(ui, usage, node.allocatable);
}

pub(super) fn show_generic_summary(ui: &mut egui::Ui, detail: &ResourceDetail) {
    detail_summary_card(ui, |ui| {
        InspectorDetails::show_properties(
            ui,
            &[
                DetailRow::new([
                    DetailCell::new("Kind", detail.api_resource.kind.as_str()).copyable(),
                    DetailCell::new("Name", detail.name.as_str()).copyable(),
                    detail.namespace.as_deref().map_or_else(
                        || DetailCell::new("Namespace", "Cluster-wide"),
                        |namespace| DetailCell::new("Namespace", namespace).copyable(),
                    ),
                ]),
                DetailRow::new([
                    DetailCell::new("UID", detail.uid.as_str()).copyable(),
                    DetailCell::new("Resource version", detail.resource_version.as_str())
                        .copyable(),
                    DetailCell::new("Age", format_age(detail.creation_timestamp)),
                ]),
            ],
        );
    });
}

pub(super) fn show_diagnostic_detail(ui: &mut egui::Ui, diagnostic: &DiagnosticDetail) {
    for (index, section) in diagnostic.sections.iter().enumerate() {
        InspectorDetails::show_titled_properties(
            ui,
            section.title.as_str(),
            &[DetailRow::new(section.fields.iter().map(|field| {
                DetailCell::new(field.label.as_str(), field.value.as_str()).copyable()
            }))],
        );
        if index + 1 < diagnostic.sections.len() {
            ui.add_space(CARD_GAP);
        }
    }
}

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
    section_header(
        ui,
        "Data",
        Some(format!(
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
            ui.label(egui::RichText::new("No text data entries.").color(gray::_500));
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
                if TailwindButton::secondary(format!("Copy {key}"))
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
    section_header(
        ui,
        "Data",
        Some(format!(
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
            ui.label(egui::RichText::new("No data entries.").color(gray::_500));
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
                    && TailwindButton::secondary(format!("Copy {key}"))
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                {
                    ui.ctx().copy_text(copy_value.clone());
                }
                if value.text.is_some()
                    && TailwindButton::secondary(if revealed { "Hide" } else { "Reveal" })
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

pub(super) fn detail_summary_card(ui: &mut egui::Ui, add_content: impl FnOnce(&mut egui::Ui)) {
    WorkspaceCard::new().padding(18).show(ui, add_content);
}

pub(super) fn detail_item_card(
    ui: &mut egui::Ui,
    add_header: impl FnOnce(&mut egui::Ui),
    add_content: impl FnOnce(&mut egui::Ui),
) {
    WorkspaceCard::new()
        .padding(CARD_CONTENT_PADDING)
        .show(ui, |ui| {
            add_header(ui);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            add_content(ui);
        });
}

pub(super) fn detail_message_card(ui: &mut egui::Ui, add_content: impl FnOnce(&mut egui::Ui)) {
    WorkspaceCard::new()
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
    detail_item_card(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(key)
                        .monospace()
                        .strong()
                        .color(gray::_800),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    add_action(ui);
                    if let Some(byte_len) = byte_len {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("{byte_len} bytes"))
                                .font(typography::metadata())
                                .color(gray::_500),
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
    let response = TailwindTextArea::new(value)
        .id_salt(("resource-data-value", key))
        .accessibility_label(format!("Value for {key}"))
        .monospace()
        .desired_rows(3)
        .enabled(!immutable && !editor.saving)
        .show(ui);
    if response.hovered() && immutable {
        response.on_hover_text("This resource's data is immutable.");
    }
}

pub(super) fn secret_value_mask(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(gray::_50)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM + spacing::XS) as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("••••••••••••")
                    .monospace()
                    .color(gray::_700),
            );
        });
}

pub(super) fn unavailable_secret_value(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(gray::_50)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM + spacing::XS) as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Binary data")
                    .strong()
                    .color(gray::_700),
            );
            ui.label(
                egui::RichText::new("This value cannot be edited in the inspector.")
                    .font(typography::metadata())
                    .color(gray::_600),
            );
        });
}

pub(super) fn data_save_controls(
    ui: &mut egui::Ui,
    editor: &mut super::super::state::ResourceDataEditorState,
    immutable: bool,
    pending_action: &mut Option<ResourceAction>,
) {
    if let Some(error) = &editor.save_error {
        ui.colored_label(status::DANGER, error);
        ui.add_space(spacing::SM);
    }
    if immutable {
        ui.label(egui::RichText::new("Data is immutable and cannot be edited.").color(gray::_500));
        return;
    }
    let (expected_values, updated_values) = editor.changed_values();
    let save_clicked = ui
        .horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!editor.saving && !updated_values.is_empty(), |ui| {
                    TailwindButton::primary(if editor.saving {
                        "Saving…"
                    } else {
                        "Save data"
                    })
                    .size(ButtonSize::Sm)
                    .show(ui)
                })
                .inner
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
    let mut use_external = false;
    let mut keep_local = false;
    egui::Window::new("Data changed on cluster")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label("This resource changed on the cluster while you have unsaved data edits.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Use cluster version")
                    .with_pointing_hand()
                    .clicked()
                {
                    use_external = true;
                }
                if ui.button("Keep my edits").with_pointing_hand().clicked() {
                    keep_local = true;
                }
            });
        });
    if use_external {
        editor.use_external_values();
    } else if keep_local {
        editor.keep_local_edits();
    }
}
