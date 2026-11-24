//! Corner radius scale.
//!
//! Harbour radii are `u8` points, which convert into [`egui::CornerRadius`]
//! wherever egui takes `impl Into<CornerRadius>` (frames, painter shapes).
//! Every control is fully rounded with [`FULL`].

/// Link buttons (`.btn-link`), which carry their focus ring on these corners.
pub const LINK: u8 = 4;
/// Keycaps.
pub const KEY: u8 = 6;
/// Mini buttons inside fields (C04 `.mini`, `.rx`).
pub const MINI: u8 = 8;
/// Menu items.
pub const MENU_ITEM: u8 = 10;
/// Option rows of selects (C05 `.opt`).
pub const OPTION: u8 = MENU_ITEM;
/// Table row tiles.
pub const ROW: u8 = 12;
/// Selection checkboxes (16 point square).
pub const CHECKBOX: u8 = 5;
/// 36 point icon tiles (`.tile`); large 40 point tiles use [`ROW`].
pub const TILE: u8 = MENU_ITEM;
/// The knob of a switch (C06 `.sw::after`).
pub const SWITCH_KNOB: u8 = 4;
/// Status dots (`.dot`).
pub const STATUS_DOT: u8 = 3;
/// Container state squares (`.cs`).
pub const CONTAINER_SQUARE: u8 = 2;
/// Cluster avatars (`.av`).
pub const AVATAR: u8 = ROW;
/// The status square on an avatar (`.av .sd`), outer edge.
pub const AVATAR_STATUS: u8 = 3;
/// Popovers, menus and selects.
pub const POPOVER: u8 = 14;
/// Text areas (C04 `.ta`).
pub const TEXT_AREA: u8 = POPOVER;
/// Radio cards (C06 `.rcard`).
pub const RADIO_CARD: u8 = CARD;
/// Cards.
pub const CARD: u8 = 16;
/// Overlays: blades, dialogs and the command palette.
pub const OVERLAY: u8 = 22;
/// Pill shape used by every control (CSS `999px`).
///
/// egui clamps a corner radius to half of the shape's shorter side, so the
/// largest representable radius always produces a full pill.
pub const FULL: u8 = u8::MAX;

// C11
/// Callouts (`.callout`).
pub const CALLOUT: u8 = CARD;

// C12
/// Toasts (`.toast`).
pub const TOAST: u8 = CARD;

// C09
/// The floating sidebar (`.side`).
pub const SIDEBAR: u8 = 20;
/// The cluster card (`.ccard`) and item boxes in the sidebar.
pub const CLUSTER_CARD: u8 = POPOVER;
/// Sidebar items (`.sv`).
pub const SIDE_ITEM: u8 = ROW;
/// The kind colour square of a sidebar item (`.kc`).
pub const KIND_SWATCH: u8 = 3;

// C07
/// Tooltips (`.tip`).
pub const TOOLTIP: u8 = MENU_ITEM;

// C13 · data display
/// Hovered copyable values (`control-fill` pill behind the value).
pub const COPYABLE: u8 = MINI;
/// Code blocks (`.codeblk`).
pub const CODE_BLOCK: u8 = ROW;
/// Small icon tiles (`.tile.sm`).
pub const TILE_SM: u8 = MINI;
/// Large icon tiles (`.tile.lg`).
pub const TILE_LG: u8 = ROW;

// C15 · code surface
/// Search matches on a code surface (`.ln .m`, `.ln .ma`).
pub const CODE_MATCH: u8 = 3;
/// Selected text on a code surface (`.ln .sel`).
pub const CODE_SELECTION: u8 = 2;

// C17 · grouped table
/// The problem ticks track and its thumb (`.sticks`, `.sth`).
pub const PROBLEM_TRACK: u8 = 3;
/// A problem tick (`.tk`).
pub const PROBLEM_TICK: u8 = 2;

// C19 · readouts
/// Readout tiles (`.ro`).
pub const READOUT: u8 = POPOVER;
/// Section rail items (`.si`).
pub const SECTION_ITEM: u8 = MENU_ITEM;

// C20 · log viewer
/// A volume bar (`.vb`).
pub const VOLUME_BAR: u8 = 2;
/// The outline of a volume strip's visible window (`.vwin`).
pub const VOLUME_WINDOW: u8 = MENU_ITEM;
/// The active search match of a log line (`.ll .lma`).
pub const LOG_MATCH: u8 = 4;

// C18 · cluster switcher
/// Cluster tiles (`.ctile`).
pub const CLUSTER_TILE: u8 = POPOVER;

// A04 · usage cells
/// The bar of a table's used / allocatable cell.
pub const USAGE_BAR: u8 = 2;
