//! Frames of elevated surfaces: raised popovers and toasts, overlay blades
//! and dialogs.
//!
//! Each frame carries its elevation's fill, hairline ring
//! ([`ShadowRole::outline`](crate::design::elevation::ShadowRole::outline)),
//! drop shadow and radius. Add the inner margin at the call site and show
//! the contents in the matching [`ThemeScope`](crate::design::theme::ThemeScope).

use egui::{Frame, Margin};

use crate::design::theme::Theme;
use crate::design::{radius, space};

/// A raised surface (`.pop`, `.toast`, `.ftool`): `surface-raised` with the
/// raised shadow, its ring and the popover radius.
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

/// The frame of menus and popovers anchored to a control (`.pop`): a
/// [`raised_frame`] with the popover's 4 point padding. Show the popover's
/// contents in [`ThemeScope::Popover`](crate::design::theme::ThemeScope::Popover).
pub fn popover_frame(theme: &Theme) -> Frame {
    raised_frame(theme).inner_margin(Margin::same(space::S4 as i8))
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
