//! The one renderer behind every `Table::show*` variant: a sticky header
//! and a virtualised body of row tiles.

use std::collections::HashSet;
use std::hash::Hash;

use egui::{Id, Rect, Response, Sense, Ui, Vec2, vec2};

use super::layout::{
    CellAlign, ColumnLayout, HeaderCell, RowLook, Separator, paint_header_cell, paint_row_tile,
    show_cell, tile_rect,
};
use super::{HeaderState, SortDirection, SortState, Table, TableColumn, toggle_all};
use crate::PointingHand;
use crate::design::size;
use crate::selection::{CheckState, Checkbox};

/// Width of a column's resize hit area, against its trailing edge.
const RESIZE_HIT_WIDTH: f32 = 8.0;
/// The share of a cell's content width its loading skeleton bar takes when
/// the column does not say.
const DEFAULT_SKELETON_SHARE: f32 = 0.6;

/// Row selection: the caller's selected keys and how to key an item
/// (`None` for a row that cannot be selected).
pub(super) struct Selection<'s, T, K> {
    pub set: &'s mut HashSet<K>,
    pub key: &'s dyn Fn(&T) -> Option<K>,
    /// The group (of a grouped table) whose rows are being shown: a
    /// Shift-click selects a range only from an anchor in the same group.
    pub group: usize,
}

/// Called with a column's id and its new width.
pub(crate) type ResizeCallback<'a> = dyn FnMut(&str, f32) + 'a;
/// Called with a header cell's response, column id, label and whether it is
/// sortable.
pub(crate) type HeaderCallback<'a> = dyn FnMut(&Response, &str, &str, bool) + 'a;

/// What the header does besides labelling its columns.
#[derive(Default)]
pub(crate) struct HeaderOptions<'o> {
    /// The current sort, marked with a chevron.
    pub(crate) sort: Option<&'o SortState>,
    /// Clicking a sortable header asks to sort by it.
    pub click_to_sort: bool,
    /// Columns can be resized; called with a column's id and new width.
    pub resize: Option<&'o mut ResizeCallback<'o>>,
    /// Called with every header cell's response, id, label and whether it
    /// is sortable (header context menus).
    pub on_header: Option<&'o mut HeaderCallback<'o>>,
    /// Scroll horizontally when the columns exceed the available width.
    pub horizontal_scroll: bool,
}

/// Show `table` over `items`. Returns the id of a sortable column whose
/// header was clicked when `click_to_sort` is set.
pub(super) fn show<'a, T, K>(
    table: Table,
    ui: &mut Ui,
    items: &'a [T],
    mut selection: Option<Selection<'_, T, K>>,
    mut header: HeaderOptions<'_>,
    render_cell: &mut impl FnMut(&mut Ui, &'a T, usize),
    render_row: &mut impl FnMut(&Response, &'a T, usize),
) -> Option<String>
where
    K: Eq + Hash + Clone,
{
    let original_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let widths = layout_widths(&table, selection.is_some(), ui.available_width());
    let content_width = widths.iter().sum::<f32>();
    let available_height = ui.available_height();
    let horizontal_scroll = header.horizontal_scroll;
    let mut draw = |ui: &mut Ui| {
        let (layout, width, sort_clicked) = show_header_band(
            &table,
            ui,
            &widths,
            &mut items.iter(),
            selection.as_mut(),
            &mut header,
        );
        let body_height = (available_height - size::TABLE_HEADER).max(0.0);
        let mut scroll = crate::scroll::vertical()
            .id_salt(table.id.with("body"))
            .max_height(body_height)
            .auto_shrink([false, !table.fill_available_height]);
        if table.fill_available_height {
            scroll = scroll.min_scrolled_height(body_height);
        }
        scroll.show_rows(ui, size::ROW_PITCH, items.len(), |ui, range| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for index in range {
                let (row, _) = ui.allocate_exact_size(vec2(width, size::ROW_PITCH), Sense::hover());
                show_row(
                    &table,
                    ui,
                    &layout,
                    usize::from(selection.is_some()),
                    row,
                    index,
                    &items[index],
                    selection.as_mut(),
                    items,
                    index,
                    render_cell,
                    render_row,
                );
            }
        });
        sort_clicked
    };
    let clicked = if horizontal_scroll {
        crate::scroll::horizontal()
            .id_salt(table.id.with("horizontal-scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(content_width);
                draw(ui)
            })
            .inner
    } else {
        draw(ui)
    };
    ui.spacing_mut().item_spacing = original_spacing;
    clicked
}

/// The width of every data column: its own width, and for columns that
/// fill the remaining space an equal share of what the others leave.
/// The width of every column of `table` laid out in `available` points, the
/// checkbox column first when `selectable`.
pub(super) fn layout_widths(table: &Table, selectable: bool, available: f32) -> Vec<f32> {
    let leading = if selectable {
        size::TABLE_CHECKBOX_COLUMN
    } else {
        0.0
    };
    selectable
        .then_some(leading)
        .into_iter()
        .chain(resolve_widths(&table.columns, leading, available))
        .collect()
}

pub(crate) fn resolve_widths(columns: &[TableColumn], leading: f32, available: f32) -> Vec<f32> {
    let fixed: f32 = leading
        + columns
            .iter()
            .filter(|column| !column.fill_remaining)
            .map(|column| column.initial_width)
            .sum::<f32>();
    let fill_count = columns
        .iter()
        .filter(|column| column.fill_remaining)
        .count();
    let share = if fill_count == 0 {
        0.0
    } else {
        (available - fixed) / fill_count as f32
    };
    columns
        .iter()
        .map(|column| {
            if column.fill_remaining {
                share.max(column.initial_width)
            } else {
                column.initial_width
            }
        })
        .collect()
}

/// The header across the table's `widths` (and at least the available
/// width): returns its column layout, the table's width and the sortable
/// column whose header was clicked.
pub(super) fn show_header_band<'i, T: 'i, K>(
    table: &Table,
    ui: &mut Ui,
    widths: &[f32],
    items: &mut dyn Iterator<Item = &'i T>,
    selection: Option<&mut Selection<'_, T, K>>,
    header: &mut HeaderOptions<'_>,
) -> (ColumnLayout, f32, Option<String>)
where
    K: Eq + Hash + Clone,
{
    let width = widths.iter().sum::<f32>().max(ui.available_width());
    let (header_rect, _) = ui.allocate_exact_size(vec2(width, size::TABLE_HEADER), Sense::hover());
    let layout = ColumnLayout::new(header_rect.left(), widths.iter().copied());
    let sort_clicked = show_header(
        table,
        ui,
        &layout,
        header_rect.top(),
        items,
        selection,
        header,
    );
    (layout, width, sort_clicked)
}

pub(super) fn show_header<'i, T: 'i, K>(
    table: &Table,
    ui: &mut Ui,
    layout: &ColumnLayout,
    top: f32,
    items: &mut dyn Iterator<Item = &'i T>,
    selection: Option<&mut Selection<'_, T, K>>,
    header: &mut HeaderOptions<'_>,
) -> Option<String>
where
    K: Eq + Hash + Clone,
{
    let offset = usize::from(selection.is_some());
    if let Some(selection) = selection {
        let rect = layout.cell(0, top, size::TABLE_HEADER);
        if let Some(on_header) = header.on_header.as_mut() {
            let response =
                ui.interact(rect, table.id.with("selection-header-menu"), Sense::click());
            set_label(ui, &response, "Selection column");
            on_header(&response, "selection", "Selection", false);
        }
        let keys: Vec<K> = items.filter_map(|item| (selection.key)(item)).collect();
        let selected = keys
            .iter()
            .filter(|key| selection.set.contains(*key))
            .count();
        let state = CheckState::from_counts(selected, keys.len());
        let response = show_cell(
            ui,
            rect,
            CellAlign::Center,
            0.0,
            false,
            checkbox_width_id(table),
            |ui| {
                Checkbox::with_state(state, "")
                    .accessibility_label("Select all rows")
                    .show(ui)
            },
        );
        if response.clicked() {
            toggle_all(selection.set, &keys);
        }
    }

    show_column_headers(table, ui, layout, top, offset, header)
}

/// The header cells of `table`'s columns, which start at column `offset`
/// of `layout`. Returns the id of a sortable column whose header was
/// clicked when `click_to_sort` is set.
pub(crate) fn show_column_headers(
    table: &Table,
    ui: &mut Ui,
    layout: &ColumnLayout,
    top: f32,
    offset: usize,
    header: &mut HeaderOptions<'_>,
) -> Option<String> {
    let mut sort_clicked = None;
    for (index, column) in table.columns.iter().enumerate() {
        let rect = layout.cell(index + offset, top, size::TABLE_HEADER);
        let forced = table
            .forced_header
            .as_ref()
            .filter(|(id, _)| *id == column.id)
            .map(|(_, state)| *state);
        let resize_rect = Rect::from_min_max(
            egui::pos2(rect.right() - RESIZE_HIT_WIDTH, rect.top()),
            rect.right_bottom(),
        );
        // An unlabelled column (the actions button) is not resized.
        let resizable = header.resize.is_some() && !column.header.is_empty();
        let response = if header.on_header.is_some() {
            let area = if resizable {
                rect.with_max_x(resize_rect.left())
            } else {
                rect
            };
            let response = ui.interact(
                area,
                table.id.with(("header-menu", &column.id)),
                Sense::click(),
            );
            // A sortable header also sorts when clicked.
            let sorts = header.click_to_sort && column.sortable;
            let response = if sorts {
                response.with_pointing_hand()
            } else {
                response
            };
            if sorts && response.clicked() {
                sort_clicked = Some(column.id.clone());
            }
            set_label(ui, &response, format!("{} column", column.name()));
            response
        } else if header.click_to_sort && column.sortable {
            let response = ui
                .interact(rect, table.id.with(("sort", &column.id)), Sense::click())
                .with_pointing_hand();
            let label = match sort_direction(header.sort, &column.id) {
                Some(SortDirection::Ascending) => {
                    format!("Sort by {}; currently ascending", column.header)
                }
                Some(SortDirection::Descending) => {
                    format!("Sort by {}; currently descending", column.header)
                }
                None => format!("Sort by {}", column.header),
            };
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
            });
            if response.clicked() {
                sort_clicked = Some(column.id.clone());
            }
            response
        } else {
            let response = ui.interact(rect, table.id.with(("header", &column.id)), Sense::hover());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), &column.header)
            });
            response
        };
        let resizing = match header.resize.as_mut().filter(|_| resizable) {
            Some(resized) => {
                handle_column_resize(ui, table.id, column, resize_rect, &mut **resized)
            }
            None => false,
        };
        let labelled_next = table
            .columns
            .get(index + 1)
            .is_some_and(|next| !next.header.is_empty());
        let separator = if resizing || forced == Some(HeaderState::Resizing) {
            Separator::Resizing
        } else if labelled_next && !column.header.is_empty() {
            Separator::Rest
        } else {
            Separator::None
        };
        let hovered = forced.is_some()
            || resizing
            || column.sortable && response.hovered() && response.sense.senses_click();
        paint_header_cell(
            ui,
            rect,
            &response,
            &HeaderCell {
                label: &column.header,
                align: column.align,
                sort: sort_direction(header.sort, &column.id),
                hovered,
                separator,
            },
        );
        if let Some(on_header) = header.on_header.as_mut() {
            on_header(&response, &column.id, &column.header, column.sortable);
        }
    }
    sort_clicked
}

fn sort_direction(sort: Option<&SortState>, column_id: &str) -> Option<SortDirection> {
    sort.filter(|state| state.column_id == column_id)
        .map(|state| state.direction)
}

/// The resize handle of `column` over `rect`; returns whether the column is
/// being resized (hovered or dragged).
fn handle_column_resize(
    ui: &Ui,
    table_id: Id,
    column: &TableColumn,
    rect: Rect,
    resized: &mut dyn FnMut(&str, f32),
) -> bool {
    let resize_id = table_id.with(("resize", &column.id));
    let response = ui
        .interact(rect, resize_id, Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("Resize {} column", column.header),
        )
    });
    let drag_start_id = table_id.with(("resize-start", &column.id));
    if response.drag_started() {
        ui.data_mut(|data| data.insert_temp(drag_start_id, column.initial_width));
    }
    if let Some(delta) = response.total_drag_delta()
        && let Some(start_width) = ui.data(|data| data.get_temp::<f32>(drag_start_id))
    {
        let width = (start_width + delta.x).max(size::TABLE_MIN_COLUMN);
        if width != column.initial_width {
            resized(&column.id, width);
            // Lay the table out again at the new width.
            ui.ctx().request_repaint();
        }
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<f32>(drag_start_id));
    }
    response.hovered() || response.dragged()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn show_row<'a, T, K>(
    table: &Table,
    ui: &mut Ui,
    layout: &ColumnLayout,
    offset: usize,
    row: Rect,
    index: usize,
    item: &'a T,
    selection: Option<&mut Selection<'_, T, K>>,
    items: &'a [T],
    item_index: usize,
    render_cell: &mut impl FnMut(&mut Ui, &'a T, usize),
    render_row: &mut impl FnMut(&Response, &'a T, usize),
) where
    K: Eq + Hash + Clone,
{
    let tile = tile_rect(row);
    let key = selection
        .as_ref()
        .and_then(|selection| (selection.key)(item));
    let selected = selection
        .as_ref()
        .zip(key.as_ref())
        .is_some_and(|(selection, key)| selection.set.contains(key));
    let hovered = table.forced_row_hover == Some(index) || ui.rect_contains_pointer(tile);
    let look = match (selected, hovered) {
        _ if table.inspected_row == Some(index) => RowLook::Inspected,
        (true, true) => RowLook::SelectedHovered,
        (true, false) => RowLook::Selected,
        (false, true) => RowLook::Hovered,
        (false, false) => RowLook::Rest,
    };
    paint_row_tile(ui, row, look);

    if let Some(selection) = selection {
        let rect = layout.cell(0, tile.top(), tile.height());
        if key.is_some() {
            let response = show_cell(
                ui,
                rect,
                CellAlign::Center,
                0.0,
                false,
                checkbox_width_id(table),
                |ui| {
                    Checkbox::with_state(CheckState::from(selected), "")
                        .accessibility_label(&format!("Select row {}", index + 1))
                        .show(ui)
                },
            );
            if response.clicked() {
                toggle_row(ui, table.id, selection, items, item_index, !selected);
            }
        }
    }

    for (column_index, column) in table.columns.iter().enumerate() {
        let rect = layout.cell(column_index + offset, tile.top(), tile.height());
        let response = ui.interact(
            rect,
            table.id.with(("row-context-menu", index, column_index)),
            Sense::click(),
        );
        set_label(
            ui,
            &response,
            format!("{}, row {}", column.name(), index + 1),
        );
        show_cell(
            ui,
            rect,
            column.align,
            column.cell_inset(),
            column.tabular,
            cell_width_id(table, column_index),
            |ui| {
                render_cell(ui, item, column_index);
            },
        );
        render_row(&response, item, column_index);
    }
}

/// Select or deselect the row at `index`; with Shift held,
/// every row from the last clicked one to this one (S08).
fn toggle_row<T, K>(
    ui: &Ui,
    table_id: Id,
    selection: &mut Selection<'_, T, K>,
    items: &[T],
    index: usize,
    select: bool,
) where
    K: Eq + Hash + Clone,
{
    let anchor_id = table_id.with("selection-anchor");
    let anchor = ui
        .data(|data| data.get_temp::<(usize, usize)>(anchor_id))
        .filter(|(group, _)| *group == selection.group)
        .map(|(_, anchor)| anchor);
    let range = match anchor {
        Some(anchor) if ui.input(|input| input.modifiers.shift) && anchor < items.len() => {
            anchor.min(index)..=anchor.max(index)
        }
        _ => index..=index,
    };
    for item in &items[range] {
        let Some(row_key) = (selection.key)(item) else {
            continue;
        };
        if select {
            selection.set.insert(row_key);
        } else {
            selection.set.remove(&row_key);
        }
    }
    ui.data_mut(|data| data.insert_temp(anchor_id, (selection.group, index)));
}

/// Where the width of a centred cell of column `index` is remembered.
pub(crate) fn cell_width_id(table: &Table, index: usize) -> Id {
    table.id.with(("cell-width", index))
}

fn checkbox_width_id(table: &Table) -> Id {
    table.id.with("checkbox-width")
}

fn set_label(ui: &Ui, response: &Response, label: impl Into<String>) {
    let label = label.into();
    ui.ctx()
        .accesskit_node_builder(response.id, |builder| builder.set_label(label));
}

/// Show `table`'s header over `rows` loading rows of skeleton bars, read as
/// `label`. A selectable table keeps its (empty) checkbox column so the
/// columns do not move once the rows arrive; an unlabelled column (the row
/// actions) gets no bar.
pub(super) fn show_loading(table: Table, ui: &mut Ui, rows: usize, label: &str) {
    let original_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let offset = usize::from(table.is_selectable);
    let widths = layout_widths(&table, table.is_selectable, ui.available_width());
    let width = widths.iter().sum::<f32>().max(ui.available_width());
    let (header_rect, _) = ui.allocate_exact_size(vec2(width, size::TABLE_HEADER), Sense::hover());
    let layout = ColumnLayout::new(header_rect.left(), widths.iter().copied());
    show_column_headers(
        &table,
        ui,
        &layout,
        header_rect.top(),
        offset,
        &mut HeaderOptions::default(),
    );
    let (body, response) =
        ui.allocate_exact_size(vec2(width, size::ROW_PITCH * rows as f32), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
    for row_index in 0..rows {
        let row = Rect::from_min_size(
            body.left_top() + vec2(0.0, size::ROW_PITCH * row_index as f32),
            vec2(width, size::ROW_PITCH),
        );
        paint_row_tile(ui, row, RowLook::Rest);
        let tile = tile_rect(row);
        for (index, column) in table.columns.iter().enumerate() {
            if column.header.is_empty() {
                continue;
            }
            let rect = layout.cell(index + offset, tile.top(), tile.height());
            let bar = column.skeleton_width.unwrap_or_else(|| {
                (rect.width() - 2.0 * super::layout::CELL_PADDING_X) * DEFAULT_SKELETON_SHARE
            });
            show_cell(
                ui,
                rect,
                column.align,
                column.cell_inset(),
                false,
                cell_width_id(&table, index),
                |ui| {
                    crate::skeleton_bar(ui, bar);
                },
            );
        }
    }
    ui.spacing_mut().item_spacing = original_spacing;
}
