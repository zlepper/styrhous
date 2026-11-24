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
/// How many history blades show as peeks behind the active one.
const HISTORY_PEEKS: usize = 2;

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
    blade_title_with_actions, paint_blade_peek, paint_blade_peek_on,
};
// C19
pub use inspector_header::{InspectorHeader, InspectorHeaderResponse, InspectorStatus};
pub use interaction::HeaderAction;
pub use navigation::{BladeNavigator, BladeTransition};
pub use stack::{
    BladeDescription, BladeHeader, BladeLayer, BladePeek, BladeResponse, BladeStack, BladeStyle,
};

#[cfg(test)]
mod tests;
