//! Status lamps and words (C01 "Status text" and container squares).
//!
//! Status is always a lamp plus words, never colour alone: a [`StatusLabel`]
//! in tables and property grids, a [`crate::Pill`] in headers. Lamps are small
//! rounded squares in a [`Tone`]'s fill; lit tones (success, warning, danger,
//! accent) glow. Container state squares are the same lamp, smaller-cornered
//! and unlit, one per container.

use crate::tooltip::TooltipExt;
use std::sync::Arc;

use egui::emath::GuiRounding as _;
use egui::text::LayoutJob;
use egui::{Color32, Painter, Rect, Response, Sense, Ui, Vec2, WidgetInfo, WidgetType, pos2, vec2};

use crate::design::theme::{Theme, ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole};
use crate::design::{elevation, radius, size};

/// Paint a status lamp (`.dot`) filling `rect` (normally
/// [`size::STATUS_DOT`] square) in `tone`, with the tone's glow when lit.
pub fn paint_status_dot(painter: &Painter, theme: &Theme, rect: Rect, tone: Tone) {
    paint_dot(painter, theme, rect, tone, None);
}

// C20
/// [`paint_status_dot`] ringed by `halo` (`.dot.ring`: `0 0 0 3px halo,
/// 0 0 8px glow`): the glow, the ring over it, then the lamp.
pub(crate) fn paint_ringed_status_dot(
    painter: &Painter,
    theme: &Theme,
    rect: Rect,
    tone: Tone,
    halo: Color32,
) {
    paint_dot(painter, theme, rect, tone, Some(halo));
}

fn paint_dot(painter: &Painter, theme: &Theme, rect: Rect, tone: Tone, halo: Option<Color32>) {
    // Chromium snaps boxes to device pixels; the glow must centre on the snapped lamp.
    let rect = rect.round_to_pixels(painter.pixels_per_point());
    let colors = theme.tone(tone);
    let corner = f32::from(radius::STATUS_DOT);
    if let Some(glow) = colors.glow {
        elevation::paint_glow(painter, rect, corner, glow, elevation::GLOW_BLUR);
    }
    if let Some(halo) = halo {
        painter.rect_filled(rect.expand(size::LAMP_RING), corner + size::LAMP_RING, halo);
    }
    painter.rect_filled(rect, radius::STATUS_DOT, colors.fill);
}

/// The rect of a status lamp whose left edge is at `left`, vertically
/// centred on `center_y`: the lamp of a [`StatusLabel`], a pill or a
/// select option.
pub(crate) fn lamp_rect(left: f32, center_y: f32) -> Rect {
    Rect::from_min_size(
        pos2(left, center_y - size::STATUS_DOT / 2.0),
        Vec2::splat(size::STATUS_DOT),
    )
}

/// A status lamp on its own, [`size::STATUS_DOT`] square, read as `label`
/// and showing it as a tooltip. Prefer a [`StatusLabel`]: a lamp alone
/// relies on colour.
pub fn status_dot(ui: &mut Ui, tone: Tone, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size::STATUS_DOT), Sense::hover());
    paint_status_dot(ui.painter(), &ui.tokens(), rect, tone);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Image, ui.is_enabled(), label));
    response.tooltip(label)
}

/// Status words with their lamp (`.st`): "● Running".
///
/// The label occupies the text's CSS line box, with the lamp
/// [`size::STATUS_DOT_GAP`] before the words, so it centres in a row like
/// any other line of text. Words that do not fit the available width are
/// elided and shown in full as a tooltip.
#[must_use = "call `show` to add the label"]
pub struct StatusLabel<'a> {
    text: &'a str,
    tone: Tone,
    role: TextRole,
    color: Option<Color32>,
    egui_row: bool,
}

impl<'a> StatusLabel<'a> {
    /// `text` with a lamp in `tone`, in [`type_scale::UI`] and `text-primary`.
    pub fn new(text: &'a str, tone: Tone) -> Self {
        Self {
            text,
            tone,
            role: type_scale::UI,
            color: None,
            egui_row: false,
        }
    }

    /// Set the words in `role` instead of [`type_scale::UI`].
    pub fn role(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    /// Set the words in `color` instead of `text-primary`.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Lay the words out as a plain egui label (one font row, egui's
    /// baseline) instead of a CSS line box, so they line up with the egui
    /// labels beside them on surfaces not yet set in CSS line boxes (tables,
    /// inspector values).
    pub fn in_egui_row(mut self) -> Self {
        self.egui_row = true;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let color = self.color.unwrap_or(theme.text.primary);
        let text_left = size::STATUS_DOT + size::STATUS_DOT_GAP;
        // One line, elided to the space left (narrow table columns).
        let wrap_width = (ui.available_width() - text_left).max(0.0);
        let galley = if self.egui_row {
            // Exactly the job an egui label of this text would lay out.
            let mut job = Arc::unwrap_or_clone(
                egui::WidgetText::from(self.role.text(ui.ctx(), self.text, color)).into_layout_job(
                    ui.style(),
                    egui::FontSelection::Default,
                    egui::Align::Min,
                ),
            );
            elide_to_one_row(&mut job, wrap_width);
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        } else {
            let mut job = self.role.layout_job(ui.ctx(), self.text, color, wrap_width);
            elide_to_one_row(&mut job, wrap_width);
            self.role.galley_from_job(ui.ctx(), job)
        };
        let desired = vec2(text_left + galley.rect.width(), galley.rect.height());
        let elided = galley.elided;
        let (rect, mut response) = type_scale::allocate_text(ui, desired, self.text);
        if ui.is_rect_visible(rect) {
            paint_status_dot(
                ui.painter(),
                &theme,
                lamp_rect(rect.left(), rect.center().y),
                self.tone,
            );
        }
        type_scale::paint_text(
            ui,
            &response,
            rect.min + vec2(text_left, 0.0),
            galley,
            color,
        );
        if elided {
            response = response.tooltip(self.text);
        }
        response
    }
}

/// Lay `job` out on one row, elided to `width`.
fn elide_to_one_row(job: &mut LayoutJob, width: f32) {
    job.wrap.max_width = width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
}

/// One container state square (`.cs`) in `tone`, labelled `label` for
/// assistive technologies. Lay them out [`size::CONTAINER_SQUARE_GAP`] apart,
/// one per container in spec order, with a tooltip describing each.
pub fn container_square(ui: &mut Ui, tone: Tone, label: &str) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(size::CONTAINER_SQUARE), Sense::hover());
    ui.painter()
        .rect_filled(rect, radius::CONTAINER_SQUARE, ui.tokens().tone(tone).fill);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Image, true, label));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::install_distinct_role_theme;
    use egui::epaint::{RectShape, Shape};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn rect_fills(shapes: &[egui::epaint::ClippedShape]) -> Vec<(Rect, Color32)> {
        shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Rect(RectShape { rect, fill, .. }) => Some((*rect, *fill)),
                _ => None,
            })
            .collect()
    }

    /// The colours of the glow meshes among `shapes`, one set per mesh.
    fn glow_colours(shapes: &[egui::epaint::ClippedShape]) -> Vec<Vec<Color32>> {
        shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Mesh(mesh) => {
                    Some(mesh.vertices.iter().map(|vertex| vertex.color).collect())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lit_lamps_glow_and_neutral_lamps_do_not() {
        let mut harness = Harness::builder().build_ui(|ui| {
            ui.horizontal(|ui| {
                status_dot(ui, Tone::Success, "Available");
                status_dot(ui, Tone::Neutral, "Offline");
            });
        });
        install_distinct_role_theme(&harness.ctx);
        harness.run();
        let theme = harness.ctx.tokens();
        let output = harness.output();
        let fills = rect_fills(&output.shapes);
        let glows = glow_colours(&output.shapes);
        assert_eq!(glows.len(), 1, "only the success lamp glows");
        let brightest = glows[0]
            .iter()
            .copied()
            .max_by_key(|colour| colour.a())
            .expect("glow vertices");
        let glow = theme.success.glow;
        assert!(brightest.a() > 0 && brightest.a() < glow.a());
        let scale = f32::from(brightest.a()) / f32::from(glow.a());
        let expected = glow.gamma_multiply(scale);
        for (channel, reference) in brightest.to_array().into_iter().zip(expected.to_array()) {
            assert!(
                channel.abs_diff(reference) <= 1,
                "{brightest:?} is not a fainter {glow:?}"
            );
        }
        assert!(fills.iter().any(|(_, fill)| *fill == theme.success.fill));
        assert!(fills.iter().any(|(_, fill)| *fill == theme.status.neutral));
    }

    #[test]
    fn status_labels_put_the_lamp_before_the_words_in_a_line_box() {
        let mut harness = Harness::builder().build_ui(|ui| {
            StatusLabel::new("Running", Tone::Success).show(ui);
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        let label = harness.get_by_label("Running");
        let bounds = label.rect();
        assert_eq!(bounds.height(), type_scale::UI.line_height());
        let output = harness.output();
        let fills = rect_fills(&output.shapes);
        let success = harness.ctx.tokens().success.fill;
        let lamp = fills
            .iter()
            .find(|(_, fill)| *fill == success)
            .expect("the lamp");
        assert_eq!(lamp.0.left(), bounds.left());
        assert_eq!(lamp.0.center().y, bounds.center().y);
        assert_eq!(lamp.0.size(), Vec2::splat(size::STATUS_DOT));
    }

    const LONG_STATUS: &str = "CrashLoopBackOff: back-off restarting failed container";

    /// How many accessibility nodes read `text` after hovering the status
    /// label reading it, in a 120 point wide column.
    fn nodes_after_hovering(text: &'static str) -> usize {
        let mut harness = Harness::builder().build_ui(move |ui| {
            ui.set_max_width(120.0);
            StatusLabel::new(text, Tone::Danger).show(ui);
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.get_by_label(text).hover();
        harness.run();
        harness.get_all_by_label(text).count()
    }

    #[test]
    fn only_elided_status_labels_show_their_words_on_hover() {
        assert_eq!(
            nodes_after_hovering(LONG_STATUS),
            2,
            "the tooltip repeats the words"
        );
        assert_eq!(
            nodes_after_hovering("Failed"),
            1,
            "words that fit need no tooltip"
        );
    }

    #[test]
    fn status_labels_in_an_egui_row_share_the_baseline_of_egui_labels() {
        let mut harness = Harness::builder().build_ui(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("api-pod").font(type_scale::BODY.font_id(ui.ctx())));
                StatusLabel::new("Running", Tone::Success)
                    .role(type_scale::BODY)
                    .in_egui_row()
                    .show(ui);
            });
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        let neighbour = harness.get_by_label("api-pod").rect();
        let status = harness.get_by_label("Running").rect();
        assert_eq!(status.top(), neighbour.top());
        assert_eq!(status.height(), neighbour.height());
        let output = harness.output();
        let baselines: Vec<f32> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Text(text) => {
                    let row = &text.galley.rows[0];
                    Some(text.pos.y + row.pos.y + row.glyphs[0].pos.y)
                }
                _ => None,
            })
            .collect();
        assert_eq!(baselines.len(), 2);
        assert_eq!(baselines[0], baselines[1]);
    }

    #[test]
    fn a_lamp_on_its_own_is_read_as_its_words() {
        let mut harness = Harness::builder().build_ui(|ui| {
            status_dot(ui, Tone::Success, "Online");
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        let lamp = harness.get_by_label("Online");
        assert_eq!(lamp.rect().size(), Vec2::splat(size::STATUS_DOT));
    }
}
