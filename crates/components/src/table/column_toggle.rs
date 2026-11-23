//! Column visibility: hiding columns from a table and the menu that
//! toggles them (C08 "Column visibility").

use std::collections::HashSet;

use egui::Ui;

use super::{Table, TableColumn};
use crate::icons::Icon;
use crate::menu::{Menu, MenuItem};
use crate::selection::CheckState;

impl Table {
    /// Show the table without the columns whose ids are in
    /// `hidden_columns`. Only hideable columns ([`super::TableColumnBuilder::not_hideable`])
    /// are hidden; `render_cell` still receives each column's original index.
    pub fn show_with_column_toggle<'a, T>(
        mut self,
        ui: &mut Ui,
        items: &'a [T],
        hidden_columns: &HashSet<String>,
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) {
        let original: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .filter(|(_, column)| is_visible(column, hidden_columns))
            .map(|(index, _)| index)
            .collect();
        self.columns
            .retain(|column| is_visible(column, hidden_columns));
        self.show(ui, items, |ui, item, column| {
            render_cell(ui, item, original[column]);
        });
    }
}

fn is_visible(column: &TableColumn, hidden_columns: &HashSet<String>) -> bool {
    !column.hideable || !hidden_columns.contains(&column.id)
}

/// What the column visibility menu did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnVisibilityResponse {
    /// A column was shown or hidden.
    pub changed: bool,
    /// "Reset to default" was chosen; the caller restores its default
    /// columns.
    pub reset: bool,
}

/// The column visibility menu (C08): a "Columns" heading, a checkbox item
/// per `(id, label)` column toggling its id in `hidden`, and "Reset to
/// default". Show it from a table's header context menu or a Columns
/// action.
pub fn column_visibility_menu(
    menu: &mut Menu<'_>,
    columns: &[(&str, &str)],
    hidden: &mut HashSet<String>,
) -> ColumnVisibilityResponse {
    let mut response = ColumnVisibilityResponse::default();
    menu.heading("Columns");
    for (id, label) in columns {
        let visible = !hidden.contains(*id);
        if menu.checkbox(label, CheckState::from(visible)).clicked() {
            if visible {
                hidden.insert((*id).to_owned());
            } else {
                hidden.remove(*id);
            }
            response.changed = true;
        }
    }
    menu.separator();
    response.reset = menu
        .item(MenuItem::new("Reset to default").icon(Icon::ArrowUturnLeft))
        .clicked();
    response
}
