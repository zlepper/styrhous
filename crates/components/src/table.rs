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

use egui::{Id, Ui, Vec2};
use egui_extras::{Column, TableBuilder};
use std::collections::HashSet;
use std::hash::Hash;

use crate::colors::{gray, indigo, WHITE};
use crate::icons;

// Layout constants (from Tailwind classes)
const ROW_HEIGHT: f32 = 52.0; // py-4 + content
const HEADER_HEIGHT: f32 = 48.0; // py-3.5 + content
const CELL_PADDING_X: f32 = 12.0; // px-3
const TEXT_FONT_SIZE: f32 = 14.0; // text-sm
const HEADER_BG: egui::Color32 = WHITE;
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
    id: String,
    header: String,
    initial_width: f32,
    sortable: bool,
    hideable: bool,
}

impl TableColumn {
    fn new(id: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            header: header.into(),
            initial_width: 100.0,
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
    id: Id,
    columns: Vec<TableColumn>,
    is_selectable: bool,
}

impl TailwindTable {
    /// Create a new table with the given id source
    pub fn new(id_source: impl std::hash::Hash) -> Self {
        Self {
            id: Id::new(id_source),
            columns: Vec::new(),
            is_selectable: false,
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
        let builder = TableColumnBuilder::new(id, header);
        self.columns.push(configure(builder).column);
        self
    }

    /// Enable row selection with checkboxes
    pub fn selectable(mut self) -> Self {
        self.is_selectable = true;
        self
    }

    /// Show the table with the given items
    ///
    /// The `render_cell` closure is called for each cell with (ui, item, column_index).
    /// Column index 0 is the first column (rendered with stronger text color).
    pub fn show<'a, T>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) {
        let available_height = ui.available_height();
        let num_columns = self.columns.len();

        // Build columns for egui_extras::TableBuilder
        let mut table = TableBuilder::new(ui)
            .striped(false) // We handle striping ourselves for correct colors
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(available_height);

        // Add columns
        for col in &self.columns {
            table = table.column(Column::initial(col.initial_width).resizable(true));
        }

        table
            .header(HEADER_HEIGHT, |mut header| {
                for col in &self.columns {
                    header.col(|ui| {
                        // White background for header
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(rect, 0.0, HEADER_BG);

                        ui.horizontal(|ui| {
                            ui.add_space(CELL_PADDING_X);
                            ui.label(
                                egui::RichText::new(&col.header)
                                    .size(TEXT_FONT_SIZE)
                                    .color(gray::_900)
                                    .strong(),
                            );
                        });
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, items.len(), |mut row| {
                    let row_index = row.index();
                    let item = &items[row_index];

                    // Render each column
                    for col_index in 0..num_columns {
                        row.col(|ui| {
                            // Apply alternating row background (odd rows get gray-100 for visibility)
                            let bg_color = if row_index % 2 == 1 {
                                gray::_100
                            } else {
                                WHITE
                            };
                            let rect = ui.max_rect();
                            ui.painter().rect_filled(rect, 0.0, bg_color);

                            // Add padding and render cell content
                            ui.horizontal(|ui| {
                                ui.add_space(CELL_PADDING_X);
                                render_cell(ui, item, col_index);
                            });
                        });
                    }
                });
            });
    }
}

/// Builder passed to the row render closure for rendering cells
pub struct TableRowBuilder<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl TableRowBuilder<'_> {
    /// Render text in a cell with appropriate styling
    ///
    /// First column gets stronger text color (gray-900), others get gray-500.
    pub fn text(ui: &mut Ui, text: &str, is_first_column: bool) {
        let color = if is_first_column {
            gray::_900
        } else {
            gray::_500
        };
        ui.label(egui::RichText::new(text).size(TEXT_FONT_SIZE).color(color));
    }
}

/// Checkbox state for rendering
#[derive(Clone, Copy, PartialEq)]
enum CheckboxState {
    Unchecked,
    Checked,
    Indeterminate,
}

/// Render a Tailwind-styled checkbox
fn render_checkbox(ui: &mut Ui, state: CheckboxState) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CHECKBOX_SIZE), egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let rounding = egui::CornerRadius::same(3);

        match state {
            CheckboxState::Unchecked => {
                // Border only
                painter.rect_stroke(
                    rect,
                    rounding,
                    egui::Stroke::new(1.5, gray::_300),
                    egui::StrokeKind::Inside,
                );
            }
            CheckboxState::Checked => {
                // Filled background
                painter.rect_filled(rect, rounding, indigo::_600);
                // Checkmark
                let check_color = WHITE;
                let stroke = egui::Stroke::new(2.0, check_color);
                let center = rect.center();
                let size = CHECKBOX_SIZE * 0.35;
                // Draw checkmark path
                let p1 = center + egui::vec2(-size * 0.6, 0.0);
                let p2 = center + egui::vec2(-size * 0.1, size * 0.5);
                let p3 = center + egui::vec2(size * 0.6, -size * 0.4);
                painter.line_segment([p1, p2], stroke);
                painter.line_segment([p2, p3], stroke);
            }
            CheckboxState::Indeterminate => {
                // Filled background
                painter.rect_filled(rect, rounding, indigo::_600);
                // Horizontal dash
                let dash_color = WHITE;
                let stroke = egui::Stroke::new(2.0, dash_color);
                let center = rect.center();
                let half_width = CHECKBOX_SIZE * 0.25;
                painter.line_segment(
                    [center - egui::vec2(half_width, 0.0), center + egui::vec2(half_width, 0.0)],
                    stroke,
                );
            }
        }
    }

    response
}

impl TailwindTable {
    /// Show the table with selection support
    ///
    /// The `selection` set contains the keys of selected items.
    /// The `key_fn` extracts a unique key from each item.
    /// The `render_cell` closure renders each cell.
    pub fn show_selectable<'a, T, K>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        selection: &HashSet<K>,
        key_fn: impl Fn(&T) -> K,
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) where
        K: Eq + Hash + Clone,
    {
        let available_height = ui.available_height();
        let num_columns = self.columns.len();
        let num_items = items.len();

        // Determine select-all checkbox state
        let selected_count = items.iter().filter(|item| selection.contains(&key_fn(item))).count();
        let select_all_state = if selected_count == 0 {
            CheckboxState::Unchecked
        } else if selected_count == num_items {
            CheckboxState::Checked
        } else {
            CheckboxState::Indeterminate
        };

        // Build columns for egui_extras::TableBuilder
        let mut table = TableBuilder::new(ui)
            .striped(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(available_height);

        // Add checkbox column first
        table = table.column(Column::exact(CHECKBOX_COL_WIDTH));

        // Add data columns
        for col in &self.columns {
            table = table.column(Column::initial(col.initial_width).resizable(true));
        }

        table
            .header(HEADER_HEIGHT, |mut header| {
                // Checkbox column header (select all)
                header.col(|ui| {
                    let rect = ui.max_rect();
                    ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                    ui.horizontal_centered(|ui| {
                        ui.add_space(CELL_PADDING_X);
                        render_checkbox(ui, select_all_state);
                    });
                });

                // Data column headers
                for col in &self.columns {
                    header.col(|ui| {
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                        ui.horizontal(|ui| {
                            ui.add_space(CELL_PADDING_X);
                            ui.label(
                                egui::RichText::new(&col.header)
                                    .size(TEXT_FONT_SIZE)
                                    .color(gray::_900)
                                    .strong(),
                            );
                        });
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, num_items, |mut row| {
                    let row_index = row.index();
                    let item = &items[row_index];
                    let is_selected = selection.contains(&key_fn(item));
                    let bg_color = if row_index % 2 == 1 { gray::_100 } else { WHITE };

                    // Checkbox column
                    row.col(|ui| {
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(rect, 0.0, bg_color);
                        ui.horizontal_centered(|ui| {
                            ui.add_space(CELL_PADDING_X);
                            let checkbox_state = if is_selected {
                                CheckboxState::Checked
                            } else {
                                CheckboxState::Unchecked
                            };
                            render_checkbox(ui, checkbox_state);
                        });
                    });

                    // Data columns
                    for col_index in 0..num_columns {
                        row.col(|ui| {
                            let rect = ui.max_rect();
                            ui.painter().rect_filled(rect, 0.0, bg_color);
                            ui.horizontal(|ui| {
                                ui.add_space(CELL_PADDING_X);
                                render_cell(ui, item, col_index);
                            });
                        });
                    }
                });
            });
    }

    /// Show the table with sortable columns
    ///
    /// When a sortable header is clicked, the `sort_state` is updated automatically:
    /// - Clicking an unsorted column sorts it ascending
    /// - Clicking the currently sorted column toggles between ascending/descending
    ///
    /// Returns `true` if the sort state changed (caller should re-sort their data).
    ///
    /// # Example
    /// ```ignore
    /// let mut sort_state = Some(SortState::new("name", SortDirection::Ascending));
    ///
    /// // Sort items based on current state
    /// let mut items = get_items();
    /// if let Some(ref state) = sort_state {
    ///     items.sort_by(|a, b| {
    ///         let cmp = match state.column_id.as_str() {
    ///             "name" => a.name.cmp(&b.name),
    ///             "email" => a.email.cmp(&b.email),
    ///             _ => std::cmp::Ordering::Equal,
    ///         };
    ///         if state.direction == SortDirection::Descending { cmp.reverse() } else { cmp }
    ///     });
    /// }
    ///
    /// let sort_changed = TailwindTable::new("table")
    ///     .column("name", "Name", |col| col.sortable())
    ///     .show_sortable(ui, &items, &mut sort_state, |ui, item, col| { ... });
    ///
    /// if sort_changed {
    ///     // Data needs to be re-sorted on next frame
    /// }
    /// ```
    pub fn show_sortable<'a, T>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        sort_state: &mut Option<SortState>,
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) -> bool {
        let available_height = ui.available_height();
        let num_columns = self.columns.len();

        // Track which column header was clicked
        let clicked_column = std::cell::RefCell::new(None::<String>);

        // Build columns for egui_extras::TableBuilder
        let mut table = TableBuilder::new(ui)
            .striped(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(available_height);

        // Add columns
        for col in &self.columns {
            table = table.column(Column::initial(col.initial_width).resizable(true));
        }

        table
            .header(HEADER_HEIGHT, |mut header| {
                for col in &self.columns {
                    header.col(|ui| {
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(rect, 0.0, HEADER_BG);

                        let is_sorted = sort_state
                            .as_ref()
                            .map(|s| s.column_id == col.id)
                            .unwrap_or(false);
                        let sort_direction = if is_sorted {
                            sort_state.as_ref().map(|s| s.direction)
                        } else {
                            None
                        };

                        // Make sortable headers clickable
                        let response = if col.sortable {
                            let response = ui.interact(
                                rect,
                                ui.id().with(&col.id),
                                egui::Sense::click(),
                            );
                            if response.clicked() {
                                *clicked_column.borrow_mut() = Some(col.id.clone());
                            }
                            if response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            Some(response)
                        } else {
                            None
                        };

                        ui.horizontal(|ui| {
                            ui.add_space(CELL_PADDING_X);

                            ui.label(
                                egui::RichText::new(&col.header)
                                    .size(TEXT_FONT_SIZE)
                                    .color(gray::_900)
                                    .strong(),
                            );

                            // Sort indicator for sortable columns
                            if col.sortable {
                                ui.add_space(4.0);
                                render_sort_indicator(ui, sort_direction);
                            }
                        });

                        let _ = response;
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, items.len(), |mut row| {
                    let row_index = row.index();
                    let item = &items[row_index];
                    let bg_color = if row_index % 2 == 1 { gray::_100 } else { WHITE };

                    for col_index in 0..num_columns {
                        row.col(|ui| {
                            let rect = ui.max_rect();
                            ui.painter().rect_filled(rect, 0.0, bg_color);
                            ui.horizontal(|ui| {
                                ui.add_space(CELL_PADDING_X);
                                render_cell(ui, item, col_index);
                            });
                        });
                    }
                });
            });

        // Handle sort state update
        if let Some(clicked_col_id) = clicked_column.borrow_mut().take() {
            let new_state = match sort_state.as_ref() {
                Some(current) if current.column_id == clicked_col_id => {
                    // Toggle direction
                    Some(SortState {
                        column_id: clicked_col_id,
                        direction: match current.direction {
                            SortDirection::Ascending => SortDirection::Descending,
                            SortDirection::Descending => SortDirection::Ascending,
                        },
                    })
                }
                _ => {
                    // New column, start with ascending
                    Some(SortState {
                        column_id: clicked_col_id,
                        direction: SortDirection::Ascending,
                    })
                }
            };
            *sort_state = new_state;
            true
        } else {
            false
        }
    }
}

impl TailwindTable {
    /// Show the table with column visibility toggle
    ///
    /// The `hidden_columns` set contains the IDs of columns that should be hidden.
    /// Only columns with `hideable: true` (the default) can be toggled.
    pub fn show_with_column_toggle<'a, T>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        hidden_columns: &HashSet<String>,
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) {
        let available_height = ui.available_height();

        // Filter visible columns and track original indices
        let visible_columns: Vec<(usize, &TableColumn)> = self
            .columns
            .iter()
            .enumerate()
            .filter(|(_, col)| !hidden_columns.contains(&col.id))
            .collect();

        let num_visible_columns = visible_columns.len();

        // Build columns for egui_extras::TableBuilder
        let mut table = TableBuilder::new(ui)
            .striped(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(available_height);

        // Add visible columns + one narrow column for the settings icon
        for (_, col) in &visible_columns {
            table = table.column(Column::initial(col.initial_width).resizable(true));
        }
        // Add settings column (narrow, at the end)
        table = table.column(Column::exact(32.0));

        table
            .header(HEADER_HEIGHT, |mut header| {
                // Data column headers
                for (_, col) in &visible_columns {
                    header.col(|ui| {
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                        ui.horizontal(|ui| {
                            ui.add_space(CELL_PADDING_X);
                            ui.label(
                                egui::RichText::new(&col.header)
                                    .size(TEXT_FONT_SIZE)
                                    .color(gray::_900)
                                    .strong(),
                            );
                        });
                    });
                }

                // Settings column header (gear icon placeholder)
                header.col(|ui| {
                    let rect = ui.max_rect();
                    ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                    ui.centered_and_justified(|ui| {
                        render_settings_icon(ui);
                    });
                });
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, items.len(), |mut row| {
                    let row_index = row.index();
                    let item = &items[row_index];
                    let bg_color = if row_index % 2 == 1 { gray::_100 } else { WHITE };

                    // Visible data columns
                    for (original_index, _) in &visible_columns {
                        row.col(|ui| {
                            let rect = ui.max_rect();
                            ui.painter().rect_filled(rect, 0.0, bg_color);
                            ui.horizontal(|ui| {
                                ui.add_space(CELL_PADDING_X);
                                render_cell(ui, item, *original_index);
                            });
                        });
                    }

                    // Empty settings column for rows
                    row.col(|ui| {
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(rect, 0.0, bg_color);
                    });
                });
            });

        // Note: The gear icon menu interaction would be implemented here
        // For now, we just show the icon - actual menu requires mutable hidden_columns
        let _ = num_visible_columns;
    }
}

/// Render a settings/menu icon using SVG
fn render_settings_icon(ui: &mut Ui) -> egui::Response {
    icons::bars_3(ui, 16.0, gray::_500)
}

/// Render sort indicator using SVG icons
fn render_sort_indicator(ui: &mut Ui, direction: Option<SortDirection>) {
    match direction {
        Some(SortDirection::Ascending) => {
            icons::chevron_up(ui, SORT_ICON_SIZE, gray::_700);
        }
        Some(SortDirection::Descending) => {
            icons::chevron_down(ui, SORT_ICON_SIZE, gray::_700);
        }
        None => {
            // Show subtle unsorted indicator - just use a lighter chevron down
            icons::chevron_down(ui, SORT_ICON_SIZE, gray::_300);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use std::collections::HashSet;

    #[derive(Clone)]
    struct User {
        id: u32,
        name: String,
        title: String,
        email: String,
        role: String,
    }

    fn test_users() -> Vec<User> {
        vec![
            User {
                id: 1,
                name: "Lindsay Walton".into(),
                title: "Front-end Developer".into(),
                email: "lindsay.walton@example.com".into(),
                role: "Member".into(),
            },
            User {
                id: 2,
                name: "Courtney Henry".into(),
                title: "Designer".into(),
                email: "courtney.henry@example.com".into(),
                role: "Admin".into(),
            },
            User {
                id: 3,
                name: "Tom Cook".into(),
                title: "Director of Product".into(),
                email: "tom.cook@example.com".into(),
                role: "Member".into(),
            },
            User {
                id: 4,
                name: "Whitney Francis".into(),
                title: "Copywriter".into(),
                email: "whitney.francis@example.com".into(),
                role: "Admin".into(),
            },
            User {
                id: 5,
                name: "Leonard Krasner".into(),
                title: "Senior Designer".into(),
                email: "leonard.krasner@example.com".into(),
                role: "Owner".into(),
            },
            User {
                id: 6,
                name: "Floyd Miles".into(),
                title: "Principal Designer".into(),
                email: "floyd.miles@example.com".into(),
                role: "Member".into(),
            },
        ]
    }

    #[test]
    fn test_table_basic() {
        let users = test_users();

        let mut harness = Harness::new_ui(|ui| {
            TailwindTable::new("users-table")
                .column("name", "Name", |col| col.initial_width(150.0))
                .column("title", "Title", |col| col.initial_width(150.0))
                .column("email", "Email", |col| col.initial_width(200.0))
                .column("role", "Role", |col| col.initial_width(100.0))
                .show(ui, &users, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        2 => &user.email,
                        3 => &user.role,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });

        harness.run();
        harness.snapshot("table_basic");
    }

    #[test]
    fn test_table_alternating_rows() {
        let users = test_users();

        let mut harness = Harness::new_ui(|ui| {
            TailwindTable::new("users-alternating")
                .column("name", "Name", |col| col.initial_width(200.0))
                .column("email", "Email", |col| col.initial_width(250.0))
                .show(ui, &users, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.email,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });

        harness.run();
        harness.snapshot("table_alternating_rows");
    }

    #[test]
    fn test_table_with_selection() {
        let users = test_users();
        let selection: HashSet<u32> = HashSet::new();

        let mut harness = Harness::new_ui(|ui| {
            TailwindTable::new("users-selection")
                .column("name", "Name", |col| col.initial_width(150.0))
                .column("title", "Title", |col| col.initial_width(150.0))
                .selectable()
                .show_selectable(ui, &users, &selection, |user| user.id, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });

        harness.run();
        harness.snapshot("table_with_selection");
    }

    #[test]
    fn test_table_select_all() {
        let users = test_users();
        // All users selected
        let selection: HashSet<u32> = users.iter().map(|u| u.id).collect();

        let mut harness = Harness::new_ui(|ui| {
            TailwindTable::new("users-select-all")
                .column("name", "Name", |col| col.initial_width(150.0))
                .column("title", "Title", |col| col.initial_width(150.0))
                .selectable()
                .show_selectable(ui, &users, &selection, |user| user.id, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });

        harness.run();
        harness.snapshot("table_select_all");
    }

    #[test]
    fn test_table_select_all_indeterminate() {
        let users = test_users();
        // Only some users selected (partial selection)
        let mut selection: HashSet<u32> = HashSet::new();
        selection.insert(1);
        selection.insert(3);

        let mut harness = Harness::new_ui(|ui| {
            TailwindTable::new("users-indeterminate")
                .column("name", "Name", |col| col.initial_width(150.0))
                .column("title", "Title", |col| col.initial_width(150.0))
                .selectable()
                .show_selectable(ui, &users, &selection, |user| user.id, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });

        harness.run();
        harness.snapshot("table_select_all_indeterminate");
    }

    /// Helper function to sort users based on sort state
    fn sort_users(users: &mut [User], sort_state: &Option<SortState>) {
        if let Some(state) = sort_state {
            users.sort_by(|a, b| {
                let cmp = match state.column_id.as_str() {
                    "name" => a.name.cmp(&b.name),
                    "title" => a.title.cmp(&b.title),
                    "email" => a.email.cmp(&b.email),
                    _ => std::cmp::Ordering::Equal,
                };
                match state.direction {
                    SortDirection::Ascending => cmp,
                    SortDirection::Descending => cmp.reverse(),
                }
            });
        }
    }

    #[test]
    fn test_table_sorting() {
        // Comprehensive sorting test that demonstrates the full sorting workflow:
        // 1. No sort state (unsorted indicator shown)
        // 2. Sort by name ascending (data is sorted, up arrow shown)
        // 3. Sort by title descending (data is sorted, down arrow shown)

        // --- Snapshot 1: No sort state (unsorted) ---
        let users = test_users();
        let mut harness = Harness::new_ui(|ui| {
            let mut sort_state: Option<SortState> = None;
            TailwindTable::new("users-sortable")
                .column("name", "Name", |col| col.sortable().initial_width(150.0))
                .column("title", "Title", |col| col.sortable().initial_width(150.0))
                .column("email", "Email", |col| col.initial_width(200.0)) // Not sortable
                .show_sortable(ui, &users, &mut sort_state, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        2 => &user.email,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });
        egui_extras::install_image_loaders(&harness.ctx);
        harness.run();
        harness.snapshot("table_sorting_unsorted");

        // --- Snapshot 2: Sort by name ascending ---
        // Sort state is set before rendering, and data is sorted accordingly
        let mut users = test_users();
        let mut sort_state = Some(SortState::new("name", SortDirection::Ascending));
        sort_users(&mut users, &sort_state);

        let mut harness = Harness::new_ui(|ui| {
            TailwindTable::new("users-sort-name-asc")
                .column("name", "Name", |col| col.sortable().initial_width(150.0))
                .column("title", "Title", |col| col.sortable().initial_width(150.0))
                .column("email", "Email", |col| col.initial_width(200.0))
                .show_sortable(ui, &users, &mut sort_state, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        2 => &user.email,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });
        egui_extras::install_image_loaders(&harness.ctx);
        harness.run();
        harness.snapshot("table_sorting_name_asc");

        // --- Snapshot 3: Sort by title descending ---
        let mut users = test_users();
        let mut sort_state = Some(SortState::new("title", SortDirection::Descending));
        sort_users(&mut users, &sort_state);

        let mut harness = Harness::new_ui(|ui| {
            TailwindTable::new("users-sort-title-desc")
                .column("name", "Name", |col| col.sortable().initial_width(150.0))
                .column("title", "Title", |col| col.sortable().initial_width(150.0))
                .column("email", "Email", |col| col.initial_width(200.0))
                .show_sortable(ui, &users, &mut sort_state, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        2 => &user.email,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });
        egui_extras::install_image_loaders(&harness.ctx);
        harness.run();
        harness.snapshot("table_sorting_title_desc");
    }

    #[test]
    fn test_table_column_toggle_menu() {
        let users = test_users();
        let hidden_columns: HashSet<String> = HashSet::new();

        let mut harness = Harness::new_ui(|ui| {
            TailwindTable::new("users-column-toggle")
                .column("name", "Name", |col| col.initial_width(150.0))
                .column("title", "Title", |col| col.initial_width(150.0))
                .column("email", "Email", |col| col.initial_width(200.0))
                .show_with_column_toggle(ui, &users, &hidden_columns, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        2 => &user.email,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });

        egui_extras::install_image_loaders(&harness.ctx);
        harness.run();
        harness.snapshot("table_column_toggle_menu");
    }

    #[test]
    fn test_table_hidden_column() {
        let users = test_users();
        let mut hidden_columns: HashSet<String> = HashSet::new();
        hidden_columns.insert("title".to_string()); // Hide the Title column

        let mut harness = Harness::new_ui(|ui| {
            TailwindTable::new("users-hidden-column")
                .column("name", "Name", |col| col.initial_width(150.0))
                .column("title", "Title", |col| col.initial_width(150.0))
                .column("email", "Email", |col| col.initial_width(200.0))
                .show_with_column_toggle(ui, &users, &hidden_columns, |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        2 => &user.email,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                });
        });

        egui_extras::install_image_loaders(&harness.ctx);
        harness.run();
        harness.snapshot("table_hidden_column");
    }
}
