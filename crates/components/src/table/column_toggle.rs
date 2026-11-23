use super::*;

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
        let original_item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.x = 0.0;

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
            .id_salt(self.id)
            .striped(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(available_height);

        // Add visible columns + one narrow column for the settings icon
        for (_, col) in &visible_columns {
            table = table.column(egui_column(col));
        }
        // Add settings column (narrow, at the end)
        table = table.column(Column::exact(32.0));

        table
            .header(HEADER_HEIGHT, |mut header| {
                // Data column headers
                for (_, col) in &visible_columns {
                    header.col(|ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(CELL_PADDING_X);
                            header_label(ui, &col.header);
                        });
                    });
                }

                // Settings column header (gear icon placeholder)
                header.col(|ui| {
                    ui.centered_and_justified(|ui| {
                        render_settings_icon(ui);
                    });
                });
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, items.len(), |mut row| {
                    let row_index = row.index();
                    let item = &items[row_index];

                    // Visible data columns
                    for (original_index, _) in &visible_columns {
                        row.col(|ui| {
                            let rect = ui.max_rect();
                            ui.painter()
                                .rect_filled(rect, 0.0, ui.tokens().components.row_bg);
                            ui.horizontal(|ui| {
                                ui.add_space(CELL_PADDING_X);
                                render_cell(ui, item, *original_index);
                            });
                        });
                    }

                    // Empty settings column for rows
                    row.col(|ui| {
                        let rect = ui.max_rect();
                        ui.painter()
                            .rect_filled(rect, 0.0, ui.tokens().components.row_bg);
                    });
                });
            });

        ui.spacing_mut().item_spacing = original_item_spacing;

        // Note: The gear icon menu interaction would be implemented here
        // For now, we just show the icon - actual menu requires mutable hidden_columns
        let _ = num_visible_columns;
    }
}

/// Render a settings/menu icon using SVG
fn render_settings_icon(ui: &mut Ui) -> egui::Response {
    let color = ui.tokens().text.muted;
    icons::bars_3(ui, 16.0, color)
}

/// Render sort indicator using SVG icons
pub(super) fn render_sort_indicator(ui: &mut Ui, direction: Option<SortDirection>) {
    let tokens = ui.tokens();
    match direction {
        Some(SortDirection::Ascending) => {
            icons::chevron_up(ui, SORT_ICON_SIZE, tokens.accent.text);
        }
        Some(SortDirection::Descending) => {
            icons::chevron_down(ui, SORT_ICON_SIZE, tokens.accent.text);
        }
        None => {
            // Show subtle unsorted indicator - just use a lighter chevron down
            icons::chevron_down(ui, SORT_ICON_SIZE, tokens.text.disabled);
        }
    }
}
