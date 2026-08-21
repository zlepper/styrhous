//! Tailwind-styled table component for egui
//!
//! A table component wrapping `egui_extras::TableBuilder` with Tailwind styling,
//! virtual scrolling, multi-selection, sortable columns, and column visibility toggle.
//!
//! # Example
//!
//! ```ignore
//! use components::TailwindTable;
//!
//! TailwindTable::new("users-table")
//!     .column("name", "Name", |col| col.sortable().initial_width(150.0))
//!     .column("email", "Email", |col| col.initial_width(200.0))
//!     .show(ui, &users, |ui, user, col_index| {
//!         match col_index {
//!             0 => { ui.label(&user.name); },
//!             1 => { ui.label(&user.email); },
//!             _ => {},
//!         }
//!     });
//! ```

use egui::{Button, Color32, Id, Ui, Vec2, WidgetText};
use egui_extras::{Column, TableBuilder};
use std::collections::HashSet;
use std::hash::Hash;

use crate::PointingHand;
use crate::colors::{
    CONTENT_BACKGROUND, TABLE_BORDER, TABLE_HEADER_BACKGROUND, WHITE, gray, indigo,
};
use crate::design::{radius, spacing, typography};
use crate::icons;

fn egui_column(column: &TableColumn) -> Column {
    let column = if column.fill_remaining {
        Column::remainder()
            .at_least(column.initial_width)
            .clip(true)
    } else {
        Column::initial(column.initial_width).clip(true)
    };
    // Non-configurable tables keep their default surface quiet. Configurable
    // resource tables provide their own explicit resize gutters in the header.
    column.resizable(false)
}

// The resource table follows the compact desktop rhythm used by controls and
// sidebars, while keeping a comfortable row target for pointer interactions.
const ROW_HEIGHT: f32 = 44.0;
const HEADER_HEIGHT: f32 = 40.0;
const CELL_PADDING_X: f32 = spacing::LG;
const ROOMY_ROW_HEIGHT: f32 = 81.25;
const ROOMY_HEADER_HEIGHT: f32 = 64.0;
const ROOMY_CELL_PADDING_X: f32 = 30.0;
const HEADER_BG: egui::Color32 = TABLE_HEADER_BACKGROUND;
const CHECKBOX_SIZE: f32 = 16.0;
const CHECKBOX_COL_WIDTH: f32 = 48.0;
const SORT_ICON_SIZE: f32 = 12.0;

/// Sort direction for a column
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// Sort state for the table
#[derive(Clone, Debug)]
pub struct SortState {
    pub column_id: String,
    pub direction: SortDirection,
}

impl SortState {
    /// Create a new sort state
    pub fn new(column_id: impl Into<String>, direction: SortDirection) -> Self {
        Self {
            column_id: column_id.into(),
            direction,
        }
    }
}

/// Column definition for the table
pub struct TableColumn {
    pub(super) id: String,
    pub(super) header: String,
    pub(super) initial_width: f32,
    pub(super) fill_remaining: bool,
    pub(super) sortable: bool,
    pub(super) hideable: bool,
}

impl TableColumn {
    fn new(id: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            header: header.into(),
            initial_width: 100.0,
            fill_remaining: false,
            sortable: false,
            hideable: true,
        }
    }
}

/// Builder for configuring a table column
pub struct TableColumnBuilder {
    column: TableColumn,
}

impl TableColumnBuilder {
    fn new(id: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            column: TableColumn::new(id, header),
        }
    }

    /// Set the initial width of the column
    pub fn initial_width(mut self, width: f32) -> Self {
        self.column.initial_width = width;
        self
    }

    /// Make this column consume space remaining after fixed-width columns.
    pub fn fill_remaining(mut self) -> Self {
        self.column.fill_remaining = true;
        self
    }

    /// Make this column sortable
    pub fn sortable(mut self) -> Self {
        self.column.sortable = true;
        self
    }

    /// Prevent user from hiding this column
    pub fn not_hideable(mut self) -> Self {
        self.column.hideable = false;
        self
    }
}

/// Builder for creating a Tailwind-styled table
pub struct TailwindTable {
    #[allow(dead_code)] // Reserved for future state persistence
    pub(super) id: Id,
    pub(super) columns: Vec<TableColumn>,
    pub(super) is_selectable: bool,
    pub(super) fill_available_height: bool,
    pub(super) roomy: bool,
}

mod column_toggle;
mod configurable;
mod rows;
mod selectable;
mod standard;

use column_toggle::render_sort_indicator;
use rows::{
    CheckboxState, handle_column_resize, render_checkbox, row_context_menu_response,
    set_accessibility_label,
};
pub use rows::{TableRowBuilder, tailwind_checkbox};

#[cfg(test)]
mod tests;
