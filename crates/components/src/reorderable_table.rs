//! Reorderable tables (C08 `ReorderableTable`): settings lists whose rows
//! are reordered by dragging their grip.
//!
//! Rows are table tiles behind a 32 point grip column (`bars-3`). While a
//! row is dragged it floats raised with a drop shadow, and a dashed gap
//! shows where it will land.
//!
//! ```ignore
//! let moved = ReorderableTable::new("debug-images")
//!     .column("name", "Name", |col| col.initial_width(120.0))
//!     .column("image", "Image", |col| col.fill_remaining())
//!     .show(ui, &mut presets, |preset| format!("Reorder {}", preset.name), |ui, preset, _, column| match column {
//!         0 => TableRowBuilder::text(ui, &preset.name, false),
//!         _ => TableRowBuilder::mono(ui, &preset.image),
//!     });
//! ```

use egui::{Id, LayerId, Order, Rect, Response, Sense, Ui, Vec2, vec2};

use crate::design::size;
use crate::design::theme::ThemeExt;
use crate::icons::Icon;
use crate::table::body::{HeaderOptions, cell_width_id, resolve_widths, show_column_headers};
use crate::table::layout::{
    ColumnLayout, RowLook, paint_row_tile, paint_tile, show_cell, tile_rect,
};
use crate::table::{Table, TableColumnBuilder};

/// Width of the grip column.
const GRIP_COLUMN: f32 = 32.0;

#[derive(Clone)]
struct DragPayload {
    table_id: Id,
    from: usize,
}

/// A drag handle supplied to a [`ReorderableTable::show_rows`] row renderer.
pub struct ReorderHandle {
    table_id: Id,
    index: usize,
}

impl ReorderHandle {
    /// Mark an existing drag response as this row's reorder handle.
    pub fn register(&self, response: &Response) {
        response.dnd_set_drag_payload(DragPayload {
            table_id: self.table_id,
            from: self.index,
        });
    }

    /// Add the grip (`bars-3`, `text-disabled`, `text-secondary` while
    /// hovered or dragged) centred in `rect`, read as `label` ("Reorder
    /// Busybox"), and register it as the handle.
    pub fn grip(&self, ui: &mut Ui, rect: Rect, label: &str) -> Response {
        let response = ui
            .interact(
                rect,
                self.table_id.with(("grip", self.index)),
                Sense::drag(),
            )
            .on_hover_cursor(egui::CursorIcon::Grab);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        self.register(&response);
        let active = response.hovered() || response.dragged();
        paint_grip(ui, rect, active);
        response
    }

    /// Paint the idle grip of a row that cannot move just now (it is being
    /// edited) centred in `rect`.
    pub fn paint_still(ui: &Ui, rect: Rect) {
        paint_grip(ui, rect, false);
    }
}

fn paint_grip(ui: &Ui, rect: Rect, active: bool) {
    let theme = ui.tokens();
    let color = if active {
        theme.text.secondary
    } else {
        theme.text.disabled
    };
    let icon = Rect::from_center_size(rect.center(), Vec2::splat(size::ICON));
    Icon::Bars3.paint(ui, icon, color);
}

/// A row shown dragged in place (design boards and snapshots).
#[derive(Clone, Copy, Debug)]
struct DragPreview {
    from: usize,
    to: usize,
    offset: Vec2,
}

/// A table whose rows can be reordered by dragging.
pub struct ReorderableTable {
    table: Table,
    preview: Option<DragPreview>,
}

/// Where a drag over `count` rows stands this frame.
struct DragState {
    payload: Option<DragPayload>,
    /// The row being dragged.
    from: Option<usize>,
    /// The row boundary it would be dropped at.
    drop: usize,
}

impl ReorderableTable {
    pub fn new(id_source: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            table: Table::new(id_source),
            preview: None,
        }
    }

    /// Add a column after the grip column (see [`Table::column`]).
    pub fn column(
        mut self,
        id: impl Into<String>,
        header: impl Into<String>,
        configure: impl FnOnce(TableColumnBuilder) -> TableColumnBuilder,
    ) -> Self {
        self.table = self.table.column(id, header, configure);
        self
    }

    /// Show the row at `from` dragged towards the row boundary `to`,
    /// floating `offset` from the gap it leaves (design boards and
    /// snapshots).
    pub fn preview_drag(mut self, from: usize, to: usize, offset: Vec2) -> Self {
        self.preview = Some(DragPreview { from, to, offset });
        self
    }

    fn id(&self) -> Id {
        self.table.id
    }

    fn drag_state(&self, ui: &Ui, count: usize, table_rect: Rect, pitch: f32) -> DragState {
        if let Some(preview) = self.preview.filter(|preview| preview.from < count) {
            return DragState {
                payload: None,
                from: Some(preview.from),
                drop: preview.to.min(count),
            };
        }
        let payload = egui::DragAndDrop::payload::<DragPayload>(ui.ctx())
            .filter(|payload| payload.table_id == self.id() && payload.from < count)
            .map(|payload| (*payload).clone());
        if let Some(payload) = &payload {
            egui::DragAndDrop::set_payload(ui.ctx(), payload.clone());
        }
        let from = payload.as_ref().map(|payload| payload.from);
        let drop = from.map_or(0, |from| {
            let pointer_y = ui
                .ctx()
                .pointer_interact_pos()
                .map_or(table_rect.center().y, |position| position.y);
            drop_index(pointer_y, table_rect, count, from, pitch)
        });
        DragState {
            payload,
            from,
            drop,
        }
    }

    /// Move the dragged row once it is dropped over the table.
    fn finish_drag<T>(ui: &Ui, state: DragState, visible: Rect, items: &mut Vec<T>) -> bool {
        let released = ui.input(|input| input.pointer.any_released())
            && ui
                .ctx()
                .pointer_interact_pos()
                .is_some_and(|position| visible.contains(position));
        match state.payload {
            Some(payload) if released => move_item(items, payload.from, state.drop),
            _ => false,
        }
    }

    /// Where the floating copy of a dragged row goes: under the pointer
    /// (within the table), or `offset` from its gap in a preview.
    fn floating_rect(&self, ui: &Ui, visible: Rect, slot: Rect, pitch: f32) -> Option<Rect> {
        if let Some(preview) = self.preview {
            return Some(slot.translate(preview.offset));
        }
        if !ui.input(|input| input.pointer.primary_down()) || visible.height() < pitch {
            return None;
        }
        let pointer = ui.ctx().pointer_interact_pos()?;
        let center = pointer
            .y
            .clamp(visible.top() + pitch / 2.0, visible.bottom() - pitch / 2.0);
        Some(Rect::from_min_size(
            egui::pos2(visible.left(), center - pitch / 2.0),
            vec2(visible.width(), pitch),
        ))
    }

    fn floating_ui(&self, ui: &mut Ui, from: usize, rect: Rect) -> Ui {
        ui.new_child(
            egui::UiBuilder::new()
                .layer_id(LayerId::new(
                    Order::Tooltip,
                    self.id().with(("preview", from)),
                ))
                .max_rect(rect),
        )
    }

    /// Show the header and the rows of `items`, laying each cell out with
    /// `render_cell` (the cell's ui, the item, its row index and the column
    /// index). Each row's grip is read as `grip_label` of its item.
    /// Returns `true` when a row was moved.
    pub fn show<T>(
        self,
        ui: &mut Ui,
        items: &mut Vec<T>,
        grip_label: impl Fn(&T) -> String,
        mut render_cell: impl FnMut(&mut Ui, &mut T, usize, usize),
    ) -> bool {
        let original_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let widths = resolve_widths(&self.table.columns, GRIP_COLUMN, ui.available_width());
        let width = GRIP_COLUMN + widths.iter().sum::<f32>();
        let (header, _) = ui.allocate_exact_size(vec2(width, size::TABLE_HEADER), Sense::hover());
        let layout = ColumnLayout::new(
            header.left(),
            std::iter::once(GRIP_COLUMN).chain(widths.iter().copied()),
        );
        show_column_headers(
            &self.table,
            ui,
            &layout,
            header.top(),
            1,
            &mut HeaderOptions::default(),
        );

        let count = items.len();
        let pitch = size::ROW_PITCH;
        let table_rect = Rect::from_min_size(ui.cursor().min, vec2(width, count as f32 * pitch));
        let visible = table_rect.intersect(ui.clip_rect());
        let state = self.drag_state(ui, count, table_rect, pitch);
        let gap = state.from.map(|from| insertion_index(from, state.drop));
        let mut order = (0..count).filter(|index| Some(*index) != state.from);
        let mut gap_rect = None;
        for visual in 0..count {
            let (row, _) = ui.allocate_exact_size(vec2(width, pitch), Sense::hover());
            if gap == Some(visual) {
                paint_row_tile(ui, row, RowLook::Gap);
                gap_rect = Some(row);
                continue;
            }
            let index = order
                .next()
                .expect("every row that is not dragged is shown");
            let look = if ui.rect_contains_pointer(tile_rect(row)) {
                RowLook::Hovered
            } else {
                RowLook::Rest
            };
            paint_row_tile(ui, row, look);
            let tile = tile_rect(row);
            let handle = ReorderHandle {
                table_id: self.id(),
                index,
            };
            handle.grip(
                ui,
                layout.cell(0, tile.top(), tile.height()),
                &grip_label(&items[index]),
            );
            show_cells(
                &self.table,
                ui,
                &layout,
                tile,
                index,
                &mut items[index],
                &mut render_cell,
            );
        }

        if let (Some(from), Some(slot)) = (state.from, gap_rect)
            && let Some(rect) = self.floating_rect(ui, visible, slot, pitch)
        {
            let mut floating = self.floating_ui(ui, from, rect);
            let layout = ColumnLayout::new(
                rect.left(),
                std::iter::once(GRIP_COLUMN).chain(widths.iter().copied()),
            );
            paint_row_tile(&floating, rect, RowLook::Dragged);
            let tile = tile_rect(rect);
            paint_grip(&floating, layout.cell(0, tile.top(), tile.height()), true);
            show_cells(
                &self.table,
                &mut floating,
                &layout,
                tile,
                from,
                &mut items[from],
                &mut render_cell,
            );
        }
        ui.spacing_mut().item_spacing = original_spacing;
        Self::finish_drag(ui, state, visible, items)
    }

    /// Show rows the caller lays out itself, `row_height` points apart and
    /// `table_width` wide, with a drop gap and a floating copy
    /// (`render_preview`) of a dragged row. Register each row's handle
    /// with [`ReorderHandle::register`] or [`ReorderHandle::grip`].
    /// Returns `true` when an item was moved.
    pub fn show_rows<T>(
        self,
        ui: &mut Ui,
        items: &mut Vec<T>,
        row_height: f32,
        table_width: f32,
        mut render_row: impl FnMut(&mut Ui, &mut T, usize, &ReorderHandle),
        mut render_preview: impl FnMut(&mut Ui, &T),
    ) -> bool {
        let count = items.len();
        let table_rect = Rect::from_min_size(
            ui.cursor().min,
            vec2(table_width, count as f32 * row_height),
        );
        let visible = table_rect.intersect(ui.clip_rect());
        let state = self.drag_state(ui, count, table_rect, row_height);
        let gap = state.from.map(|from| insertion_index(from, state.drop));
        let mut order = (0..count).filter(|index| Some(*index) != state.from);
        let mut gap_rect = None;
        for visual in 0..count {
            if gap == Some(visual) {
                let (rect, _) =
                    ui.allocate_exact_size(vec2(table_width, row_height), Sense::hover());
                paint_tile(ui, rect, RowLook::Gap);
                gap_rect = Some(rect);
                continue;
            }
            let index = order
                .next()
                .expect("every row that is not dragged is shown");
            let handle = ReorderHandle {
                table_id: self.id(),
                index,
            };
            render_row(ui, &mut items[index], index, &handle);
        }

        if let (Some(from), Some(slot)) = (state.from, gap_rect)
            && let Some(rect) = self.floating_rect(ui, visible, slot, row_height)
        {
            let mut floating = self.floating_ui(ui, from, rect);
            paint_tile(&floating, rect, RowLook::Dragged);
            render_preview(&mut floating, &items[from]);
        }
        Self::finish_drag(ui, state, visible, items)
    }
}

/// The cells of one row of `table` across `tile`, after the grip column.
fn show_cells<T>(
    table: &Table,
    ui: &mut Ui,
    layout: &ColumnLayout,
    tile: Rect,
    row: usize,
    item: &mut T,
    render_cell: &mut impl FnMut(&mut Ui, &mut T, usize, usize),
) {
    for (index, column) in table.columns.iter().enumerate() {
        let rect = layout.cell(index + 1, tile.top(), tile.height());
        show_cell(
            ui,
            rect,
            column.align,
            column.cell_inset(),
            column.tabular,
            cell_width_id(table, index),
            |ui| {
                render_cell(ui, item, row, index);
            },
        );
    }
}

/// Move an item to a row boundary, returning whether its order changed.
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= items.len() || to > items.len() {
        return false;
    }
    let item = items.remove(from);
    let insert_at = insertion_index(from, to);
    if insert_at == from {
        items.insert(from, item);
        return false;
    }
    items.insert(insert_at, item);
    true
}

fn insertion_index(from: usize, to: usize) -> usize {
    if from < to { to - 1 } else { to }
}

fn drop_index(
    pointer_y: f32,
    table_rect: egui::Rect,
    count: usize,
    from: usize,
    row_height: f32,
) -> usize {
    let offset = (pointer_y - table_rect.top()).clamp(0.0, table_rect.height());
    let visual_index = (offset / row_height)
        .floor()
        .min(count.saturating_sub(1) as f32) as usize;
    if visual_index >= from {
        visual_index + 1
    } else {
        visual_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_item_uses_row_boundaries() {
        let mut items = vec!["a", "b", "c"];
        assert!(move_item(&mut items, 0, 2));
        assert_eq!(items, ["b", "a", "c"]);
        assert!(move_item(&mut items, 1, 3));
        assert_eq!(items, ["b", "c", "a"]);
        assert!(!move_item(&mut items, 2, 2));
    }

    #[test]
    fn dragging_a_grip_lifts_the_row_and_drops_it_at_the_gap() {
        use crate::TableRowBuilder;
        use crate::test_support::{UiHarnessSnapshot, primary_press, primary_release};
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        use std::cell::RefCell;
        use std::rc::Rc;

        let items = Rc::new(RefCell::new(vec!["Busybox", "Ubuntu", "Netshoot"]));
        let items_in_ui = items.clone();
        let mut harness = Harness::new_ui(move |ui| {
            ui.set_max_width(392.0);
            ReorderableTable::new("presets")
                .column("name", "Name", |col| col.fill_remaining())
                .show(
                    ui,
                    &mut items_in_ui.borrow_mut(),
                    |item| format!("Reorder {item}"),
                    |ui, item, _, _| {
                        TableRowBuilder::text(ui, item, false);
                    },
                );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        let grip = harness.get_by_label("Reorder Busybox").rect().center();
        let target = harness.get_by_label("Netshoot").rect().center();
        primary_press(&mut harness, grip);
        harness.run();
        harness.hover_at(grip + vec2(0.0, 10.0));
        harness.run();
        let drop = egui::pos2(grip.x, target.y + 8.0);
        harness.hover_at(drop);
        harness.run();
        harness
            .ui_harness("reorderable_table/dragging_a_grip_lifts_the_row_and_drops_it_at_the_gap");

        primary_release(&mut harness, drop);
        harness.run();
        assert_eq!(*items.borrow(), ["Ubuntu", "Netshoot", "Busybox"]);
    }
}
