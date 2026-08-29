use super::global_blade::{GlobalBladeContent, GlobalBladeRenderContext, GlobalBladeRenderResult};
pub(super) use super::metadata_fields::MetadataKeySuggestions;
use super::table_preferences::{
    CustomMetadataColumn, MetadataColumnSource, PersistedResourceTablePreferences,
    ResourceTableKey, TableColumnDefinition,
};
use components::{
    ButtonSize, MoreMenu, ReorderHandle, ReorderableTable, TailwindButton, TailwindTextInput,
};
use std::cell::RefCell;

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
    // Keep the active form adjacent to its trigger. Rendering it after the
    // complete column list leaves its submission controls below the blade
    // viewport once standard button sizing is applied.
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

mod blade;
use blade::*;
