//! Reusable right-side overlay blades with optional navigation history.

use crate::colors::{WHITE, gray};
use crate::design::spacing;
use crate::icons;
use crate::{ButtonSize, ButtonVariant, TailwindButton};
use egui::{Color32, Id, Order, Pos2, Rect, Sense, Ui, WidgetInfo, WidgetType};
use egui_extras::{Size, StripBuilder};

/// The fixed width used by every foreground and history blade.
pub const BLADE_WIDTH: f32 = 744.0;
const WIDTH: f32 = BLADE_WIDTH;
const INSET: f32 = 8.0;
const PADDING: i8 = spacing::XL as i8;
const FRAME_STROKE_WIDTH: f32 = 1.0;
const CONTENT_WIDTH: f32 = WIDTH - spacing::XL * 2.0 - FRAME_STROKE_WIDTH * 2.0;
const TRANSITION_DURATION: f32 = 0.25;
const HISTORY_SCALES: [f32; 2] = [0.9, 0.8];
/// Horizontal recession is proportional to the scale of every earlier blade,
/// matching the visual spacing of the inspector implementation this replaces.
const HISTORY_X_TRANSLATIONS: [f32; 2] = [
    1.0 / 3.0,
    (1.0 / 3.0) * (1.0 + HISTORY_SCALES[1] / HISTORY_SCALES[0]),
];

mod interaction;
mod navigation;
mod rendering;
mod stack;
mod transforms;

pub use navigation::{BladeNavigator, BladeTransition};
pub use stack::{BladeLayer, BladeResponse, BladeStack};

#[cfg(test)]
mod tests;
