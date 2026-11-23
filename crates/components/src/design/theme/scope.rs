//! Context re-pointing: containers that change which roles component
//! overrides point at, mirroring the scoped custom properties of the design's
//! CSS (`.card`, `.blade`, `.dlg`, `.pop`, `.side`, `.palette`, `.ftool`).

use super::{Override, Role};

/// A container that re-points component overrides for everything inside it.
///
/// Scopes nest like CSS descendants: each scope applies its re-pointing on
/// top of the enclosing one. A [`ThemeScope::Card`] inside a blade, dialog or
/// popover (directly or further down) uses the overlay card mapping, which
/// sets wells, fields and rows back to `surface-panel` so they stand out on
/// the card's `surface-raised` background.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeScope {
    /// A card on a panel (`.card`): rows become `surface-raised`.
    Card,
    /// An inspector blade (`.blade`): rows become `surface-raised`.
    Blade,
    /// A modal dialog (`.dlg`): rows become `surface-raised`.
    Dialog,
    /// A popover, menu or select list (`.pop`): fields become `surface-panel`.
    Popover,
    /// The navigator sidebar (`.side`): fields become `surface-raised`.
    Sidebar,
    /// The command palette (`.palette`): fields and wells become `surface-panel`.
    CommandPalette,
    /// A floating toolbar (`.ftool`): fields and wells become `surface-panel`.
    FloatingToolbar,
}

use Override::{FieldBg, FieldBgHover, RowBg, RowBgHover, WellBg};
use Role::{SurfacePanel, SurfacePanelHover, SurfaceRaised, SurfaceRaisedHover};

const ROWS_RAISED: &[(Override, Role)] =
    &[(RowBg, SurfaceRaised), (RowBgHover, SurfaceRaisedHover)];
const FIELDS_RAISED: &[(Override, Role)] =
    &[(FieldBg, SurfaceRaised), (FieldBgHover, SurfaceRaisedHover)];
const FIELDS_PANEL: &[(Override, Role)] =
    &[(FieldBg, SurfacePanel), (FieldBgHover, SurfacePanelHover)];
const FIELDS_AND_WELL_PANEL: &[(Override, Role)] = &[
    (FieldBg, SurfacePanel),
    (FieldBgHover, SurfacePanelHover),
    (WellBg, SurfacePanel),
];
/// `.blade .card, .dlg .card, .pop .card`.
const OVERLAY_CARD: &[(Override, Role)] = &[
    (WellBg, SurfacePanel),
    (FieldBg, SurfacePanel),
    (FieldBgHover, SurfacePanelHover),
    (RowBg, SurfacePanel),
    (RowBgHover, SurfacePanelHover),
];

impl ThemeScope {
    /// Every scope.
    pub const ALL: [Self; 7] = [
        Self::Card,
        Self::Blade,
        Self::Dialog,
        Self::Popover,
        Self::Sidebar,
        Self::CommandPalette,
        Self::FloatingToolbar,
    ];

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    /// Whether cards nested inside this scope use the overlay card mapping.
    pub(super) const fn is_overlay(self) -> bool {
        matches!(self, Self::Blade | Self::Dialog | Self::Popover)
    }

    /// The overrides this scope re-points, given whether it sits inside an
    /// overlay.
    pub(super) const fn repoints(self, in_overlay: bool) -> &'static [(Override, Role)] {
        match self {
            Self::Card if in_overlay => OVERLAY_CARD,
            Self::Card | Self::Blade | Self::Dialog => ROWS_RAISED,
            Self::Popover => FIELDS_PANEL,
            Self::Sidebar => FIELDS_RAISED,
            Self::CommandPalette | Self::FloatingToolbar => FIELDS_AND_WELL_PANEL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::theme::Theme;

    #[test]
    fn blade_rows_use_raised_surface() {
        let blade = Theme::default().scoped(ThemeScope::Blade);
        assert_eq!(blade.components.row_bg, blade.surface.raised);
        assert_eq!(blade.components.row_bg_hover, blade.surface.raised_hover);
        assert_eq!(blade.components.field_bg, blade.surface.raised);
    }

    #[test]
    fn card_mapping_depends_on_enclosing_overlay() {
        let root = Theme::default();
        let card = root.scoped(ThemeScope::Card);
        assert_eq!(card.components.row_bg, card.surface.raised);
        assert_eq!(card.components.field_bg, card.surface.raised);
        assert_eq!(card.components.well_bg, card.control.fill);

        let blade_card = root.scoped(ThemeScope::Blade).scoped(ThemeScope::Card);
        assert_eq!(blade_card.components.field_bg, blade_card.surface.panel);
        assert_eq!(
            blade_card.components.field_bg_hover,
            blade_card.surface.panel_hover
        );
        assert_eq!(blade_card.components.row_bg, blade_card.surface.panel);
        assert_eq!(blade_card.components.well_bg, blade_card.surface.panel);

        // `.blade .card` matches every descendant card, however deep.
        let nested = blade_card.scoped(ThemeScope::Card);
        assert_eq!(nested.components.row_bg, nested.surface.panel);
    }

    #[test]
    fn nested_scopes_inherit_untouched_overrides() {
        // A popover inside a blade re-points fields but keeps the blade's rows.
        let popover = Theme::default()
            .scoped(ThemeScope::Blade)
            .scoped(ThemeScope::Popover);
        assert_eq!(popover.components.field_bg, popover.surface.panel);
        assert_eq!(popover.components.row_bg, popover.surface.raised);
    }

    #[test]
    fn scoped_themes_are_cached_and_keep_roles() {
        let root = Theme::default();
        let first = root.scoped(ThemeScope::Sidebar);
        let second = root.scoped(ThemeScope::Sidebar);
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(first.surface, root.surface);
        assert_eq!(first.components.field_bg, root.surface.raised);
    }

    #[test]
    fn scope_repointing_respects_user_role_overrides() {
        let theme = Theme::from_file_str(
            r#"
            name = "Brighter rows"
            extends = "harbour-dark"
            [roles]
            surface-raised = "sand-700"
            "#,
        )
        .unwrap();
        let blade = theme.scoped(ThemeScope::Blade);
        assert_eq!(blade.components.row_bg, crate::design::palette::SAND_700);
    }
}
