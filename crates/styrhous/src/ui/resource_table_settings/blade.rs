use super::*;

impl GlobalBladeContent for ResourceTableSettingsTarget {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        ui.label(
            egui::RichText::new("Configure columns")
                .font(components::design::typography::page_title())
                .color(components::colors::gray::_900),
        );
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

pub(super) fn show_column_row(
    ui: &mut egui::Ui,
    column: &mut EditableColumn,
    handle: &ReorderHandle,
) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, components::colors::WHITE);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, components::colors::TABLE_BORDER),
    );
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row.add_space(components::design::spacing::LG);
    let (handle_rect, _) = row.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
    let response = row.interact(
        handle_rect,
        row.id().with(("reorder", &column.definition.id)),
        egui::Sense::drag(),
    );
    row.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_label(format!("Reorder {} column", column.definition.label));
    });
    handle.register(&response);
    components::icons::bars_3(
        &mut row.new_child(egui::UiBuilder::new().max_rect(handle_rect)),
        16.0,
        components::colors::gray::_500,
    );
    row.add_space(components::design::spacing::MD);
    if components::tailwind_checkbox(&mut row, column.visible, &column.definition.label).clicked() {
        column.visible = !column.visible;
    }
    row.add_space(components::design::spacing::MD);
    row.label(
        egui::RichText::new(&column.definition.label)
            .font(components::design::typography::body())
            .color(components::colors::gray::_900),
    );
    if column.custom_column.is_some() {
        row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |row| {
            if TailwindButton::secondary("Remove")
                .size(ButtonSize::Xs)
                .accessibility_label(format!("Remove {} column", column.definition.label))
                .show(row)
                .clicked()
            {
                column.remove_requested = true;
            }
            if TailwindButton::secondary("Edit")
                .size(ButtonSize::Xs)
                .accessibility_label(format!("Edit {} column", column.definition.label))
                .show(row)
                .clicked()
            {
                column.edit_requested = true;
            }
        });
    }
}

pub(super) fn show_column_preview(ui: &mut egui::Ui, column: &EditableColumn) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, components::colors::WHITE);
    ui.painter().text(
        rect.left_center() + egui::vec2(48.0, 0.0),
        egui::Align2::LEFT_CENTER,
        &column.definition.label,
        components::design::typography::body(),
        components::colors::gray::_900,
    );
}

pub(super) fn show_custom_column_form(
    ui: &mut egui::Ui,
    draft: &mut CustomColumnDraft,
    suggestions: &MetadataKeySuggestions,
    preferences: &mut PersistedResourceTablePreferences,
    key: &ResourceTableKey,
    columns: &mut Vec<EditableColumn>,
) -> bool {
    let mut close = false;
    ui.add_space(components::design::spacing::XL);
    egui::Frame::new()
        .fill(components::colors::gray::_50)
        .stroke(components::design::surface::muted_border())
        .corner_radius(components::design::radius::surface())
        .inner_margin(egui::Margin::same(components::design::spacing::LG as i8))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(if draft.editing_id.is_some() {
                    "Edit custom column"
                } else {
                    "Add custom column"
                })
                .font(components::design::typography::section_heading())
                .color(components::colors::gray::_900),
            );
            ui.add_space(components::design::spacing::MD);
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
                    .font(components::design::typography::body())
                    .color(components::colors::gray::_600),
                );
            }
            ui.add_space(components::design::spacing::MD);
            labeled_text_edit(
                ui,
                "Column header",
                &mut draft.label,
                "For example: Application",
            );
            ui.add_space(components::design::spacing::LG);
            ui.horizontal(|ui| {
                let duplicate = draft.editing_id.is_none()
                    && preferences.custom_columns(key).iter().any(|column| {
                        column.source == draft.source && column.key == draft.key.trim()
                    });
                let valid = !draft.key.trim().is_empty() && !draft.label.trim().is_empty();
                let saved = ui
                    .add_enabled_ui(valid && !duplicate, |ui| {
                        TailwindButton::primary(if draft.editing_id.is_some() {
                            "Save column"
                        } else {
                            "Add column"
                        })
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                    })
                    .inner;
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
                if TailwindButton::secondary("Cancel")
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
                ui.add_space(components::design::spacing::SM);
                ui.label(
                    egui::RichText::new("This metadata key is already a column.")
                        .font(components::design::typography::body())
                        .color(components::design::status::DANGER),
                );
            }
        });
    close
}

fn labeled_text_edit(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(
        egui::RichText::new(label)
            .font(components::design::typography::body())
            .color(components::colors::gray::_700),
    );
    TailwindTextInput::new(value)
        .id_salt(("resource-table-custom-column-input", label))
        .hint_text(hint)
        .accessibility_label(label)
        .show(ui);
}

fn metadata_source_label(source: MetadataColumnSource) -> &'static str {
    super::super::metadata_fields::source_label(source)
}
