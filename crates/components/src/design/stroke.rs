//! Strokes: the few line widths and colours outlines and dividers use.
//!
//! Harbour separates surfaces by tone rather than borders, so strokes are
//! rare: dividers between regions, the borders of controls that have one,
//! and the elevation rings ([`super::elevation::ShadowRole::outline`]).
//! Build every stroke here so widths stay on the scale in [`super::size`].

use egui::epaint::CornerRadiusF32;
use egui::{Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind};

use super::size;
use super::theme::Theme;

/// A [`size::HAIRLINE`] stroke in `color`.
pub fn hairline(color: Color32) -> Stroke {
    Stroke::new(size::HAIRLINE, color)
}

/// A line between rows or regions: a hairline in `divider`.
pub fn divider(theme: &Theme) -> Stroke {
    hairline(theme.divider.base)
}

/// The quietest separation, inside surfaces: a hairline in `divider-subtle`.
pub fn subtle_divider(theme: &Theme) -> Stroke {
    hairline(theme.divider.subtle)
}

/// The hairline border of a field-like control: `control-border`.
pub fn control(theme: &Theme) -> Stroke {
    hairline(theme.control.border)
}

/// A CSS `1.5px` control border (checkboxes, radios, selection outlines) in
/// `color`, snapped to whole device pixels the way Chromium paints borders
/// ([`size::css_border_width`]): 1 point at 1×, 1.5 points at 2×.
pub fn control_border(color: Color32, pixels_per_point: f32) -> Stroke {
    Stroke::new(
        size::css_border_width(size::CONTROL_STROKE, pixels_per_point),
        color,
    )
}

/// A focus or selection ring around `rect` (CSS `box-shadow: 0 0 0 2px
/// surface, 0 0 0 4px ring`): a [`size::FOCUS_RING_GAP`] band in `surface`,
/// the colour around the control, then a [`size::FOCUS_RING`] band in `ring`
/// each with corners grown by its spread. Paint it before the control.
pub fn paint_focus_ring(
    painter: &Painter,
    rect: Rect,
    corner_radius: impl Into<CornerRadius>,
    surface: Color32,
    ring: Color32,
) {
    let corner_radius = corner_radius.into();
    let band = |inset: f32, width: f32, color: Color32| {
        let offset = inset + width / 2.0;
        painter.rect_stroke(
            rect.expand(offset),
            grow(corner_radius, offset),
            Stroke::new(width, color),
            StrokeKind::Middle,
        );
    };
    // Chromium paints the ring shadow from the control's edge and the gap
    // shadow over it, both clipped to outside the control, so a trace of
    // the ring shows along the edge.
    band(0.0, size::FOCUS_RING_EXTENT, ring);
    band(0.0, size::FOCUS_RING_GAP, surface);
}

/// `corner_radius` grown by `amount` points, as a CSS box-shadow spread grows
/// its corners.
pub fn grow(corner_radius: CornerRadius, amount: f32) -> CornerRadius {
    let mut grown = CornerRadiusF32::from(corner_radius);
    grown += amount;
    grown.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_use_the_scale_and_theme_roles() {
        let theme = Theme::default();
        assert_eq!(divider(&theme), Stroke::new(1.0, theme.divider.base));
        assert_eq!(
            subtle_divider(&theme),
            Stroke::new(1.0, theme.divider.subtle)
        );
        assert_eq!(control(&theme), Stroke::new(1.0, theme.control.border));
        assert_eq!(control_border(theme.accent.fill, 1.0).width, 1.0);
        assert_eq!(control_border(theme.accent.fill, 2.0).width, 1.5);
    }
}
