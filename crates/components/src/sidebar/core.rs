use super::*;

pub(super) struct SidebarContentCore<'a> {
    pub(super) ui: &'a mut Ui,
    pub(super) id: Id,
    pub(super) item_height: f32,
    pub(super) show_text: bool,
    pub(super) dark: bool,
}

impl<'a> SidebarContentCore<'a> {
    pub(super) fn wide(ui: &'a mut Ui, dark: bool) -> Self {
        let id = ui.auto_id_with("wide-sidebar");
        Self {
            ui,
            id,
            item_height: WIDE_ITEM_HEIGHT,
            show_text: true,
            dark,
        }
    }

    pub(super) fn narrow(ui: &'a mut Ui, dark: bool) -> Self {
        let id = ui.auto_id_with("narrow-sidebar");
        Self {
            ui,
            id,
            item_height: NARROW_ITEM_HEIGHT,
            show_text: false,
            dark,
        }
    }

    pub(super) fn ui_mut(&mut self) -> &mut Ui {
        self.ui
    }

    pub(super) fn item(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        selected: bool,
    ) -> Response {
        let text = text.into();
        let text_str = text.text();

        let (rect, inner_rect, response) = allocate_item(self.ui, self.item_height);
        let colors = ItemColors::navigation(selected, response.hovered(), self.dark);

        let background_rect = if self.show_text {
            inner_rect
        } else {
            inner_rect.shrink2(Vec2::new(0.0, 2.0))
        };
        draw_background(self.ui.painter(), background_rect, colors.background);

        if self.show_text {
            render_icon(
                self.ui,
                icon_rect_wide(inner_rect, self.item_height),
                icon,
                colors.icon,
            );
            let text_is_truncated = render_text(
                self.ui.painter(),
                text_pos_after_icon(inner_rect, self.item_height),
                text_str,
                colors.text,
                inner_rect.right() - text_pos_after_icon(inner_rect, self.item_height).x,
            );
            if text_is_truncated {
                response.clone().on_hover_text(text_str);
            }
        } else {
            // For narrow mode, center icon within full rect (not inner_rect)
            render_icon(self.ui, icon_rect_centered(rect), icon, colors.icon);
            response.clone().on_hover_text(text_str);
        }

        add_selectable_accessibility(&response, self.ui, text_str, selected);
        response
    }

    pub(super) fn avatar_item(
        &mut self,
        text: impl Into<WidgetText>,
        initial: &str,
        selected: bool,
    ) -> Response {
        self.avatar_item_with_tooltip(text, initial, selected, None)
    }

    pub(super) fn avatar_item_with_tooltip(
        &mut self,
        text: impl Into<WidgetText>,
        initial: &str,
        selected: bool,
        tooltip: Option<&str>,
    ) -> Response {
        let text = text.into();
        let text_str = text.text();
        let tooltip = tooltip.unwrap_or(text_str);

        let (rect, inner_rect, response) = allocate_item(self.ui, self.item_height);
        let colors = ItemColors::navigation(selected, response.hovered(), self.dark);

        let background_rect = if self.show_text {
            inner_rect
        } else {
            inner_rect.shrink2(Vec2::new(0.0, 2.0))
        };
        draw_background(self.ui.painter(), background_rect, colors.background);

        if self.show_text {
            let avatar_x = CHEVRON_SIZE + CHEVRON_GAP;
            let avatar_center =
                inner_rect.min + Vec2::new(avatar_x + ICON_SIZE / 2.0, self.item_height / 2.0);
            render_avatar(
                self.ui.painter(),
                avatar_center,
                initial,
                self.dark,
                ICON_SIZE,
            );
            let text_is_truncated = render_text(
                self.ui.painter(),
                text_pos_after_icon(inner_rect, self.item_height),
                text_str,
                colors.text,
                inner_rect.right() - text_pos_after_icon(inner_rect, self.item_height).x,
            );
            if text_is_truncated {
                response.clone().on_hover_text(tooltip);
            }
        } else {
            // For narrow mode, center avatar within full rect (not inner_rect)
            render_avatar(
                self.ui.painter(),
                rect.center(),
                initial,
                self.dark,
                NARROW_AVATAR_SIZE,
            );
            response.clone().on_hover_text(tooltip);
        }

        add_selectable_accessibility(&response, self.ui, text_str, selected);
        response
    }

    pub(super) fn separator(&mut self) {
        self.ui.add_space(8.0);
        let available_width = self.ui.available_width();
        let rect = self
            .ui
            .allocate_exact_size(Vec2::new(available_width, 1.0), Sense::hover())
            .0;
        let line_rect = rect.shrink2(Vec2::new(ITEM_PADDING_X, 0.0));
        self.ui.painter().line_segment(
            [line_rect.left_center(), line_rect.right_center()],
            Stroke::new(1.0, if self.dark { gray::_800 } else { gray::_200 }),
        );
        self.ui.add_space(8.0);
    }
}

/// Shared sidebar shell - draws background and creates child UI
pub(super) fn render_sidebar<R>(
    ui: &mut Ui,
    width: Option<f32>,
    default_width: f32,
    background: Color32,
    top_padding: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let rect = ui.available_rect_before_wrap();
    // Use specified width, or available width capped at default (handles being inside a SidePanel)
    let width = width.unwrap_or_else(|| rect.width().min(default_width));
    let sidebar_rect = egui::Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
    ui.painter().rect_filled(sidebar_rect, 0.0, background);

    let mut child_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(sidebar_rect)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );

    let scroll_id = ui.auto_id_with("sidebar-scroll");
    crate::scroll::vertical()
        .id_salt(scroll_id)
        .auto_shrink(false)
        .show(&mut child_ui, |ui| {
            ui.add_space(top_padding);
            add_contents(ui)
        })
        .inner
}
