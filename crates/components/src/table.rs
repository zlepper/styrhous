//! Tables (C08): a virtualised, sortable, resizable and selectable list of
//! rows, used for every resource list, inspector sub-table and settings list.
//!
//! The header is 36 points tall, without a fill, in condensed caps. Rows are
//! 36 point rounded tiles on a 40 point pitch, separated by a gap instead of
//! borders; cells have 12 points of horizontal padding. Numbers are
//! right-aligned ([`TableColumnBuilder::numeric`], in tabular figures with
//! [`TableColumnBuilder::tabular`]) and a
//! missing value is an em dash with an explaining tooltip
//! ([`TableRowBuilder::missing`]).
//!
//! ```ignore
//! use components::Table;
//!
//! Table::new("pods")
//!     .column("name", "Name", |col| col.sortable().fill_remaining())
//!     .column("restarts", "Restarts", |col| col.numeric().initial_width(88.0))
//!     .show(ui, &pods, |ui, pod, column| match column {
//!         0 => TableRowBuilder::text(ui, &pod.name, true),
//!         _ => TableRowBuilder::number(ui, &pod.restarts.to_string()),
//!     });
//! ```

use egui::{Id, Ui};
use std::collections::HashSet;
use std::hash::Hash;

pub(crate) mod body;
mod bulk_bar;
mod cells;
mod column_toggle;
// C17
mod grouped;
pub(crate) mod layout;
mod variants;

pub use bulk_bar::{BulkSelectionBar, BulkSelectionResponse};
pub use cells::TableRowBuilder;
pub use column_toggle::{ColumnVisibilityResponse, column_visibility_menu};
// C17
pub use grouped::{GroupedTableResponse, TableGroup};

#[cfg(test)]
mod tests;

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

    /// The state after the header of `column_id` was clicked: ascending,
    /// then descending, then back to the default order (S08).
    pub fn after_click(state: Option<&Self>, column_id: &str) -> Option<Self> {
        match state {
            Some(current) if current.column_id == column_id => match current.direction {
                SortDirection::Ascending => Some(Self::new(column_id, SortDirection::Descending)),
                SortDirection::Descending => None,
            },
            _ => Some(Self::new(column_id, SortDirection::Ascending)),
        }
    }
}

/// Where a column's cells (and header) sit horizontally.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellAlign {
    /// Against the leading edge, after the cell padding.
    #[default]
    Start,
    /// Against the trailing edge, before the cell padding (numbers).
    End,
    /// Centred, without padding (checkboxes, the actions button).
    Center,
}

/// The look a header cell is forced into (design boards and snapshots).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderState {
    /// The pointer is over the header.
    Hovered,
    /// The column is being resized.
    Resizing,
}

/// Column definition for the table
pub struct TableColumn {
    pub(crate) id: String,
    pub(crate) header: String,
    pub(crate) initial_width: f32,
    pub(crate) fill_remaining: bool,
    pub(crate) sortable: bool,
    pub(crate) hideable: bool,
    pub(crate) align: CellAlign,
    pub(crate) skeleton_width: Option<f32>,
    pub(crate) tabular: bool,
    /// The cells hold small fields that inset their own text.
    pub(crate) holds_fields: bool,
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
            align: CellAlign::Start,
            skeleton_width: None,
            tabular: false,
            holds_fields: false,
        }
    }

    /// The horizontal padding of the column's start- or end-aligned cells.
    pub(crate) fn cell_inset(&self) -> f32 {
        if self.holds_fields {
            0.0
        } else {
            layout::CELL_PADDING_X
        }
    }

    /// What the column is called to assistive technology: its header, or
    /// its id when the header is empty ("actions").
    pub(crate) fn name(&self) -> &str {
        if self.header.is_empty() {
            &self.id
        } else {
            &self.header
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

    /// Align the column's header and cells (see [`CellAlign`]).
    pub fn align(mut self, align: CellAlign) -> Self {
        self.column.align = align;
        self
    }

    /// A column of numbers: right-aligned, for [`TableRowBuilder::number`]
    /// cells.
    pub fn numeric(self) -> Self {
        self.align(CellAlign::End)
    }

    /// Set this column's [`TableRowBuilder::number`] cells in tabular
    /// figures, where the design applies `.tnum` (columns of readouts that
    /// must line up digit for digit). Numbers use proportional figures
    /// otherwise, as the tables of the design do.
    pub fn tabular(mut self) -> Self {
        self.column.tabular = true;
        self
    }

    /// Cells that hold small fields (inputs, selects) which inset their own
    /// text by the cell padding: the cells add none, so the fields' text
    /// lines up with the header's.
    pub fn fields(mut self) -> Self {
        self.column.holds_fields = true;
        self
    }

    /// The width of this column's skeleton bar while the table is loading
    /// ([`Table::show_loading`]).
    pub fn skeleton_width(mut self, width: f32) -> Self {
        self.column.skeleton_width = Some(width);
        self
    }
}

/// The column `id` headed `header`, configured by `configure`.
pub(crate) fn build_column(
    id: impl Into<String>,
    header: impl Into<String>,
    configure: impl FnOnce(TableColumnBuilder) -> TableColumnBuilder,
) -> TableColumn {
    configure(TableColumnBuilder::new(id, header)).column
}

/// Builder for creating a table
pub struct Table {
    pub(crate) id: Id,
    pub(super) columns: Vec<TableColumn>,
    pub(super) is_selectable: bool,
    pub(super) fill_available_height: bool,
    pub(super) inspected_row: Option<usize>,
    pub(super) forced_row_hover: Option<usize>,
    pub(super) forced_header: Option<(String, HeaderState)>,
}

impl Table {
    /// Create a new table with the given id source
    pub fn new(id_source: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            id: Id::new(id_source),
            columns: Vec::new(),
            is_selectable: false,
            fill_available_height: false,
            inspected_row: None,
            forced_row_hover: None,
            forced_header: None,
        }
    }

    /// Add a column to the table
    ///
    /// The closure receives a [`TableColumnBuilder`] to configure the column.
    pub fn column(
        mut self,
        id: impl Into<String>,
        header: impl Into<String>,
        configure: impl FnOnce(TableColumnBuilder) -> TableColumnBuilder,
    ) -> Self {
        self.columns.push(build_column(id, header, configure));
        self
    }

    /// Enable row selection with checkboxes
    pub fn selectable(mut self) -> Self {
        self.is_selectable = true;
        self
    }

    /// Expand the table's scroll surface to the remaining available height.
    ///
    /// This is useful for primary workspace tables, where a short result set should
    /// still read as a deliberate working surface instead of a floating list.
    pub fn fill_available_height(mut self) -> Self {
        self.fill_available_height = true;
        self
    }

    /// Mark the row at `index` as inspected (its blade is open):
    /// `item-selected` with an accent ring.
    pub fn inspected_row(mut self, index: Option<usize>) -> Self {
        self.inspected_row = index;
        self
    }

    /// Paint the row at `index` hovered regardless of the pointer (boards
    /// and snapshots).
    pub fn force_row_hovered(mut self, index: usize) -> Self {
        self.forced_row_hover = Some(index);
        self
    }

    /// Paint the header of `column_id` in `state` regardless of the pointer
    /// (boards and snapshots).
    pub fn force_header_state(mut self, column_id: impl Into<String>, state: HeaderState) -> Self {
        self.forced_header = Some((column_id.into(), state));
        self
    }

    /// Show `rows` loading rows: every cell a skeleton bar
    /// ([`TableColumnBuilder::skeleton_width`]), as on a first load. The rows
    /// are read as `label` ("Loading pods").
    pub fn show_loading(self, ui: &mut Ui, rows: usize, label: &str) {
        body::show_loading(self, ui, rows, label);
    }
}

/// Add or remove every key in `keys` from `selection`: remove them when
/// they are all selected, otherwise add the missing ones.
pub(super) fn toggle_all<'a, K>(selection: &mut HashSet<K>, keys: impl IntoIterator<Item = &'a K>)
where
    K: Eq + Hash + Clone + 'a,
{
    let keys: Vec<&K> = keys.into_iter().collect();
    if keys.iter().all(|key| selection.contains(*key)) {
        for key in keys {
            selection.remove(key);
        }
    } else {
        selection.extend(keys.into_iter().cloned());
    }
}
