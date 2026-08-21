use super::*;

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
        let available_height = ui.available_height();
        let (header_height, row_height, cell_padding_x) = if self.roomy {
            (ROOMY_HEADER_HEIGHT, ROOMY_ROW_HEIGHT, ROOMY_CELL_PADDING_X)
        } else {
            (HEADER_HEIGHT, ROW_HEIGHT, CELL_PADDING_X)
        };
        let num_columns = self.columns.len();
        let table_id = self.id;
        let content_width = self
            .columns
            .iter()
            .map(|column| column.initial_width)
            .sum::<f32>();
        let original_item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.x = 0.0;
        crate::scroll::horizontal()
            .id_salt(table_id.with("horizontal-scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(content_width);
                let mut table = TableBuilder::new(ui)
                    .id_salt(table_id)
                    .striped(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .drag_to_scroll(egui::scroll_area::DragScroll::Never)
                    .max_scroll_height(available_height);
                if self.fill_available_height {
                    table = table
                        .auto_shrink([false, false])
                        .min_scrolled_height((available_height - header_height).max(0.0));
                }
                for column in &self.columns {
                    table = table.column(Column::initial(column.initial_width).clip(true));
                }
                table
                    .header(header_height, |mut header| {
                        for column in &self.columns {
                            header.col(|ui| {
                                let rect = ui.max_rect();
                                ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                                ui.painter().line_segment(
                                    [rect.left_bottom(), rect.right_bottom()],
                                    egui::Stroke::new(1.0, TABLE_BORDER),
                                );
                                let resize_rect = egui::Rect::from_min_max(
                                    egui::pos2(rect.right() - 8.0, rect.top()),
                                    rect.right_bottom(),
                                );
                                handle_column_resize(
                                    ui,
                                    table_id,
                                    &column.id,
                                    &column.header,
                                    column.initial_width,
                                    resize_rect,
                                    &mut resized,
                                );
                                let header_response = ui.interact(
                                    rect.with_max_x(resize_rect.left()),
                                    table_id.with(("header-menu", &column.id)),
                                    egui::Sense::click(),
                                );
                                set_accessibility_label(
                                    ui,
                                    &header_response,
                                    format!("{} column", column.header),
                                );
                                ui.horizontal(|ui| {
                                    ui.add_space(cell_padding_x);
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
                                                (state.column_id == column.id)
                                                    .then_some(state.direction)
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
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(row_height, items.len(), |mut row| {
                            let row_index = row.index();
                            let item = &items[row_index];
                            for column_index in 0..num_columns {
                                let mut interaction = None;
                                row.col(|ui| {
                                    let rect = ui.max_rect();
                                    interaction = Some(row_context_menu_response(
                                        ui,
                                        rect,
                                        table_id,
                                        row_index,
                                        column_index,
                                        &self.columns[column_index].header,
                                    ));
                                    ui.painter().rect_filled(rect, 0.0, CONTENT_BACKGROUND);
                                    ui.painter().line_segment(
                                        [rect.left_bottom(), rect.right_bottom()],
                                        egui::Stroke::new(1.0, TABLE_BORDER),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.add_space(cell_padding_x);
                                        render_cell(ui, item, column_index);
                                    });
                                });
                                render_row(
                                    &interaction.expect("table cell should register interaction"),
                                    item,
                                    column_index,
                                );
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
        key_fn: impl Fn(&T) -> Option<K>,
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
        let available_height = ui.available_height();
        let (header_height, row_height, cell_padding_x) = if self.roomy {
            (ROOMY_HEADER_HEIGHT, ROOMY_ROW_HEIGHT, ROOMY_CELL_PADDING_X)
        } else {
            (HEADER_HEIGHT, ROW_HEIGHT, CELL_PADDING_X)
        };
        let num_columns = self.columns.len();
        let visible_keys = items.iter().filter_map(&key_fn).collect::<Vec<_>>();
        let selected_count = visible_keys
            .iter()
            .filter(|key| selection.contains(*key))
            .count();
        let select_all_state = if visible_keys.is_empty() || selected_count == 0 {
            CheckboxState::Unchecked
        } else if selected_count == visible_keys.len() {
            CheckboxState::Checked
        } else {
            CheckboxState::Indeterminate
        };
        let table_id = self.id;
        let content_width = CHECKBOX_COL_WIDTH
            + self
                .columns
                .iter()
                .map(|column| column.initial_width)
                .sum::<f32>();
        let original_item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.x = 0.0;

        crate::scroll::horizontal()
            .id_salt(table_id.with("horizontal-scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(content_width);
                let mut table = TableBuilder::new(ui)
                    .id_salt(table_id)
                    .striped(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .drag_to_scroll(egui::scroll_area::DragScroll::Never)
                    .max_scroll_height(available_height);
                if self.fill_available_height {
                    table = table
                        .auto_shrink([false, false])
                        .min_scrolled_height((available_height - header_height).max(0.0));
                }
                table = table.column(Column::exact(CHECKBOX_COL_WIDTH));
                for column in &self.columns {
                    table = table.column(Column::initial(column.initial_width).clip(true));
                }
                table
                    .header(header_height, |mut header| {
                        header.col(|ui| {
                            let rect = ui.max_rect();
                            ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                            ui.painter().line_segment(
                                [rect.left_bottom(), rect.right_bottom()],
                                egui::Stroke::new(1.0, TABLE_BORDER),
                            );
                            ui.horizontal_centered(|ui| {
                                ui.add_space(cell_padding_x);
                                let response =
                                    render_checkbox(ui, select_all_state, "Select all rows");
                                if response.clicked() {
                                    if selected_count == visible_keys.len() {
                                        for key in &visible_keys {
                                            selection.remove(key);
                                        }
                                    } else {
                                        selection.extend(visible_keys.iter().cloned());
                                    }
                                }
                            });
                            let header_response = ui.interact(
                                rect,
                                table_id.with("selection-header-menu"),
                                egui::Sense::click(),
                            );
                            set_accessibility_label(ui, &header_response, "Selection column");
                            render_header(&header_response, "selection", "Selection", false);
                        });
                        for column in &self.columns {
                            header.col(|ui| {
                                let rect = ui.max_rect();
                                ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                                ui.painter().line_segment(
                                    [rect.left_bottom(), rect.right_bottom()],
                                    egui::Stroke::new(1.0, TABLE_BORDER),
                                );
                                let resize_rect = egui::Rect::from_min_max(
                                    egui::pos2(rect.right() - 8.0, rect.top()),
                                    rect.right_bottom(),
                                );
                                handle_column_resize(
                                    ui,
                                    table_id,
                                    &column.id,
                                    &column.header,
                                    column.initial_width,
                                    resize_rect,
                                    &mut resized,
                                );
                                let header_response = ui.interact(
                                    rect.with_max_x(resize_rect.left()),
                                    table_id.with(("header-menu", &column.id)),
                                    egui::Sense::click(),
                                );
                                set_accessibility_label(
                                    ui,
                                    &header_response,
                                    format!("{} column", column.header),
                                );
                                ui.horizontal(|ui| {
                                    ui.add_space(cell_padding_x);
                                    ui.label(
                                        egui::RichText::new(&column.header)
                                            .font(typography::body())
                                            .color(gray::_900)
                                            .strong(),
                                    );
                                    if column.sortable {
                                        ui.add_space(4.0);
                                        let direction = sort_state.and_then(|state| {
                                            (state.column_id == column.id)
                                                .then_some(state.direction)
                                        });
                                        render_sort_indicator(ui, direction);
                                    }
                                });
                                render_header(
                                    &header_response,
                                    &column.id,
                                    &column.header,
                                    column.sortable,
                                );
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(row_height, items.len(), |mut row| {
                            let row_index = row.index();
                            let item = &items[row_index];
                            let item_key = key_fn(item);
                            let selected =
                                item_key.as_ref().is_some_and(|key| selection.contains(key));
                            row.col(|ui| {
                                let rect = ui.max_rect();
                                ui.painter().rect_filled(rect, 0.0, WHITE);
                                ui.painter().line_segment(
                                    [rect.left_bottom(), rect.right_bottom()],
                                    egui::Stroke::new(1.0, TABLE_BORDER),
                                );
                                ui.horizontal_centered(|ui| {
                                    ui.add_space(cell_padding_x);
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
                                                selection.insert(key.clone());
                                            }
                                        }
                                    }
                                });
                            });
                            for column_index in 0..num_columns {
                                let mut interaction = None;
                                row.col(|ui| {
                                    let rect = ui.max_rect();
                                    interaction = Some(row_context_menu_response(
                                        ui,
                                        rect,
                                        table_id,
                                        row_index,
                                        column_index,
                                        &self.columns[column_index].header,
                                    ));
                                    ui.painter().rect_filled(rect, 0.0, WHITE);
                                    ui.painter().line_segment(
                                        [rect.left_bottom(), rect.right_bottom()],
                                        egui::Stroke::new(1.0, TABLE_BORDER),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.add_space(cell_padding_x);
                                        render_cell(ui, item, column_index);
                                    });
                                });
                                render_row(
                                    &interaction.expect("table cell should register interaction"),
                                    item,
                                    column_index,
                                );
                            }
                        });
                    });
            });
        ui.spacing_mut().item_spacing = original_item_spacing;
    }
}
