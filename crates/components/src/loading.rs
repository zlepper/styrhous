//! Loading indicators (C01 "Spinner" and "Skeleton").
//!
//! Show a [`Spinner`] only after about 300 ms of waiting, so fast responses
//! never flash. Skeleton bars stand in for table rows on first load only:
//! never skeleton a surface that already has data — keep the stale data and
//! show a pill instead.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use egui::emath::GuiRounding as _;
use egui::{
    Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, WidgetInfo, WidgetType, vec2,
};

use crate::design::theme::ThemeExt;
use crate::design::{radius, size};

/// Seconds per spinner revolution.
const SPINNER_PERIOD: f64 = 0.8;
/// Line segments approximating the spinner's head arc.
const HEAD_SEGMENTS: usize = 16;

/// A spinner (`.spin`): a ring in `accent-tint-active` whose top quarter is
/// `accent-fill`, turning while shown.
#[must_use = "call `show` to add the spinner"]
pub struct Spinner {
    size: f32,
    colors: SpinnerColors,
    rotation: Option<f32>,
}

/// The colour treatment of a [`Spinner`].
#[derive(Clone, Copy, Debug, PartialEq)]
enum SpinnerColors {
    /// Accent on `accent-tint-active` (`.spin`).
    Accent,
    /// On a filled button (`spinner-on-fill-*`).
    OnFill,
    /// The text colour on `divider-strong`, for quiet progress in text.
    Neutral(Color32),
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner {
    /// A [`size::SPINNER`] spinner on a surface.
    pub fn new() -> Self {
        Self {
            size: size::SPINNER,
            colors: SpinnerColors::Accent,
            rotation: None,
        }
    }

    /// Set the diameter: [`size::SPINNER_SM`] beside 12 point status text,
    /// [`size::SPINNER`] by default, [`size::SPINNER_LG`] in empty states.
    pub fn size(mut self, diameter: f32) -> Self {
        self.size = diameter;
        self
    }

    /// The spinner on a filled (primary or danger) button: the
    /// `spinner-on-fill-*` colours.
    pub fn on_fill(mut self) -> Self {
        self.colors = SpinnerColors::OnFill;
        self
    }

    /// A quiet spinner in the surrounding text colour `color` on
    /// `divider-strong`, for progress set in muted text.
    pub fn neutral(mut self, color: Color32) -> Self {
        self.colors = SpinnerColors::Neutral(color);
        self
    }

    /// Hold the head at `radians` clockwise from the top instead of turning
    /// (design boards and snapshots).
    pub fn rotation(mut self, radians: f32) -> Self {
        self.rotation = Some(radians);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), Sense::hover());
        self.paint(ui, rect);
        response.widget_info(|| {
            WidgetInfo::labeled(WidgetType::ProgressIndicator, ui.is_enabled(), "Loading")
        });
        response
    }

    /// Paint the spinner centred in `rect` without allocating space or an
    /// accessibility node, for controls that describe their own progress.
    pub fn paint(&self, ui: &Ui, rect: Rect) {
        // Chromium snaps the box to device pixels before drawing its border.
        let rect = Rect::from_center_size(rect.center(), Vec2::splat(self.size))
            .round_to_pixels(ui.pixels_per_point());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let rotation = self.rotation.unwrap_or_else(|| {
            if ui.style().animation_time == 0.0 {
                return 0.0;
            }
            ui.ctx().request_repaint();
            let turns = ui.input(|input| input.time) / SPINNER_PERIOD;
            (turns.fract() as f32) * TAU
        });
        let theme = ui.tokens();
        let (track, head) = match self.colors {
            SpinnerColors::Accent => (theme.accent.tint_active, theme.accent.fill),
            SpinnerColors::OnFill => (
                theme.components.spinner_on_fill_track,
                theme.components.spinner_on_fill_head,
            ),
            SpinnerColors::Neutral(color) => (theme.divider.strong, color),
        };
        paint_ring(ui, rect.center(), self.size, rotation, track, head);
    }
}

/// The ring of a spinner `diameter` points wide: a full track and, like a CSS
/// `border-top` on a circle, a head spanning the quarter between the top
/// corners' diagonals, cut radially.
fn paint_ring(ui: &Ui, center: Pos2, diameter: f32, rotation: f32, track: Color32, head: Color32) {
    let ring_radius = (diameter - size::SPINNER_STROKE) / 2.0;
    let painter = ui.painter();
    // egui strokes circles outside their radius.
    painter.circle_stroke(
        center,
        ring_radius - size::SPINNER_STROKE / 2.0,
        Stroke::new(size::SPINNER_STROKE, track),
    );
    // Screen angles run clockwise from +x; the top is at -π/2.
    let start = -FRAC_PI_2 - FRAC_PI_4 + rotation;
    let points = (0..=HEAD_SEGMENTS)
        .map(|step| {
            let angle = start + FRAC_PI_2 * step as f32 / HEAD_SEGMENTS as f32;
            center + ring_radius * vec2(angle.cos(), angle.sin())
        })
        .collect();
    painter.add(Shape::line(points, Stroke::new(size::SPINNER_STROKE, head)));
}

/// A skeleton bar (`.skel`) `width` points wide, standing in for a line of
/// text that is still loading.
pub fn skeleton_bar(ui: &mut Ui, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, size::SKELETON_BAR), Sense::hover());
    ui.painter()
        .rect_filled(rect, radius::FULL, ui.tokens().components.well_bg);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::epaint::{CircleShape, PathShape};
    use egui_kittest::Harness;

    fn harness(spinner: impl Fn() -> Spinner + 'static) -> Harness<'static> {
        let mut harness = Harness::builder().build_ui(move |ui| {
            spinner().show(ui);
        });
        crate::test_support::setup_egui(&mut harness);
        harness
    }

    #[test]
    fn spinners_turn_by_repainting_and_hold_still_without_animation() {
        let mut turning = harness(Spinner::new);
        // egui_kittest turns animations off; turn them back on.
        turning
            .ctx
            .global_style_mut(|style| style.animation_time = egui::Style::default().animation_time);
        assert!(
            turning.run_ok().is_none(),
            "a turning spinner keeps repainting"
        );

        let mut still = harness(Spinner::new);
        still
            .ctx
            .global_style_mut(|style| style.animation_time = 0.0);
        assert!(still.run_ok().is_some(), "reduced motion stops the spinner");

        let mut held = harness(|| Spinner::new().rotation(0.0));
        assert!(held.run_ok().is_some(), "a held spinner never repaints");
    }

    #[test]
    fn spinners_on_fills_use_the_on_fill_colours() {
        let mut harness = harness(|| Spinner::new().on_fill().rotation(0.0));
        crate::test_support::install_distinct_role_theme(&harness.ctx);
        harness.run();
        let theme = harness.ctx.tokens();
        let output = harness.output();
        let track = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Circle(CircleShape { stroke, .. }) => Some(stroke.color),
                _ => None,
            });
        let head = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::Shape::Path(PathShape { stroke, .. }) => Some(stroke.color.clone()),
                _ => None,
            });
        assert_eq!(track, Some(theme.components.spinner_on_fill_track));
        assert_eq!(
            head,
            Some(egui::epaint::ColorMode::Solid(
                theme.components.spinner_on_fill_head
            ))
        );
    }
}
