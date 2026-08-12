//! Shared drag-and-drop row reordering for settings-style tables.

use egui::{Id, LayerId, Order, Response, Ui};

use crate::colors::indigo;

#[derive(Clone)]
struct DragPayload {
    table_id: Id,
    from: usize,
}

/// A drag handle supplied to a [`ReorderableTable`] row renderer.
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
}

/// A settings-table body that supports handle-based row reordering.
pub struct ReorderableTable {
    id: Id,
    row_height: f32,
}

impl ReorderableTable {
    pub fn new(id_source: impl std::hash::Hash + std::fmt::Debug, row_height: f32) -> Self {
        Self {
            id: Id::new(id_source),
            row_height,
        }
    }

    /// Render rows, a drop placeholder, and a non-interactive drag preview.
    /// Returns `true` when an item was moved.
    pub fn show<T: Clone>(
        &self,
        ui: &mut Ui,
        items: &mut Vec<T>,
        table_width: f32,
        mut render_row: impl FnMut(&mut Ui, &mut T, usize, &ReorderHandle),
        mut render_preview: impl FnMut(&mut Ui, &T),
    ) -> bool {
        let count = items.len();
        let table_rect = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(table_width, count as f32 * self.row_height),
        );
        let visible_table_rect = table_rect.intersect(ui.clip_rect());
        let drag = egui::DragAndDrop::payload::<DragPayload>(ui.ctx())
            .filter(|payload| payload.table_id == self.id && payload.from < count);
        let dragged_index = drag.as_ref().map(|payload| payload.from);
        if let Some(from) = dragged_index {
            egui::DragAndDrop::set_payload(
                ui.ctx(),
                DragPayload {
                    table_id: self.id,
                    from,
                },
            );
        }
        let drop_index = dragged_index.map_or(0, |from| {
            drop_index(
                ui.ctx()
                    .pointer_interact_pos()
                    .map_or(table_rect.center().y, |position| position.y),
                table_rect,
                count,
                from,
                self.row_height,
            )
        });
        let placeholder_index = dragged_index.map(|from| insertion_index(from, drop_index));
        let mut item_indices = (0..count).filter(|index| Some(*index) != dragged_index);
        for visual_index in 0..count {
            if placeholder_index == Some(visual_index) {
                placeholder(ui, table_width, self.row_height);
                continue;
            }
            let index = item_indices
                .next()
                .expect("every visible reorderable item has a row");
            let handle = ReorderHandle {
                table_id: self.id,
                index,
            };
            render_row(ui, &mut items[index], index, &handle);
        }

        if let Some(payload) = drag
            .as_ref()
            .filter(|_| ui.input(|input| input.pointer.primary_down()))
            && let Some(item) = items.get(payload.from)
            && visible_table_rect.height() >= self.row_height
            && let Some(pointer_position) = ui.ctx().pointer_interact_pos()
        {
            let preview_y = pointer_position.y.clamp(
                visible_table_rect.top() + self.row_height / 2.0,
                visible_table_rect.bottom() - self.row_height / 2.0,
            );
            let preview_rect = egui::Rect::from_min_size(
                egui::pos2(visible_table_rect.left(), preview_y - self.row_height / 2.0),
                egui::vec2(visible_table_rect.width(), self.row_height),
            );
            let mut preview_ui = ui.new_child(
                egui::UiBuilder::new()
                    .layer_id(LayerId::new(
                        Order::Tooltip,
                        self.id.with(("preview", payload.from)),
                    ))
                    .max_rect(preview_rect),
            );
            render_preview(&mut preview_ui, item);
        }

        let should_move = ui.input(|input| input.pointer.any_released())
            && ui
                .ctx()
                .pointer_interact_pos()
                .is_some_and(|position| visible_table_rect.contains(position));
        if should_move && let Some(payload) = drag {
            return move_item(items, payload.from, drop_index);
        }
        false
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

fn placeholder(ui: &mut Ui, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, indigo::_50);
    ui.painter().rect_stroke(
        rect.shrink(1.0),
        0.0,
        egui::Stroke::new(2.0, indigo::_500),
        egui::StrokeKind::Inside,
    );
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
}
