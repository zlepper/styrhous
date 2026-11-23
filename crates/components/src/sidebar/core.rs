use super::*;
use crate::tooltip::TooltipExt;

// The settings SVG has generous view-box padding. Allocate a larger image so its visible cog
// matches the visual weight of the 21px control it replaced.
const NARROW_FOOTER_BUTTON_ICON_SIZE: f32 = 28.0;

pub(super) struct SidebarContentCore<'a> {
    pub(super) ui: &'a mut Ui,
    pub(super) id: Id,
    pub(super) item_height: f32,
    pub(super) show_text: bool,
}

impl<'a> SidebarContentCore<'a> {
    pub(super) fn wide(ui: &'a mut Ui) -> Self {
        let id = ui.auto_id_with("wide-sidebar");
        Self {
            ui,
            id,
            item_height: WIDE_ITEM_HEIGHT,
            show_text: true,
        }
    }

    pub(super) fn narrow(ui: &'a mut Ui) -> Self {
        let id = ui.auto_id_with("narrow-sidebar");
        Self {
            ui,
            id,
            item_height: NARROW_ITEM_HEIGHT,
            show_text: false,
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
        self.item_with_tooltip(text, icon, selected, None)
    }

    pub(super) fn item_with_tooltip(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        selected: bool,
        tooltip: Option<&str>,
    ) -> Response {
        self.show_icon_item(text, icon, selected, tooltip, false, ICON_SIZE)
    }

    pub(super) fn button_with_tooltip(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        tooltip: &str,
    ) -> Response {
        self.show_icon_item(
            text,
            icon,
            false,
            Some(tooltip),
            true,
            NARROW_FOOTER_BUTTON_ICON_SIZE,
        )
    }

    fn show_icon_item(
        &mut self,
        text: impl Into<WidgetText>,
        icon: Image<'_>,
        selected: bool,
        tooltip: Option<&str>,
        is_button: bool,
        icon_size: f32,
    ) -> Response {
        let text = text.into();
        let text_str = text.text();
        let tooltip = tooltip.unwrap_or(text_str);

        let (rect, inner_rect, response) = allocate_item(self.ui, self.item_height);
        let colors = ItemColors::navigation(&self.ui.tokens(), selected, response.hovered());

        let background_rect = if self.show_text {
            inner_rect
        } else {
            inner_rect.shrink2(Vec2::new(0.0, 2.0))
        };
        draw_background(self.ui.painter(), background_rect, colors.background);

        if self.show_text {
            render_icon(
                self.ui,
                icon_rect_wide(inner_rect, self.item_height, icon_size),
                icon,
                colors.icon,
                icon_size,
            );
            let text_is_truncated = render_text(
                self.ui.painter(),
                text_pos_after_icon(inner_rect, self.item_height),
                text_str,
                colors.text,
                inner_rect.right() - text_pos_after_icon(inner_rect, self.item_height).x,
            );
            if text_is_truncated {
                response.clone().tooltip(tooltip);
            }
        } else {
            // For narrow mode, center icon within full rect (not inner_rect)
            render_icon(
                self.ui,
                icon_rect_centered(rect, icon_size),
                icon,
                colors.icon,
                icon_size,
            );
            response.clone().tooltip(tooltip);
        }

        if is_button {
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, self.ui.is_enabled(), text_str)
            });
        } else {
            add_selectable_accessibility(&response, self.ui, text_str, selected);
        }
        response
    }

    pub(super) fn avatar_item(
        &mut self,
        text: impl Into<WidgetText>,
        initial: &str,
        selected: bool,
    ) -> Response {
        self.avatar_item_with_tooltip(text, initial, selected, None, None)
    }

    pub(super) fn avatar_item_with_tooltip(
        &mut self,
        text: impl Into<WidgetText>,
        initial: &str,
        selected: bool,
        tooltip: Option<&str>,
        status: Option<crate::design::theme::Tone>,
    ) -> Response {
        let text = text.into();
        let text_str = text.text();
        let tooltip = tooltip.unwrap_or(text_str);

        let (rect, inner_rect, response) = allocate_item(self.ui, self.item_height);
        let colors = ItemColors::navigation(&self.ui.tokens(), selected, response.hovered());

        if self.show_text {
            draw_background(self.ui.painter(), inner_rect, colors.background);
            let avatar_x = CHEVRON_SIZE + CHEVRON_GAP;
            let avatar_center =
                inner_rect.min + Vec2::new(avatar_x + ICON_SIZE / 2.0, self.item_height / 2.0);
            paint_avatar(self.ui, avatar_center, initial, ICON_SIZE);
            let text_is_truncated = render_text(
                self.ui.painter(),
                text_pos_after_icon(inner_rect, self.item_height),
                text_str,
                colors.text,
                inner_rect.right() - text_pos_after_icon(inner_rect, self.item_height).x,
            );
            if text_is_truncated {
                response.clone().tooltip(tooltip);
            }
        } else {
            // The rail shows the cluster avatar itself (`.av`): its selected
            // ring and hover brightening replace the item background.
            crate::Avatar::new(initial)
                .identity()
                .status(status)
                .selected(selected)
                .hovered(response.hovered())
                .paint(self.ui, rect.center());
            response.clone().tooltip(tooltip);
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
            stroke::hairline(self.ui.tokens().nav.divider),
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
    render_sidebar_with_footer(
        ui,
        SidebarLayout {
            width,
            default_width,
            background,
            top_padding,
        },
        0.0,
        add_contents,
        |_| {},
    )
}

pub(super) fn render_sidebar_with_footer<R>(
    ui: &mut Ui,
    layout: SidebarLayout,
    footer_height: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
    add_footer: impl FnOnce(&mut Ui),
) -> R {
    theme::scope(ui, ThemeScope::Sidebar, |ui| {
        render_sidebar_frame(ui, layout, footer_height, add_contents, add_footer)
    })
}

fn render_sidebar_frame<R>(
    ui: &mut Ui,
    layout: SidebarLayout,
    footer_height: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
    add_footer: impl FnOnce(&mut Ui),
) -> R {
    let rect = ui.available_rect_before_wrap();
    // Use specified width, or available width capped at default (handles being inside a SidePanel)
    let width = layout
        .width
        .unwrap_or_else(|| rect.width().min(layout.default_width));
    let sidebar_rect = egui::Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
    ui.painter()
        .rect_filled(sidebar_rect, 0.0, layout.background);

    let mut child_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                sidebar_rect.min,
                egui::pos2(sidebar_rect.right(), sidebar_rect.bottom() - footer_height),
            ))
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );

    let scroll_id = ui.auto_id_with("sidebar-scroll");
    let inner = crate::scroll::vertical()
        .id_salt(scroll_id)
        .auto_shrink(false)
        .show(&mut child_ui, |ui| {
            ui.add_space(layout.top_padding);
            add_contents(ui)
        })
        .inner;

    if footer_height > 0.0 {
        let footer_rect = egui::Rect::from_min_max(
            egui::pos2(sidebar_rect.left(), sidebar_rect.bottom() - footer_height),
            sidebar_rect.right_bottom(),
        );
        let mut footer_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(footer_rect)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        add_footer(&mut footer_ui);
    }

    inner
}

pub(super) struct SidebarLayout {
    pub(super) width: Option<f32>,
    pub(super) default_width: f32,
    pub(super) background: Color32,
    pub(super) top_padding: f32,
}

/// A small identity avatar beside a wide sidebar item's text; its initials
/// use the 12 point meta role so they fit the 16 point square.
fn paint_avatar(ui: &Ui, center: egui::Pos2, text: &str, size: f32) {
    crate::Avatar::new(text)
        .identity()
        .size(size)
        .font(type_scale::META.with_weight(type_scale::Weight::SemiBold))
        .paint(ui, center);
}
