use super::*;

struct ConfigurableTableLayout {
    available_height: f32,
    header_height: f32,
    row_height: f32,
    cell_padding_x: f32,
    table_id: Id,
    content_width: f32,
}

struct DataCell<'column, 'item, T> {
    column: &'column TableColumn,
    item: &'item T,
    row_index: usize,
    column_index: usize,
    background: Color32,
}

impl ConfigurableTableLayout {
    fn new(table: &TailwindTable, ui: &Ui, selection_width: f32) -> Self {
        let (header_height, row_height, cell_padding_x) = if table.roomy {
            (ROOMY_HEADER_HEIGHT, ROOMY_ROW_HEIGHT, ROOMY_CELL_PADDING_X)
        } else {
            (HEADER_HEIGHT, ROW_HEIGHT, CELL_PADDING_X)
        };
        Self {
            available_height: ui.available_height(),
            header_height,
            row_height,
            cell_padding_x,
            table_id: table.id,
            content_width: selection_width
                + table
                    .columns
                    .iter()
                    .map(|column| column.initial_width)
                    .sum::<f32>(),
        }
    }

    fn builder<'a>(&self, ui: &'a mut Ui, fill_available_height: bool) -> TableBuilder<'a> {
        let mut table = TableBuilder::new(ui)
            .id_salt(self.table_id)
            .striped(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .drag_to_scroll(egui::scroll_area::DragScroll::Never)
            .max_scroll_height(self.available_height);
        if fill_available_height {
            table = table
                .auto_shrink([false, false])
                .min_scrolled_height((self.available_height - self.header_height).max(0.0));
        }
        table
    }

    fn show_data_header(
        &self,
        ui: &mut Ui,
        column: &TableColumn,
        sort_state: Option<&SortState>,
        resized: &mut impl FnMut(&str, f32),
        render_header: &mut impl FnMut(&egui::Response, &str, &str, bool),
    ) {
        let rect = ui.max_rect();
        paint_table_cell(ui, rect, HEADER_BG);
        let resize_rect = egui::Rect::from_min_max(
            egui::pos2(rect.right() - 8.0, rect.top()),
            rect.right_bottom(),
        );
        handle_column_resize(
            ui,
            self.table_id,
            &column.id,
            &column.header,
            column.initial_width,
            resize_rect,
            resized,
        );
        let header_response = ui.interact(
            rect.with_max_x(resize_rect.left()),
            self.table_id.with(("header-menu", &column.id)),
            egui::Sense::click(),
        );
        set_accessibility_label(ui, &header_response, format!("{} column", column.header));
        ui.horizontal(|ui| {
            ui.add_space(self.cell_padding_x);
            ui.label(
                egui::RichText::new(&column.header)
                    .font(typography::body())
                    .color(gray::_900)
                    .strong(),
            );
            if column.sortable {
                ui.add_space(4.0);
                render_sort_indicator(
                    ui,
                    sort_state.and_then(|state| {
                        (state.column_id == column.id).then_some(state.direction)
                    }),
                );
            }
        });
        render_header(
            &header_response,
            &column.id,
            &column.header,
            column.sortable,
        );
    }

    fn show_data_cell<'item, T>(
        &self,
        ui: &mut Ui,
        cell: DataCell<'_, 'item, T>,
        render_cell: &mut impl FnMut(&mut Ui, &'item T, usize),
        render_row: &mut impl FnMut(&egui::Response, &'item T, usize),
    ) {
        let rect = ui.max_rect();
        let interaction = row_context_menu_response(
            ui,
            rect,
            self.table_id,
            cell.row_index,
            cell.column_index,
            &cell.column.header,
        );
        paint_table_cell(ui, rect, cell.background);
        ui.horizontal(|ui| {
            ui.add_space(self.cell_padding_x);
            render_cell(ui, cell.item, cell.column_index);
        });
        render_row(&interaction, cell.item, cell.column_index);
    }
}

impl TailwindTable {
    #[allow(clippy::too_many_arguments)]
    pub fn show_configurable_with_row_response<'a, T>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        sort_state: Option<&SortState>,
        mut render_header: impl FnMut(&egui::Response, &str, &str, bool),
        mut resized: impl FnMut(&str, f32),
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
        mut render_row: impl FnMut(&egui::Response, &'a T, usize),
    ) {
        let layout = ConfigurableTableLayout::new(&self, ui, 0.0);
        let original_item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.x = 0.0;
        crate::scroll::horizontal()
            .id_salt(layout.table_id.with("horizontal-scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(layout.content_width);
                let mut table = layout.builder(ui, self.fill_available_height);
                for column in &self.columns {
                    table = table.column(Column::initial(column.initial_width).clip(true));
                }
                table
                    .header(layout.header_height, |mut header| {
                        for column in &self.columns {
                            header.col(|ui| {
                                layout.show_data_header(
                                    ui,
                                    column,
                                    sort_state,
                                    &mut resized,
                                    &mut render_header,
                                );
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(layout.row_height, items.len(), |mut row| {
                            let row_index = row.index();
                            let item = &items[row_index];
                            for (column_index, column) in self.columns.iter().enumerate() {
                                row.col(|ui| {
                                    layout.show_data_cell(
                                        ui,
                                        DataCell {
                                            column,
                                            item,
                                            row_index,
                                            column_index,
                                            background: CONTENT_BACKGROUND,
                                        },
                                        &mut render_cell,
                                        &mut render_row,
                                    );
                                });
                            }
                        });
                    });
            });
        ui.spacing_mut().item_spacing = original_item_spacing;
    }

    /// Show a selectable table with persisted-width resize handles and a header callback.
    ///
    /// This variant is intended for configurable data tables. It keeps the existing
    /// virtual vertical body while an outer horizontal scroll area exposes columns
    /// that no longer fit the workspace.
    #[allow(clippy::too_many_arguments)]
    pub fn show_selectable_configurable_with_row_response<'a, T, K>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        selection: &mut HashSet<K>,
        key_fn: impl for<'item> Fn(&'item T) -> Option<&'item K>,
        sort_state: Option<&SortState>,
        mut render_header: impl FnMut(&egui::Response, &str, &str, bool),
        mut resized: impl FnMut(&str, f32),
        mut render_cell: impl FnMut(&mut Ui, &'a T, usize),
        mut render_row: impl FnMut(&egui::Response, &'a T, usize),
    ) where
        K: Eq + Hash + Clone,
    {
        debug_assert!(
            self.is_selectable,
            "selectable tables must call .selectable()"
        );
        let layout = ConfigurableTableLayout::new(&self, ui, CHECKBOX_COL_WIDTH);
        let (visible_count, selected_count) = items.iter().filter_map(&key_fn).fold(
            (0, 0),
            |(visible_count, selected_count), key| {
                (
                    visible_count + 1,
                    selected_count + usize::from(selection.contains(key)),
                )
            },
        );
        let select_all_state = if visible_count == 0 || selected_count == 0 {
            CheckboxState::Unchecked
        } else if selected_count == visible_count {
            CheckboxState::Checked
        } else {
            CheckboxState::Indeterminate
        };
        let original_item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.x = 0.0;

        crate::scroll::horizontal()
            .id_salt(layout.table_id.with("horizontal-scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(layout.content_width);
                let mut table = layout.builder(ui, self.fill_available_height);
                table = table.column(Column::exact(CHECKBOX_COL_WIDTH));
                for column in &self.columns {
                    table = table.column(Column::initial(column.initial_width).clip(true));
                }
                table
                    .header(layout.header_height, |mut header| {
                        header.col(|ui| {
                            let rect = ui.max_rect();
                            paint_table_cell(ui, rect, HEADER_BG);
                            let mut checkbox_pointer_clicked = false;
                            ui.horizontal_centered(|ui| {
                                ui.add_space(layout.cell_padding_x);
                                let response =
                                    render_checkbox(ui, select_all_state, "Select all rows");
                                let press_id =
                                    layout.table_id.with("selection-header-checkbox-press");
                                let mut press_started_on_checkbox =
                                    ui.data(|data| data.get_temp::<()>(press_id).is_some());
                                ui.input(|input| {
                                    for event in &input.events {
                                        let egui::Event::PointerButton {
                                            pos,
                                            button: egui::PointerButton::Primary,
                                            pressed,
                                            ..
                                        } = event
                                        else {
                                            continue;
                                        };

                                        if *pressed {
                                            press_started_on_checkbox =
                                                response.rect.contains(*pos);
                                        } else {
                                            checkbox_pointer_clicked |= press_started_on_checkbox
                                                && response.rect.contains(*pos);
                                            press_started_on_checkbox = false;
                                        }
                                    }
                                });

                                ui.data_mut(|data| {
                                    data.remove::<()>(press_id);
                                    if press_started_on_checkbox {
                                        data.insert_temp(press_id, ());
                                    }
                                });
                                if response.clicked() || checkbox_pointer_clicked {
                                    toggle_visible_selection(
                                        selection,
                                        items.iter().filter_map(&key_fn),
                                        selected_count == visible_count,
                                    );
                                }
                            });
                            let header_response = ui.interact(
                                rect,
                                layout.table_id.with("selection-header-menu"),
                                egui::Sense::click(),
                            );
                            set_accessibility_label(ui, &header_response, "Selection column");
                            if !checkbox_pointer_clicked {
                                render_header(&header_response, "selection", "Selection", false);
                            }
                        });
                        for column in &self.columns {
                            header.col(|ui| {
                                layout.show_data_header(
                                    ui,
                                    column,
                                    sort_state,
                                    &mut resized,
                                    &mut render_header,
                                );
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(layout.row_height, items.len(), |mut row| {
                            let row_index = row.index();
                            let item = &items[row_index];
                            let item_key = key_fn(item);
                            let selected =
                                item_key.as_ref().is_some_and(|key| selection.contains(key));
                            row.col(|ui| {
                                let rect = ui.max_rect();
                                paint_table_cell(ui, rect, WHITE);
                                ui.horizontal_centered(|ui| {
                                    ui.add_space(layout.cell_padding_x);
                                    if let Some(key) = item_key.as_ref() {
                                        let response = render_checkbox(
                                            ui,
                                            if selected {
                                                CheckboxState::Checked
                                            } else {
                                                CheckboxState::Unchecked
                                            },
                                            &format!("Select row {}", row_index + 1),
                                        );
                                        if response.clicked() {
                                            if selected {
                                                selection.remove(key);
                                            } else {
                                                selection.insert((*key).clone());
                                            }
                                        }
                                    }
                                });
                            });
                            for (column_index, column) in self.columns.iter().enumerate() {
                                row.col(|ui| {
                                    layout.show_data_cell(
                                        ui,
                                        DataCell {
                                            column,
                                            item,
                                            row_index,
                                            column_index,
                                            background: WHITE,
                                        },
                                        &mut render_cell,
                                        &mut render_row,
                                    );
                                });
                            }
                        });
                    });
            });
        ui.spacing_mut().item_spacing = original_item_spacing;
    }
}

fn toggle_visible_selection<'a, K>(
    selection: &mut HashSet<K>,
    visible_keys: impl IntoIterator<Item = &'a K>,
    all_selected: bool,
) where
    K: Eq + Hash + Clone + 'a,
{
    if all_selected {
        for key in visible_keys {
            selection.remove(key);
        }
    } else {
        selection.extend(visible_keys.into_iter().cloned());
    }
}
