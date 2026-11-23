use super::*;

impl GlobalBladeContent for ResourceTableSettingsTarget {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        components::blade_title(ui, None, "Configure columns");
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        show_target(ui, self, context.table_preferences());
        GlobalBladeRenderResult::default()
    }

    fn is_owned_by_resource_detail(&self, history_entry_id: u64) -> bool {
        self.resource_detail_owner == Some(history_entry_id)
    }
}

/// Edit and Remove for a custom (label or annotation) column.
pub(super) fn show_custom_column_actions(ui: &mut egui::Ui, column: &mut EditableColumn) {
    if column.custom_column.is_none() {
        return;
    }
    if Button::secondary("Remove")
        .size(ButtonSize::Sm)
        .accessibility_label(format!("Remove {} column", column.definition.label))
        .show(ui)
        .clicked()
    {
        column.remove_requested = true;
    }
    if Button::secondary("Edit")
        .size(ButtonSize::Sm)
        .accessibility_label(format!("Edit {} column", column.definition.label))
        .show(ui)
        .clicked()
    {
        column.edit_requested = true;
    }
}

pub(super) fn show_custom_column_form(
    ui: &mut egui::Ui,
    draft: &mut CustomColumnDraft,
    suggestions: &MetadataKeySuggestions,
    preferences: &mut PersistedResourceTablePreferences,
    key: &ResourceTableKey,
    columns: &mut Vec<EditableColumn>,
) -> bool {
    let theme = ui.tokens();
    let mut close = false;
    ui.add_space(components::design::space::S24);
    Card::new().show_inner(ui, |ui| {
        ui.label(
            egui::RichText::new(if draft.editing_id.is_some() {
                "Edit custom column"
            } else {
                "Add custom column"
            })
            .font(components::design::type_scale::TITLE.font_id(ui.ctx()))
            .color(theme.text.primary),
        );
        ui.add_space(components::design::space::S12);
        if draft.editing_id.is_none() {
            if let Some(selected) = super::super::metadata_fields::show_metadata_key_picker(
                ui,
                &mut draft.source,
                &mut draft.key,
                suggestions,
                "Choose a key found in this table",
            ) && draft.label.is_empty()
            {
                draft.label = selected;
            }
        } else {
            ui.label(
                egui::RichText::new(format!(
                    "{}: {}",
                    metadata_source_label(draft.source),
                    draft.key
                ))
                .font(components::design::type_scale::BODY.font_id(ui.ctx()))
                .color(theme.text.secondary),
            );
        }
        ui.add_space(components::design::space::S12);
        labeled_text_edit(
            ui,
            "Column header",
            &mut draft.label,
            "For example: Application",
        );
        ui.add_space(components::design::space::S16);
        ui.horizontal(|ui| {
            let duplicate = draft.editing_id.is_none()
                && preferences
                    .custom_columns(key)
                    .iter()
                    .any(|column| column.source == draft.source && column.key == draft.key.trim());
            let valid = !draft.key.trim().is_empty() && !draft.label.trim().is_empty();
            let saved = Button::primary(if draft.editing_id.is_some() {
                "Save column"
            } else {
                "Add column"
            })
            .size(ButtonSize::Sm)
            .enabled(valid && !duplicate)
            .show(ui)
            .clicked();
            if saved {
                let source = draft.source;
                let metadata_key = draft.key.trim().to_owned();
                let label = draft.label.trim().to_owned();
                let changed = if let Some(id) = &draft.editing_id {
                    preferences.rename_custom_column(key, id, label.clone())
                } else {
                    preferences.add_custom_column(
                        key,
                        CustomMetadataColumn {
                            source,
                            key: metadata_key.clone(),
                            label: label.clone(),
                        },
                    )
                };
                if changed {
                    if draft.editing_id.is_none() {
                        let custom_column = CustomMetadataColumn {
                            source,
                            key: metadata_key,
                            label: label.clone(),
                        };
                        columns.push(EditableColumn {
                            definition: TableColumnDefinition {
                                id: custom_column.id(),
                                label,
                                default_width: 160.0,
                                sortable: true,
                                numeric: false,
                            },
                            visible: true,
                            custom_column: Some(custom_column),
                            edit_requested: false,
                            remove_requested: false,
                        });
                    } else if let Some(id) = &draft.editing_id
                        && let Some(column) = columns
                            .iter_mut()
                            .find(|column| column.definition.id == *id)
                    {
                        column.definition.label = label;
                        if let Some(custom_column) = &mut column.custom_column {
                            custom_column.label = column.definition.label.clone();
                        }
                    }
                }
                if changed {
                    close = true;
                }
            }
            if Button::secondary("Cancel")
                .size(ButtonSize::Sm)
                .show(ui)
                .clicked()
            {
                close = true;
            }
        });
        if draft.editing_id.is_none()
            && preferences
                .custom_columns(key)
                .iter()
                .any(|column| column.source == draft.source && column.key == draft.key.trim())
        {
            ui.add_space(components::design::space::S8);
            ui.label(
                egui::RichText::new("This metadata key is already a column.")
                    .font(components::design::type_scale::BODY.font_id(ui.ctx()))
                    .color(theme.danger.text),
            );
        }
    });
    close
}

fn labeled_text_edit(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(label)
            .font(components::design::type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.secondary),
    );
    TextInput::new(value)
        .id_salt(("resource-table-custom-column-input", label))
        .placeholder(hint)
        .accessibility_label(label)
        .show(ui);
}

fn metadata_source_label(source: MetadataColumnSource) -> &'static str {
    super::super::metadata_fields::source_label(source)
}
