use super::*;

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
        selection: &mut HashSet<K>,
        key_fn: impl Fn(&T) -> K,
        render_cell: impl FnMut(&mut Ui, &'a T, usize),
    ) where
        K: Eq + Hash + Clone,
    {
        self.show_selectable_with_row_response(
            ui,
            items,
            selection,
            |item| Some(key_fn(item)),
            render_cell,
            |_, _, _| {},
        );
    }

    /// Show a selectable table and receive an interactive response for every
    /// data cell.
    ///
    /// This is the selectable counterpart to [`Self::show_with_row_response`].
    /// Checkbox interactions update `selection` directly; returning `None`
    /// from `key_fn` renders an unselectable row while preserving the regular
    /// row callback for callers that need contextual row interactions.
    pub fn show_selectable_with_row_response<'a, T, K>(
        self,
        ui: &mut Ui,
        items: &'a [T],
        selection: &mut HashSet<K>,
        key_fn: impl Fn(&T) -> Option<K>,
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
        let num_selectable_items = visible_keys.len();
        let table_id = self.id;
        let original_item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.x = 0.0;

        // Determine select-all checkbox state
        let selected_count = visible_keys
            .iter()
            .filter(|key| selection.contains(*key))
            .count();
        let select_all_state = if num_selectable_items == 0 || selected_count == 0 {
            CheckboxState::Unchecked
        } else if selected_count == num_selectable_items {
            CheckboxState::Checked
        } else {
            CheckboxState::Indeterminate
        };

        // Build columns for egui_extras::TableBuilder
        let mut table = TableBuilder::new(ui)
            .id_salt(self.id)
            .striped(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(available_height);

        if self.fill_available_height {
            table = table
                .auto_shrink([false, false])
                .min_scrolled_height((available_height - header_height).max(0.0));
        }

        // Add checkbox column first
        table = table.column(Column::exact(CHECKBOX_COL_WIDTH));

        // Add data columns
        for col in &self.columns {
            table = table.column(egui_column(col));
        }

        table
            .header(header_height, |mut header| {
                // Checkbox column header (select all)
                header.col(|ui| {
                    let rect = ui.max_rect();
                    ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                    ui.painter().line_segment(
                        [rect.left_bottom(), rect.right_bottom()],
                        egui::Stroke::new(1.0, TABLE_BORDER),
                    );
                    ui.horizontal_centered(|ui| {
                        ui.add_space(cell_padding_x);
                        let response = render_checkbox(ui, select_all_state, "Select all rows");
                        if response.clicked() {
                            if selected_count == num_selectable_items {
                                for key in &visible_keys {
                                    selection.remove(key);
                                }
                            } else {
                                selection.extend(visible_keys.iter().cloned());
                            }
                        }
                    });
                });

                // Data column headers
                for col in &self.columns {
                    header.col(|ui| {
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                        ui.painter().line_segment(
                            [rect.left_bottom(), rect.right_bottom()],
                            egui::Stroke::new(1.0, TABLE_BORDER),
                        );
                        ui.horizontal(|ui| {
                            ui.add_space(cell_padding_x);
                            ui.label(
                                egui::RichText::new(&col.header)
                                    .font(typography::body())
                                    .color(gray::_900)
                                    .strong(),
                            );
                        });
                    });
                }
            })
            .body(|body| {
                body.rows(row_height, items.len(), |mut row| {
                    let row_index = row.index();
                    let item = &items[row_index];
                    let item_key = key_fn(item);
                    let is_selected = item_key.as_ref().is_some_and(|key| selection.contains(key));
                    let bg_color = WHITE;

                    // Checkbox column
                    row.col(|ui| {
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(rect, 0.0, bg_color);
                        ui.painter().line_segment(
                            [rect.left_bottom(), rect.right_bottom()],
                            egui::Stroke::new(1.0, TABLE_BORDER),
                        );
                        ui.horizontal_centered(|ui| {
                            ui.add_space(cell_padding_x);
                            if let Some(key) = item_key.as_ref() {
                                let checkbox_state = if is_selected {
                                    CheckboxState::Checked
                                } else {
                                    CheckboxState::Unchecked
                                };
                                let response = render_checkbox(
                                    ui,
                                    checkbox_state,
                                    &format!("Select row {}", row_index + 1),
                                );
                                if response.clicked() {
                                    if is_selected {
                                        selection.remove(key);
                                    } else {
                                        selection.insert(key.clone());
                                    }
                                }
                            }
                        });
                    });

                    // Data columns
                    for col_index in 0..num_columns {
                        let mut interaction = None;
                        row.col(|ui| {
                            let rect = ui.max_rect();
                            interaction = Some(row_context_menu_response(
                                ui,
                                rect,
                                table_id,
                                row_index,
                                col_index,
                                &self.columns[col_index].header,
                            ));
                            ui.painter().rect_filled(rect, 0.0, bg_color);
                            ui.painter().line_segment(
                                [rect.left_bottom(), rect.right_bottom()],
                                egui::Stroke::new(1.0, TABLE_BORDER),
                            );
                            ui.horizontal(|ui| {
                                ui.add_space(cell_padding_x);
                                render_cell(ui, item, col_index);
                            });
                        });
                        let response = interaction.expect("table cell should register interaction");
                        render_row(&response, item, col_index);
                    }
                });
            });
        ui.spacing_mut().item_spacing = original_item_spacing;
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
        let original_item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.x = 0.0;

        // Track which column header was clicked
        let clicked_column = std::cell::RefCell::new(None::<String>);

        // Build columns for egui_extras::TableBuilder
        let mut table = TableBuilder::new(ui)
            .id_salt(self.id)
            .striped(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(available_height);

        // Add columns
        for col in &self.columns {
            table = table.column(egui_column(col));
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
                            let response = ui
                                .interact(rect, ui.id().with(&col.id), egui::Sense::click())
                                .with_pointing_hand();
                            if response.clicked() {
                                *clicked_column.borrow_mut() = Some(col.id.clone());
                            }
                            let sort_label = match sort_direction {
                                Some(SortDirection::Ascending) => {
                                    format!("Sort by {}; currently ascending", col.header)
                                }
                                Some(SortDirection::Descending) => {
                                    format!("Sort by {}; currently descending", col.header)
                                }
                                None => format!("Sort by {}", col.header),
                            };
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    sort_label.clone(),
                                )
                            });
                            Some(response)
                        } else {
                            None
                        };

                        ui.horizontal(|ui| {
                            ui.add_space(CELL_PADDING_X);

                            ui.label(
                                egui::RichText::new(&col.header)
                                    .font(typography::body())
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
                    let bg_color = WHITE;

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

        ui.spacing_mut().item_spacing = original_item_spacing;

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
