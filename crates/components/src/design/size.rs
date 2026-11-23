//! Fixed component and layout dimensions.

/// Mini control height.
pub const CONTROL_MINI: f32 = 24.0;
/// Small control height.
pub const CONTROL_SM: f32 = 28.0;
/// Medium (default) control height.
pub const CONTROL_MD: f32 = 32.0;
/// Large control height.
pub const CONTROL_LG: f32 = 36.0;

/// Padding of stock egui buttons, set as egui's `button_padding` by the
/// theme: the small button (C02 `.btn.sm`, `padding: 0 11px`), 28 points
/// tall around a 16 point UI text row.
pub const BUTTON_PADDING_SM: egui::Vec2 = egui::vec2(11.0, (CONTROL_SM - 16.0) / 2.0);
/// Margin of stock egui text edits: the small input (C04 `.inp.sm`,
/// `padding: 0 12px`), 28 points tall around a 16 point UI text row. egui has
/// no style setting for it; pass it to `TextEdit::margin`.
pub const FIELD_MARGIN_SM: egui::Margin = egui::Margin::symmetric(12, 6);

/// Table header height.
pub const TABLE_HEADER: f32 = 36.0;
/// Height of a table row tile.
pub const ROW_TILE: f32 = 36.0;
/// Gap between table row tiles.
pub const ROW_GAP: f32 = 4.0;
/// Vertical distance between consecutive table rows (tile plus gap).
pub const ROW_PITCH: f32 = ROW_TILE + ROW_GAP;

/// Page and window header bars.
pub const HEADER_BAR: f32 = 56.0;
/// Toolbars.
pub const TOOLBAR: f32 = 48.0;
/// Status bars.
pub const STATUS_BAR: f32 = 28.0;

/// Width of the workspace navigation rail.
pub const RAIL_WIDTH: f32 = 64.0;
/// Width of the workspace navigator panel.
pub const NAVIGATOR_WIDTH: f32 = 240.0;

/// Hairline strokes: dividers and borders.
pub const HAIRLINE: f32 = 1.0;
/// Control strokes: checkbox and radio borders, focus and selection rings.
pub const CONTROL_STROKE: f32 = 1.5;
/// Checkbox and radio size.
pub const CHECKBOX: f32 = 16.0;
/// Dot of a selected radio.
pub const RADIO_DOT: f32 = 6.0;
/// Keycap height (`.kbd`).
pub const KEY_HEIGHT: f32 = 18.0;
/// Horizontal keycap padding.
pub const KEY_PADDING_X: f32 = 5.0;
/// Icon tile (`.tile`).
pub const TILE: f32 = 36.0;
/// Default icon size.
pub const ICON: f32 = 16.0;
/// Large icon size (`.ic.s20`).
pub const ICON_LG: f32 = 20.0;

/// The width Chromium paints a CSS `border` of `width` points at
/// `pixels_per_point`: border widths are snapped down to whole device pixels
/// (at least one), so the design's 1.5 px control borders paint 1 px wide on
/// a 1× display and 3 px on a 2× display. Box-shadow rings (`inset 0 0 0
/// 1.5px`) are not snapped; draw those at their design width.
pub fn css_border_width(width: f32, pixels_per_point: f32) -> f32 {
    (width * pixels_per_point).floor().max(1.0) / pixels_per_point
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borders_snap_to_whole_device_pixels() {
        assert_eq!(css_border_width(CONTROL_STROKE, 1.0), 1.0);
        assert_eq!(css_border_width(CONTROL_STROKE, 2.0), 1.5);
        assert_eq!(css_border_width(HAIRLINE, 1.0), 1.0);
        assert_eq!(css_border_width(0.5, 1.0), 1.0);
        assert_eq!(css_border_width(CONTROL_STROKE, 1.5), 4.0 / 3.0);
    }
}
