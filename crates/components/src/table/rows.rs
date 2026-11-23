use super::*;

/// Builder passed to the row render closure for rendering cells
pub struct TableRowBuilder<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl TableRowBuilder<'_> {
    /// Render text in a cell with appropriate styling
    ///
    /// The first column uses primary text, the others secondary text.
    pub fn text(ui: &mut Ui, text: &str, is_first_column: bool) {
        let tokens = ui.tokens();
        let color = if is_first_column {
            tokens.text.primary
        } else {
            tokens.text.secondary
        };
        ui.label(
            egui::RichText::new(text)
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(color),
        );
    }

    /// Render a text-styled button for a navigable table cell.
    ///
    /// The button deliberately has no chrome so it retains the table's visual
    /// language, while still letting egui route pointer and keyboard input to
    /// the cell that owns it.
    pub fn clickable_text(
        ui: &mut Ui,
        text: &str,
        color: Color32,
        accessibility_label: impl Into<WidgetText>,
    ) -> egui::Response {
        let accessibility_label = accessibility_label.into();
        let response = ui
            .add(
                Button::new(
                    egui::RichText::new(text)
                        .font(type_scale::BODY.font_id(ui.ctx()))
                        .color(color),
                )
                .small()
                .frame(false),
            )
            .with_pointing_hand();
        let is_enabled = ui.is_enabled();
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                is_enabled,
                accessibility_label.text(),
            )
        });
        response
    }
}

/// Checkbox state for rendering
#[derive(Clone, Copy, PartialEq)]
pub(super) enum CheckboxState {
    Unchecked,
    Checked,
    Indeterminate,
}

pub(super) fn paint_resize_handle(ui: &Ui, rect: egui::Rect, response: &egui::Response) {
    let tokens = ui.tokens();
    let color = if response.hovered() || response.dragged() {
        tokens.accent.fill
    } else {
        tokens.divider.base
    };
    let x = rect.center().x;
    let stroke = stroke::hairline(color);
    ui.painter().line_segment(
        [
            egui::pos2(x, rect.top() + 8.0),
            egui::pos2(x, rect.bottom() - 8.0),
        ],
        stroke,
    );
}

pub(super) fn row_context_menu_response(
    ui: &mut Ui,
    rect: egui::Rect,
    table_id: Id,
    row_index: usize,
    column_index: usize,
    column_header: &str,
) -> egui::Response {
    let response = ui.interact(
        rect,
        table_id.with(("row-context-menu", row_index, column_index)),
        egui::Sense::click(),
    );
    set_accessibility_label(
        ui,
        &response,
        format!("{column_header}, row {}", row_index + 1),
    );
    response
}

/// Paint a body cell: the row tile (`row-bg`) and the divider below it.
pub(super) fn paint_row_cell(ui: &Ui, rect: egui::Rect) {
    paint_table_cell(ui, rect, ui.tokens().components.row_bg);
}

/// Paint a header cell: transparent over the surface, with the divider below it.
pub(super) fn paint_header_cell(ui: &Ui, rect: egui::Rect) {
    paint_table_cell(ui, rect, Color32::TRANSPARENT);
}

fn paint_table_cell(ui: &Ui, rect: egui::Rect, background: Color32) {
    let tokens = ui.tokens();
    ui.painter().rect_filled(rect, 0.0, background);
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        stroke::divider(&tokens),
    );
}

/// A column header label (`.th`).
pub(super) fn header_label(ui: &mut Ui, text: &str) {
    let color = ui.tokens().text.muted;
    ui.label(
        egui::RichText::new(text)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(color),
    );
}

pub(super) fn set_accessibility_label(
    ui: &Ui,
    response: &egui::Response,
    label: impl Into<String>,
) {
    let label = label.into();
    ui.ctx().accesskit_node_builder(response.id, |builder| {
        builder.set_label(label);
    });
}

pub(super) fn handle_column_resize(
    ui: &Ui,
    table_id: Id,
    column_id: &str,
    column_header: &str,
    width: f32,
    resize_rect: egui::Rect,
    resized: &mut impl FnMut(&str, f32),
) {
    let resize_id = table_id.with(("resize", column_id));
    let response = ui
        .interact(resize_rect, resize_id, egui::Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("Resize {column_header} column"),
        )
    });
    paint_resize_handle(ui, resize_rect, &response);

    let drag_start_id = table_id.with(("resize-start", column_id));
    if response.drag_started() {
        ui.data_mut(|data| data.insert_temp(drag_start_id, width));
    }
    if let Some(delta) = response.total_drag_delta()
        && let Some(start_width) = ui.data(|data| data.get_temp::<f32>(drag_start_id))
    {
        resized(column_id, start_width + delta.x);
        ui.ctx().request_repaint();
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<f32>(drag_start_id));
    }
}

/// Render the checkbox style used by resource-table multi-selection controls.
pub fn tailwind_checkbox(ui: &mut Ui, checked: bool, label: &str) -> egui::Response {
    render_checkbox(
        ui,
        if checked {
            CheckboxState::Checked
        } else {
            CheckboxState::Unchecked
        },
        label,
    )
}

/// A checkbox with a visible label; clicking either toggles `checked`.
pub fn checkbox_field(ui: &mut Ui, checked: &mut bool, label: &str) -> egui::Response {
    ui.horizontal(|ui| {
        let box_response = tailwind_checkbox(ui, *checked, label);
        ui.add_space(space::S8);
        let label_response = ui
            .add(
                egui::Label::new(
                    egui::RichText::new(label)
                        .font(type_scale::BODY.font_id(ui.ctx()))
                        .color(ui.tokens().text.primary),
                )
                .sense(egui::Sense::click()),
            )
            .with_pointing_hand();
        if box_response.clicked() || label_response.clicked() {
            *checked = !*checked;
        }
        box_response | label_response
    })
    .inner
}

pub(super) fn render_checkbox(ui: &mut Ui, state: CheckboxState, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(CHECKBOX_SIZE), egui::Sense::click());
    let response = response.with_pointing_hand();

    if ui.is_rect_visible(rect) {
        let tokens = ui.tokens();
        let painter = ui.painter();
        let rounding = egui::CornerRadius::same(radius::KEY);

        match state {
            CheckboxState::Unchecked => {
                // Border only
                painter.rect_stroke(
                    rect,
                    rounding,
                    stroke::control_border(tokens.control.border, painter.pixels_per_point()),
                    egui::StrokeKind::Inside,
                );
            }
            CheckboxState::Checked => {
                // Filled background
                painter.rect_filled(rect, rounding, tokens.accent.fill);
                // Checkmark
                let check_color = tokens.text.on_fill;
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
                painter.rect_filled(rect, rounding, tokens.accent.fill);
                // Horizontal dash
                let dash_color = tokens.text.on_fill;
                let stroke = egui::Stroke::new(2.0, dash_color);
                let center = rect.center();
                let half_width = CHECKBOX_SIZE * 0.25;
                painter.line_segment(
                    [
                        center - egui::vec2(half_width, 0.0),
                        center + egui::vec2(half_width, 0.0),
                    ],
                    stroke,
                );
            }
        }
    }

    response.widget_info(|| match state {
        CheckboxState::Unchecked => {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), false, label)
        }
        CheckboxState::Checked => {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), true, label)
        }
        CheckboxState::Indeterminate => {
            egui::WidgetInfo::labeled(egui::WidgetType::Checkbox, ui.is_enabled(), label)
        }
    });
    response
}
