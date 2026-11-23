//! Frames of elevated surfaces: raised popovers and toasts, overlay blades
//! and dialogs.
//!
//! Each frame carries its elevation's fill, hairline ring
//! ([`ShadowRole::outline`](crate::design::elevation::ShadowRole::outline)),
//! drop shadow and radius. Add the inner margin at the call site and show
//! the contents in the matching [`ThemeScope`](crate::design::theme::ThemeScope).

use egui::{Frame, Margin};

use crate::design::elevation::ShadowRole;
use crate::design::theme::Theme;
use crate::design::{radius, size, space};

/// A raised surface (`.pop`, `.toast`, `.ftool`): `surface-raised` with the
/// raised shadow, its ring and the popover radius. The frame's own shadow is
/// egui's linear approximation; show it with [`show_elevated`] for the CSS
/// Gaussian.
pub fn raised_frame(theme: &Theme) -> Frame {
    let elevation = theme.shadow.raised;
    Frame::new()
        .fill(theme.surface.raised)
        .stroke(elevation.outline())
        .shadow(elevation.shadow)
        .corner_radius(radius::POPOVER)
}

/// An overlay surface (`.blade`, `.dlg`): `surface-panel` with the overlay
/// shadow, its ring and the overlay radius.
pub fn overlay_frame(theme: &Theme) -> Frame {
    let elevation = theme.shadow.overlay;
    Frame::new()
        .fill(theme.surface.panel)
        .stroke(elevation.outline())
        .shadow(elevation.shadow)
        .corner_radius(radius::OVERLAY)
}

/// The padding of a popover (`.pop`): rows that run to its edges (a search
/// row, a footer, a divider) extend this far past its content.
pub const POPOVER_PADDING: f32 = space::S4;

/// The frame of menus and popovers anchored to a control (`.pop`): a
/// [`raised_frame`] with the popover's 4 point padding. Show the popover's
/// contents in [`ThemeScope::Popover`](crate::design::theme::ThemeScope::Popover).
pub fn popover_frame(theme: &Theme) -> Frame {
    raised_frame(theme).inner_margin(Margin::same(POPOVER_PADDING as i8))
}

/// Show `frame` with `elevation` painted behind it as CSS paints a
/// `box-shadow` (`0 24px 60px …, 0 0 0 1px …`): the hairline ring outside
/// the box, the Gaussian-blurred drop shadow over it ([`ShadowRole::shape`]),
/// then the frame's fill, in place of the frame's own stroke and egui's
/// linear shadow. The box is exactly as wide as the frame's content and
/// margins.
pub fn show_elevated<R>(
    ui: &mut egui::Ui,
    frame: Frame,
    elevation: ShadowRole,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let slot = ui.painter().add(egui::Shape::Noop);
    let corner_radius = frame.corner_radius;
    let inner = frame
        .shadow(egui::Shadow::NONE)
        .stroke(egui::Stroke::NONE)
        .show(ui, add_contents);
    let shape = elevation.shape(
        ui.ctx(),
        ui.pixels_per_point(),
        inner.response.rect,
        corner_radius,
    );
    ui.painter().set(slot, shape);
    inner
}

/// Show `add_contents` in a popover anchored to a control whose width it
/// matches (a select, a menu): the [`popover_frame`] surface, with its
/// hairline ring and drop shadow painted under it and outside its box in
/// CSS order (`0 24px 60px …, 0 0 0 1px …`: the shadow over the ring), so
/// the popover is exactly as wide as asked. Show it inside a popup whose own
/// frame is [`Frame::NONE`].
pub fn show_anchored_popover<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    show_elevated(ui, popover_frame(theme), theme.shadow.raised, add_contents)
}

/// The frame egui draws around a submenu (`Frame::menu`), whose popup this
/// crate cannot frame itself: [`popover_frame`] without its ring, which
/// [`paint_popover_ring`] draws outside the box.
pub(crate) fn submenu_frame_style(style: &mut egui::Style, theme: &Theme) {
    style.spacing.menu_margin = Margin::same(POPOVER_PADDING as i8);
    style.visuals.menu_corner_radius = radius::POPOVER.into();
    style.visuals.window_fill = theme.surface.raised;
    style.visuals.window_stroke = egui::Stroke::NONE;
    style.visuals.popup_shadow = theme.shadow.raised.shadow;
}

/// The raised hairline ring around a submenu shown in `response`, outside
/// its box like the design's `0 0 0 1px` box-shadow.
pub(crate) fn paint_popover_ring(ctx: &egui::Context, theme: &Theme, response: &egui::Response) {
    ctx.layer_painter(response.layer_id).rect_stroke(
        response.rect,
        radius::POPOVER,
        theme.shadow.raised.outline(),
        egui::StrokeKind::Outside,
    );
}

/// The sunken footer of an overlay (`.dlg-f`, `.blade-f`) at `rect`, the
/// bottom of a dialog or blade: `surface-sunken` with the overlay's bottom
/// corners under a `divider` hairline.
pub(crate) fn sunken_footer(theme: &Theme, rect: egui::Rect) -> egui::Shape {
    let corners = egui::CornerRadius {
        sw: radius::OVERLAY,
        se: radius::OVERLAY,
        ..egui::CornerRadius::ZERO
    };
    let divider = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), size::HAIRLINE));
    egui::Shape::Vec(vec![
        egui::epaint::RectShape::filled(rect, corners, theme.surface.sunken).into(),
        egui::epaint::RectShape::filled(divider, 0, theme.divider.base).into(),
    ])
}

/// A tooltip (C07 `.tip`): `tooltip-bg` with the tooltip shadow, no ring,
/// the tooltip radius and its 10 × 6 point padding.
pub fn tooltip_frame(theme: &Theme) -> Frame {
    Frame::new()
        .fill(theme.components.tooltip_bg)
        .shadow(theme.shadow.tooltip.shadow)
        .corner_radius(radius::TOOLTIP)
        .inner_margin(Margin::symmetric(
            size::TOOLTIP_PADDING.x as i8,
            size::TOOLTIP_PADDING.y as i8,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_carry_their_elevation() {
        let theme = Theme::default();
        let raised = raised_frame(&theme);
        assert_eq!(raised.fill, theme.surface.raised);
        assert_eq!(raised.stroke, theme.shadow.raised.outline());
        assert_eq!(raised.shadow, theme.shadow.raised.shadow);
        assert_eq!(raised.corner_radius, radius::POPOVER.into());

        let overlay = overlay_frame(&theme);
        assert_eq!(overlay.fill, theme.surface.panel);
        assert_eq!(overlay.stroke, theme.shadow.overlay.outline());
        assert_eq!(overlay.shadow, theme.shadow.overlay.shadow);
        assert_eq!(overlay.corner_radius, radius::OVERLAY.into());

        assert_eq!(popover_frame(&theme).inner_margin, Margin::same(4));
    }
}
