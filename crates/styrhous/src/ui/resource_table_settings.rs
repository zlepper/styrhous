use super::global_blade::{GlobalBladeContent, GlobalBladeRenderContext, GlobalBladeRenderResult};
use super::table_preferences::{
    CustomMetadataColumn, MetadataColumnSource, PersistedResourceTablePreferences,
    ResourceTableKey, TableColumnDefinition,
};
use components::{
    ButtonSize, MoreMenu, ReorderHandle, ReorderableTable, TailwindButton, TailwindCombobox,
    TailwindTextInput,
};
use std::cell::RefCell;

#[derive(Debug, Clone, Default)]
pub(super) struct MetadataKeySuggestions {
    pub(super) labels: Vec<String>,
    pub(super) annotations: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ResourceTableSettingsTarget {
    key: ResourceTableKey,
    columns: Vec<EditableColumn>,
    resource_detail_owner: Option<u64>,
    supports_custom_columns: bool,
    metadata_key_suggestions: MetadataKeySuggestions,
    custom_column_draft: Option<CustomColumnDraft>,
}

#[derive(Debug, Clone)]
struct EditableColumn {
    definition: TableColumnDefinition,
    visible: bool,
    custom_column: Option<CustomMetadataColumn>,
    edit_requested: bool,
    remove_requested: bool,
}

#[derive(Debug, Clone)]
struct CustomColumnDraft {
    editing_id: Option<String>,
    source: MetadataColumnSource,
    key: String,
    label: String,
}

pub(super) fn target(
    preferences: &mut PersistedResourceTablePreferences,
    key: ResourceTableKey,
    definitions: &[TableColumnDefinition],
) -> ResourceTableSettingsTarget {
    target_with_options(
        preferences,
        key,
        definitions,
        MetadataKeySuggestions::default(),
        false,
    )
}

pub(super) fn show_configurable_table_header(
    menu: &mut MoreMenu<'_>,
    sortable: bool,
    id: &str,
    table_key: &ResourceTableKey,
    column_definitions: &[TableColumnDefinition],
    table_preferences: &RefCell<&mut PersistedResourceTablePreferences>,
    column_settings: &mut Option<ResourceTableSettingsTarget>,
) {
    if sortable {
        if menu.action("Sort ascending").clicked() {
            table_preferences.borrow_mut().set_sort(
                table_key,
                column_definitions,
                id,
                components::SortDirection::Ascending,
            );
        }
        if menu.action("Sort descending").clicked() {
            table_preferences.borrow_mut().set_sort(
                table_key,
                column_definitions,
                id,
                components::SortDirection::Descending,
            );
        }
        menu.separator();
    }
    if menu.action("Configure columns").clicked() {
        *column_settings = Some(target(
            &mut table_preferences.borrow_mut(),
            table_key.clone(),
            column_definitions,
        ));
    }
}

pub(super) fn target_with_metadata_key_suggestions(
    preferences: &mut PersistedResourceTablePreferences,
    key: ResourceTableKey,
    definitions: &[TableColumnDefinition],
    metadata_key_suggestions: MetadataKeySuggestions,
) -> ResourceTableSettingsTarget {
    target_with_options(
        preferences,
        key,
        definitions,
        metadata_key_suggestions,
        true,
    )
}

fn target_with_options(
    preferences: &mut PersistedResourceTablePreferences,
    key: ResourceTableKey,
    definitions: &[TableColumnDefinition],
    metadata_key_suggestions: MetadataKeySuggestions,
    supports_custom_columns: bool,
) -> ResourceTableSettingsTarget {
    let custom_columns = if supports_custom_columns {
        preferences.custom_columns(&key)
    } else {
        Vec::new()
    };
    let columns = preferences
        .all_columns(&key, definitions)
        .into_iter()
        .map(|(definition, visible)| EditableColumn {
            custom_column: custom_columns
                .iter()
                .find(|column| column.id() == definition.id)
                .cloned(),
            definition,
            visible,
            edit_requested: false,
            remove_requested: false,
        })
        .collect();
    ResourceTableSettingsTarget {
        key,
        columns,
        resource_detail_owner: None,
        supports_custom_columns,
        metadata_key_suggestions,
        custom_column_draft: None,
    }
}

impl ResourceTableSettingsTarget {
    pub(super) fn set_resource_detail_owner(&mut self, history_entry_id: u64) {
        self.resource_detail_owner = Some(history_entry_id);
    }
}

pub(super) fn show_target(
    ui: &mut egui::Ui,
    target: &mut ResourceTableSettingsTarget,
    preferences: &mut PersistedResourceTablePreferences,
) {
    ui.label(
        egui::RichText::new(
            "Choose which columns are visible and drag rows to change their order.",
        )
        .font(components::design::typography::body())
        .color(components::colors::gray::_600),
    );
    ui.add_space(components::design::spacing::XL);
    if target.supports_custom_columns {
        if TailwindButton::soft("Add custom column")
            .size(ButtonSize::Sm)
            .show(ui)
            .clicked()
        {
            target.custom_column_draft = Some(CustomColumnDraft {
                editing_id: None,
                source: MetadataColumnSource::Label,
                key: String::new(),
                label: String::new(),
            });
        }
        ui.add_space(components::design::spacing::LG);
    }
    let width = ui.available_width();
    let moved = ReorderableTable::new(("resource-table-settings-rows", &target.key), 44.0).show(
        ui,
        &mut target.columns,
        width,
        |ui, column, _index, handle| show_column_row(ui, column, handle),
        show_column_preview,
    );
    let removed_ids = target
        .columns
        .iter()
        .filter(|column| column.remove_requested)
        .map(|column| column.definition.id.clone())
        .collect::<Vec<_>>();
    for id in removed_ids {
        preferences.remove_custom_column(&target.key, &id);
    }
    target.columns.retain(|column| !column.remove_requested);
    if target.custom_column_draft.is_none()
        && let Some(column) = target.columns.iter().find(|column| column.edit_requested)
        && let Some(custom_column) = &column.custom_column
    {
        target.custom_column_draft = Some(CustomColumnDraft {
            editing_id: Some(custom_column.id()),
            source: custom_column.source,
            key: custom_column.key.clone(),
            label: custom_column.label.clone(),
        });
    }
    for column in &mut target.columns {
        column.edit_requested = false;
    }
    let close_custom_column_form = if let Some(draft) = &mut target.custom_column_draft {
        show_custom_column_form(
            ui,
            draft,
            &target.metadata_key_suggestions,
            preferences,
            &target.key,
            &mut target.columns,
        )
    } else {
        false
    };
    if close_custom_column_form {
        target.custom_column_draft = None;
    }
    let definitions = target
        .columns
        .iter()
        .map(|column| column.definition.clone())
        .collect::<Vec<_>>();
    for column in &target.columns {
        preferences.set_visible(
            &target.key,
            &definitions,
            &column.definition.id,
            column.visible,
        );
    }
    let persisted_columns = preferences.all_columns(&target.key, &definitions);
    for column in &mut target.columns {
        if let Some((_, visible)) = persisted_columns
            .iter()
            .find(|(definition, _)| definition.id == column.definition.id)
        {
            column.visible = *visible;
        }
    }
    if moved {
        preferences.set_order(
            &target.key,
            &definitions,
            &target
                .columns
                .iter()
                .map(|column| column.definition.id.clone())
                .collect::<Vec<_>>(),
        );
    }
}

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

fn show_column_row(ui: &mut egui::Ui, column: &mut EditableColumn, handle: &ReorderHandle) {
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

fn show_column_preview(ui: &mut egui::Ui, column: &EditableColumn) {
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

fn show_custom_column_form(
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
                ui.horizontal(|ui| {
                    ui.radio_value(&mut draft.source, MetadataColumnSource::Label, "Label");
                    ui.radio_value(
                        &mut draft.source,
                        MetadataColumnSource::Annotation,
                        "Annotation",
                    );
                });
                ui.add_space(components::design::spacing::MD);
                let source_keys = match draft.source {
                    MetadataColumnSource::Label => &suggestions.labels,
                    MetadataColumnSource::Annotation => &suggestions.annotations,
                };
                if !source_keys.is_empty() {
                    let selected_text = if draft.key.is_empty() {
                        "Choose a key found in this table".to_owned()
                    } else {
                        draft.key.clone()
                    };
                    let response = TailwindCombobox::from_label("Suggested metadata key")
                        .placeholder("Search keys...")
                        .search_accessibility_label("Search metadata keys")
                        .selected_text(selected_text)
                        .width(ui.available_width())
                        .filter_by(|value: &String| value)
                        .show_items(ui, source_keys, |combobox, value| {
                            if combobox.item(value, *value == draft.key).clicked() {
                                draft.key = value.clone();
                                if draft.label.is_empty() {
                                    draft.label = value.clone();
                                }
                            }
                        });
                    let _ = response;
                    ui.add_space(components::design::spacing::SM);
                }
                labeled_text_edit(
                    ui,
                    "Metadata key",
                    &mut draft.key,
                    "Enter an exact metadata key",
                );
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
    match source {
        MetadataColumnSource::Label => "Label",
        MetadataColumnSource::Annotation => "Annotation",
    }
}
