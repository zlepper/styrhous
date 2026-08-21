use super::*;

impl TailwindTable {
    /// Create a new table with the given id source
    pub fn new(id_source: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            id: Id::new(id_source),
            columns: Vec::new(),
            is_selectable: false,
            fill_available_height: false,
            roomy: false,
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

    /// Expand the table's scroll surface to the remaining available height.
    ///
    /// This is useful for primary workspace tables, where a short result set should
    /// still read as a deliberate working surface instead of a floating list.
    pub fn fill_available_height(mut self) -> Self {
        self.fill_available_height = true;
        self
    }

    /// Use the spacious rhythm required by dense inspector content where
    /// controls are addressed at stable, larger hit targets.
    pub fn roomy(mut self) -> Self {
        self.roomy = true;
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
        self.show_with_row_response(
            ui,
            items,
            move |ui, item, column_index| {
                render_cell(ui, item, column_index);
            },
            |_, _, _| {},
        );
    }

    /// Show the table and receive an interactive response for every cell.
    ///
    /// Callers receive every cell response with its column index, so they can
    /// attach a context menu across the row or keep a primary action scoped to
    /// a specific cell while preserving egui's per-widget interaction model.
    pub fn show_with_row_response<'a, T>(
        self,
        ui: &mut Ui,
        items: &'a [T],
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
        let original_item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.x = 0.0;

        // Build columns for egui_extras::TableBuilder
        let mut table = TableBuilder::new(ui)
            .id_salt(self.id)
            .striped(false) // We handle striping ourselves for correct colors
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(available_height);

        if self.fill_available_height {
            table = table
                .auto_shrink([false, false])
                .min_scrolled_height((available_height - header_height).max(0.0));
        }

        // Add columns
        for col in &self.columns {
            table = table.column(egui_column(col));
        }

        table
            .header(header_height, |mut header| {
                for col in &self.columns {
                    header.col(|ui| {
                        // White background for header
                        let rect = ui.max_rect();
                        ui.painter().rect_filled(rect, 0.0, HEADER_BG);
                        ui.painter().line_segment(
                            [rect.left_bottom(), rect.right_bottom()],
                            egui::Stroke::new(1.0, TABLE_BORDER),
                        );

                        let mut label_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(rect.translate(egui::vec2(0.0, -4.0)))
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        );
                        label_ui.horizontal(|ui| {
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

                    // Render each column
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
                            ui.painter().rect_filled(rect, 0.0, CONTENT_BACKGROUND);
                            ui.painter().line_segment(
                                [rect.left_bottom(), rect.right_bottom()],
                                egui::Stroke::new(1.0, TABLE_BORDER),
                            );

                            // Add padding and render cell content
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
}
