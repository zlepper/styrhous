//! Spacing scale.
//!
//! Everything sits on a 4 point grid; component internals may use the 2 and
//! 6 point steps (group padding, icon gaps, label-to-control gaps).

pub const S2: f32 = 2.0;
pub const S4: f32 = 4.0;
pub const S6: f32 = 6.0;
pub const S8: f32 = 8.0;
pub const S12: f32 = 12.0;
pub const S16: f32 = 16.0;
pub const S20: f32 = 20.0;
pub const S24: f32 = 24.0;
pub const S32: f32 = 32.0;
pub const S40: f32 = 40.0;
pub const S48: f32 = 48.0;

/// Gap between a field label and its control.
pub const LABEL_TO_CONTROL: f32 = S6;
/// Inset of a blade from the window edge.
pub const BLADE_INSET: f32 = S8;

// C12
/// Gap between a status icon or kind square and its words in callouts,
/// banners and sidebar items (`gap: 10px`).
pub const ICON_TEXT: f32 = 10.0;
