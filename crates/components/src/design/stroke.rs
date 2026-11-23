//! Strokes: the few line widths and colours outlines and dividers use.
//!
//! Harbour separates surfaces by tone rather than borders, so strokes are
//! rare: dividers between regions, the borders of controls that have one,
//! and the elevation rings ([`super::elevation::ShadowRole::outline`]).
//! Build every stroke here so widths stay on the scale in [`super::size`].

use egui::{Color32, Stroke};

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
