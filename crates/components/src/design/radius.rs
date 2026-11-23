//! Corner radius scale.
//!
//! Harbour radii are `u8` points, which convert into [`egui::CornerRadius`]
//! wherever egui takes `impl Into<CornerRadius>` (frames, painter shapes).
//! Every control is fully rounded with [`FULL`].

/// Keycaps.
pub const KEY: u8 = 6;
/// Menu items.
pub const MENU_ITEM: u8 = 10;
/// Table row tiles.
pub const ROW: u8 = 12;
/// Selection checkboxes (16 point square).
pub const CHECKBOX: u8 = 5;
/// 36 point icon tiles (`.tile`); large 40 point tiles use [`ROW`].
pub const TILE: u8 = MENU_ITEM;
/// Popovers, menus and selects.
pub const POPOVER: u8 = 14;
/// Cards.
pub const CARD: u8 = 16;
/// Overlays: blades, dialogs and the command palette.
pub const OVERLAY: u8 = 22;
/// Pill shape used by every control (CSS `999px`).
///
/// egui clamps a corner radius to half of the shape's shorter side, so the
/// largest representable radius always produces a full pill.
pub const FULL: u8 = u8::MAX;
