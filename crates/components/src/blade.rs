//! Reusable right-side overlay blades with optional navigation history.

use crate::design::theme::{self, ThemeExt, ThemeScope};
use crate::design::{size, space};
use egui::{Id, Order, Pos2, Rect, Sense, Ui, WidgetInfo, WidgetType};

/// The preferred width of every foreground and history blade ([`size::BLADE`]).
pub const BLADE_WIDTH: f32 = size::BLADE;
const WIDTH: f32 = BLADE_WIDTH;
const INSET: f32 = space::BLADE_INSET;

/// The width available to a blade's body inside its padding.
const fn content_width(blade_width: f32) -> f32 {
    (blade_width - (chrome::BODY_PADDING.left + chrome::BODY_PADDING.right) as f32).max(0.0)
}
const TRANSITION_DURATION: f32 = 0.25;
const HISTORY_SCALES: [f32; 2] = [0.9, 0.8];
/// Horizontal recession is proportional to the scale of every earlier blade,
/// matching the visual spacing of the inspector implementation this replaces.
const HISTORY_X_TRANSLATIONS: [f32; 2] = [
    1.0 / 3.0,
    (1.0 / 3.0) * (1.0 + HISTORY_SCALES[1] / HISTORY_SCALES[0]),
];

mod chrome;
// C19
mod inspector_header;
mod interaction;
mod navigation;
mod rendering;
mod stack;
mod transforms;

pub use chrome::{
    BLADE_BODY_FOOTER, BladeFrame, BladeNavigation, blade_body_footer, blade_footer, blade_title,
    blade_title_with_actions, paint_blade_peek,
};
// C19
pub use inspector_header::{InspectorHeader, InspectorHeaderResponse, InspectorStatus};
pub use navigation::{BladeNavigator, BladeTransition};
pub use stack::{BladeLayer, BladeResponse, BladeStack};

#[cfg(test)]
mod tests;
