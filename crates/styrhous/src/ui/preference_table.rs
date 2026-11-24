//! Tables whose column order, visibility, widths and sort live in the
//! persisted table preferences: the resource tables, the Helm tables and
//! the inspectors' sub-tables.

use super::resource_table_settings::{
    MetadataKeySuggestions, ResourceTableSettingsTarget, target,
    target_with_metadata_key_suggestions,
};
use super::table_preferences::{
    PersistedResourceTablePreferences, ResolvedTableColumn, ResourceTableKey, TableColumnDefinition,
};
use components::design::size;
use components::design::theme::Tone;
use components::icons::Icon;
use components::{
    CellAlign, GroupedTableResponse, Menu, MenuItem, MoreButton, SortDirection, SortState, Table,
    TableGroup, column_visibility_menu,
};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::hash::Hash;

/// The id of a row actions column: a centred "⋯" button under an empty
/// header (S08).
pub(super) const ACTIONS_COLUMN_ID: &str = "actions";
/// The id of a resource's Age column.
pub(super) const AGE_COLUMN_ID: &str = "age";
/// The width of a resource's Age column (C08).
const AGE_COLUMN_WIDTH: f32 = 64.0;
/// The narrowest the first (name) column starts.
const MIN_FIRST_COLUMN_WIDTH: f32 = 160.0;

/// A resource's age: numbers, sortable.
pub(super) fn age_column() -> TableColumnDefinition {
    TableColumnDefinition::sortable(AGE_COLUMN_ID, "Age", AGE_COLUMN_WIDTH).numeric()
}

/// The row actions column. It keeps its name for the column menus.
pub(super) fn actions_column() -> TableColumnDefinition {
    TableColumnDefinition {
        sortable: false,
        ..TableColumnDefinition::sortable(ACTIONS_COLUMN_ID, "Actions", size::TABLE_ACTIONS_COLUMN)
    }
}

/// Give the first column (the name) the width the others that show by
/// default leave in `available`, after `leading` (a checkbox column), but at
/// least [`MIN_FIRST_COLUMN_WIDTH`].
pub(super) fn fill_first_column(
    definitions: &mut [TableColumnDefinition],
    available: f32,
    leading: f32,
) {
    fill_column(definitions, 0, available, leading, MIN_FIRST_COLUMN_WIDTH);
}

/// [`fill_first_column`] for column `index`: it takes the width the others leave, but at
/// least `minimum`.
pub(super) fn fill_column(
    definitions: &mut [TableColumnDefinition],
    index: usize,
    available: f32,
    leading: f32,
    minimum: f32,
) {
    let fixed = leading
        + definitions
            .iter()
            .enumerate()
            .filter(|(position, column)| *position != index && !column.default_hidden)
            .map(|(_, column)| column.default_width)
            .sum::<f32>();
    let Some(column) = definitions.get_mut(index) else {
        return;
    };
    // The scroll bars float over the rows, so the columns take the full width.
    column.default_width = (available - fixed).max(minimum);
}

/// Where a table's column menus write: the preferences, and the column
/// settings to open. `metadata_key_suggestions` lets those settings add
/// label and annotation columns.
pub(super) struct ColumnControls<'a> {
    pub(super) preferences: &'a mut PersistedResourceTablePreferences,
    pub(super) metadata_key_suggestions: Option<&'a MetadataKeySuggestions>,
    pub(super) column_settings: &'a mut Option<ResourceTableSettingsTarget>,
}

/// The preferences shared by a table's callbacks while it shows.
type SharedPreferences<'p> = RefCell<&'p mut PersistedResourceTablePreferences>;

/// The visible columns and sort of one preference-backed table.
pub(super) struct PreferenceTable {
    key: ResourceTableKey,
    definitions: Vec<TableColumnDefinition>,
    /// The visible columns, in order, at their persisted widths.
    columns: Vec<ResolvedTableColumn>,
    /// The persisted sort.
    pub(super) sort: Option<SortState>,
    /// Columns [`Self::compact`] left out: not offered by the column menu.
    left_out: Vec<String>,
    /// Columns [`Self::compact`] gave a width: resizing them is not persisted.
    resized_by_compaction: Vec<String>,
}

impl PreferenceTable {
    pub(super) fn new(
        preferences: &mut PersistedResourceTablePreferences,
        key: ResourceTableKey,
        definitions: Vec<TableColumnDefinition>,
    ) -> Self {
        let columns = preferences.resolved_columns(&key, &definitions);
        let sort = preferences.sort(&key, &definitions);
        Self {
            key,
            definitions,
            columns,
            sort,
            left_out: Vec::new(),
            resized_by_compaction: Vec::new(),
        }
    }

    /// The id of the visible column at `index`.
    fn column_id(&self, index: usize) -> &str {
        &self.columns[index].definition.id
    }

    /// Whether `column_id` is the first visible column.
    pub(super) fn is_first_column(&self, column_id: &str) -> bool {
        self.columns
            .first()
            .is_some_and(|column| column.definition.id == column_id)
    }

    /// A table of the visible columns at their widths: sortable as defined,
    /// numbers right-aligned and the row actions centred under an empty
    /// header.
    pub(super) fn table(&self, id_source: impl Hash + std::fmt::Debug) -> Table {
        self.columns
            .iter()
            .fold(Table::new(id_source), |table, column| {
                let definition = &column.definition;
                let is_actions = definition.id == ACTIONS_COLUMN_ID;
                let header = if is_actions {
                    String::new()
                } else {
                    definition.label.clone()
                };
                // The actions column cannot be resized, so a width saved by an
                // earlier version must not stick.
                let width = if is_actions {
                    definition.default_width
                } else {
                    column.width
                };
                table.column(definition.id.clone(), header, |builder| {
                    let mut builder = builder.initial_width(width);
                    if definition.sortable {
                        builder = builder.sortable();
                    }
                    if definition.numeric {
                        builder = builder.numeric();
                    }
                    if is_actions {
                        builder = builder.align(CellAlign::Center);
                    }
                    builder
                })
            })
    }

    /// `rows` in the persisted sort, by `compare(left, right, column id,
    /// direction)`; in their own order when unsorted.
    pub(super) fn sorted<'r, T>(
        &self,
        rows: &'r [T],
        compare: impl Fn(&T, &T, &str, SortDirection) -> Ordering,
    ) -> Vec<&'r T> {
        let mut sorted = rows.iter().collect::<Vec<_>>();
        if let Some(sort) = &self.sort {
            sorted.sort_by(|left, right| compare(left, right, &sort.column_id, sort.direction));
        }
        sorted
    }

    /// Show `table` (from [`Self::table`]) over `rows`, persisting resized
    /// columns, header sorts and the header menus' choices.
    /// Leave out the columns `hide` names, and give the ones `resize` names
    /// their width, until the table is rebuilt: a window too narrow for
    /// every column (S08). The preferences are untouched.
    pub(super) fn compact(
        &mut self,
        hide: impl Fn(&str) -> bool,
        resize: impl Fn(&str) -> Option<f32>,
    ) {
        let (kept, left_out) = std::mem::take(&mut self.columns)
            .into_iter()
            .partition::<Vec<_>, _>(|column| !hide(&column.definition.id));
        self.columns = kept;
        self.left_out
            .extend(left_out.into_iter().map(|column| column.definition.id));
        for column in &mut self.columns {
            if let Some(width) = resize(&column.definition.id) {
                column.width = width;
                self.resized_by_compaction
                    .push(column.definition.id.clone());
            }
        }
    }

    /// `render_cell` and `on_row` get each cell's column id.
    pub(super) fn show<'r, T>(
        &self,
        ui: &mut egui::Ui,
        table: Table,
        rows: &'r [T],
        controls: ColumnControls<'_>,
        mut render_cell: impl FnMut(&mut egui::Ui, &'r T, &str),
        mut on_row: impl FnMut(&egui::Response, &'r T, &str),
    ) {
        let ColumnControls {
            preferences,
            metadata_key_suggestions,
            column_settings,
        } = controls;
        let preferences = RefCell::new(preferences);
        let sort_clicked = table.show_configurable_with_row_response(
            ui,
            rows,
            self.sort.as_ref(),
            |header, id, _label, sortable| {
                self.header_menu(
                    header,
                    id,
                    sortable,
                    &preferences,
                    metadata_key_suggestions,
                    column_settings,
                );
            },
            |id, width| self.resized(&preferences, id, width),
            |ui, row, column| render_cell(ui, row, self.column_id(column)),
            |response, row, column| on_row(response, row, self.column_id(column)),
        );
        self.sort_clicked(&mut preferences.borrow_mut(), sort_clicked);
    }

    /// [`Self::show`] with a checkbox per row keyed by `key_fn`, over `groups` of rows, each under its group
    /// header: the table's [`GroupedTableResponse`] says which group
    /// header was clicked and where the problem rows sit. `problem` is the
    /// tone of a row that is doing badly. While the table is not sorted,
    /// `default_sort` is the order its rows come in, marked on its header.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn show_grouped<'r, T, K>(
        &self,
        ui: &mut egui::Ui,
        table: Table,
        groups: Vec<TableGroup<'_, 'r, T>>,
        selection: &mut HashSet<K>,
        key_fn: impl for<'item> Fn(&'item T) -> Option<&'item K>,
        problem: impl Fn(&T) -> Option<Tone>,
        default_sort: Option<SortState>,
        controls: ColumnControls<'_>,
        mut render_cell: impl FnMut(&mut egui::Ui, &'r T, &str),
        mut on_row: impl FnMut(&egui::Response, &'r T, &str),
    ) -> GroupedTableResponse
    where
        K: Eq + Hash + Clone,
    {
        let ColumnControls {
            preferences,
            metadata_key_suggestions,
            column_settings,
        } = controls;
        let preferences = RefCell::new(preferences);
        let response = table.show_grouped_configurable(
            ui,
            groups,
            Some(selection),
            |item: &T| key_fn(item).cloned(),
            self.sort.as_ref().or(default_sort.as_ref()),
            problem,
            |header, id, _label, sortable| {
                self.header_menu(
                    header,
                    id,
                    sortable,
                    &preferences,
                    metadata_key_suggestions,
                    column_settings,
                );
            },
            |id, width| self.resized(&preferences, id, width),
            |ui, row, column| render_cell(ui, row, self.column_id(column)),
            |response, row, column| on_row(response, row, self.column_id(column)),
        );
        self.sort_clicked(&mut preferences.borrow_mut(), response.sort_clicked.clone());
        response
    }

    /// The header's context menu: sort by the column, show or hide
    /// columns, or open the column settings.
    fn header_menu(
        &self,
        header: &egui::Response,
        column_id: &str,
        sortable: bool,
        preferences: &SharedPreferences<'_>,
        metadata_key_suggestions: Option<&MetadataKeySuggestions>,
        column_settings: &mut Option<ResourceTableSettingsTarget>,
    ) {
        MoreButton::show_context_menu(header, |menu| {
            if sortable {
                for (label, icon, direction) in [
                    ("Sort ascending", Icon::ArrowUp, SortDirection::Ascending),
                    (
                        "Sort descending",
                        Icon::ArrowDown,
                        SortDirection::Descending,
                    ),
                ] {
                    if menu.item(MenuItem::new(label).icon(icon)).clicked() {
                        preferences.borrow_mut().set_sort(
                            &self.key,
                            &self.definitions,
                            column_id,
                            direction,
                        );
                    }
                }
                menu.separator();
            }
            self.column_visibility(menu, &mut preferences.borrow_mut());
            menu.separator();
            let configure =
                menu.item(MenuItem::new("Configure columns…").icon(Icon::AdjustmentsHorizontal));
            if configure.clicked() {
                let mut preferences = preferences.borrow_mut();
                *column_settings = Some(match metadata_key_suggestions {
                    Some(suggestions) => target_with_metadata_key_suggestions(
                        &mut preferences,
                        self.key.clone(),
                        &self.definitions,
                        suggestions.clone(),
                    ),
                    None => target(&mut preferences, self.key.clone(), &self.definitions),
                });
            }
        });
    }

    /// A checkbox item per column and "Reset to default" (C08's column
    /// visibility menu).
    fn column_visibility(
        &self,
        menu: &mut Menu<'_>,
        preferences: &mut PersistedResourceTablePreferences,
    ) {
        let columns = preferences.all_columns(&self.key, &self.definitions);
        let mut hidden = columns
            .iter()
            .filter(|(_, visible)| !visible)
            .map(|(definition, _)| definition.id.clone())
            .collect::<HashSet<_>>();
        let items = columns
            .iter()
            .filter(|(definition, _)| !self.left_out.contains(&definition.id))
            .map(|(definition, _)| (definition.id.as_str(), definition.label.as_str()))
            .collect::<Vec<_>>();
        let response = column_visibility_menu(menu, &items, &mut hidden);
        if response.reset {
            preferences.reset_column_visibility(&self.key, &self.definitions);
        } else if response.changed {
            for (definition, _) in &columns {
                preferences.set_visible(
                    &self.key,
                    &self.definitions,
                    &definition.id,
                    !hidden.contains(&definition.id),
                );
            }
        }
    }

    /// Persist a column's new width.
    fn resized(&self, preferences: &SharedPreferences<'_>, column_id: &str, width: f32) {
        // A width the window imposed is not the user's choice.
        if self.resized_by_compaction.iter().any(|id| id == column_id) {
            return;
        }
        preferences
            .borrow_mut()
            .set_width(&self.key, &self.definitions, column_id, width);
    }

    /// Sort by a header the table reports as clicked.
    fn sort_clicked(
        &self,
        preferences: &mut PersistedResourceTablePreferences,
        clicked: Option<String>,
    ) {
        if let Some(column_id) = clicked {
            preferences.sort_after_header_click(&self.key, &self.definitions, &column_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_resource::ApiResource;

    fn definitions() -> Vec<TableColumnDefinition> {
        vec![
            TableColumnDefinition::sortable("name", "Name", 200.0),
            TableColumnDefinition::sortable("ready", "Ready", 72.0).numeric(),
            TableColumnDefinition::sortable("node", "Node", 156.0),
        ]
    }

    #[test]
    fn a_width_the_window_imposed_is_not_persisted_and_left_out_columns_are_not_offered() {
        let pods = ApiResource {
            group: "core".into(),
            version: "v1".into(),
            kind: "Pod".into(),
            name: "pods".into(),
            namespaced: true,
        };
        let key = ResourceTableKey::workspace(&pods);
        let definitions = definitions();
        let mut preferences = PersistedResourceTablePreferences::default();
        let mut table = PreferenceTable::new(&mut preferences, key.clone(), definitions.clone());
        table.compact(|id| id == "node", |id| (id == "ready").then_some(64.0));
        let ids = |table: &PreferenceTable| {
            table
                .columns
                .iter()
                .map(|column| (column.definition.id.clone(), column.width))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(&table),
            [("name".to_owned(), 200.0), ("ready".to_owned(), 64.0)]
        );

        {
            let shared = RefCell::new(&mut preferences);
            table.resized(&shared, "ready", 90.0);
            table.resized(&shared, "name", 240.0);
        }
        let saved = preferences.resolved_columns(&key, &definitions);
        let width = |id: &str| {
            saved
                .iter()
                .find(|column| column.definition.id == id)
                .map(|column| column.width)
        };
        assert_eq!(width("ready"), Some(72.0), "the imposed width stays out");
        assert_eq!(width("name"), Some(240.0), "other widths persist");
        assert!(table.left_out.contains(&"node".to_owned()));
    }
}
