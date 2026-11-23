//! Elevation: the shadow roles of the theme.
//!
//! Harbour separates materials by tone, not borders. Flat surfaces (cards)
//! have no shadow; raised and overlay surfaces cast a soft drop shadow. The
//! geometry of each level is a fixed scale, while its colours are palette
//! entries, so a theme file that overrides `black-a55` darkens every shadow
//! that uses it.
//!
//! Resolved shadows live on the theme as [`Theme::shadow`]; read them from
//! there rather than from [`Elevation`] directly.
//!
//! [`Theme::shadow`]: super::theme::Theme::shadow

use egui::{Shadow, Stroke};

use super::palette::PaletteKey;

/// Flat surfaces: tone only, no shadow and no border.
pub const FLAT: Shadow = Shadow::NONE;

/// A raised elevation level, mirroring a CSS `--shadow-*` token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Elevation {
    /// Popovers, menus, toasts and floating toolbars.
    Raised,
    /// Blades, dialogs and the command palette.
    Overlay,
    /// Tooltips.
    Tooltip,
    /// A row or tile being dragged.
    Drag,
}

/// Unresolved definition of an elevation level: fixed geometry plus palette
/// references for its colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowSpec {
    /// Vertical offset in points.
    pub offset_y: i8,
    /// Blur width in points.
    pub blur: u8,
    /// Colour of the drop shadow.
    pub color: PaletteKey,
    /// Colour of a 1 point hairline ring around the surface (CSS
    /// `0 0 0 1px …`), if the level has one.
    pub ring: Option<PaletteKey>,
}

/// A resolved elevation level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowRole {
    /// The drop shadow, ready for `Frame::shadow` or `Visuals::popup_shadow`.
    pub shadow: Shadow,
    /// The hairline ring drawn just outside the surface, if any.
    pub ring: Option<Stroke>,
    /// The CSS blur radius of the drop shadow, which [`Self::paint`] blurs
    /// with the CSS Gaussian (`shadow.blur` is egui's wider linear ramp).
    pub blur: f32,
}

/// How much wider egui's blur must be than the CSS blur radius.
///
/// CSS blurs a shadow with a Gaussian of standard deviation `blur / 2`; egui
/// fades it linearly over `blur` points centred on the edge. A linear ramp
/// this much wider matches the Gaussian's 10–90 % transition, measured on
/// the calibration board (`docs/design-oracles/harbour/src/CAL.html`).
pub const CSS_BLUR_TO_EGUI: f32 = 1.5;

/// The egui blur width for a CSS blur radius.
fn egui_blur(css_blur: u8) -> u8 {
    (f32::from(css_blur) * CSS_BLUR_TO_EGUI)
        .round()
        .min(f32::from(u8::MAX)) as u8
}

/// Blur radius of the glow a lit status lamp casts (CSS `0 0 8px`).
pub const GLOW_BLUR: f32 = 8.0;

/// How far past the blurred rectangle its Gaussian is painted, in standard
/// deviations; beyond it the blur is invisible.
const BLUR_EXTENT_SIGMAS: f32 = 3.0;

/// The finest a blur's sample spacing gets relative to its standard
/// deviation: a quarter sigma keeps linear interpolation between samples
/// within a third of a colour level of the Gaussian.
const BLUR_SAMPLES_PER_SIGMA: f32 = 4.0;

/// Paint the glow of a small lit surface (a status lamp, CSS
/// `box-shadow: 0 0 <blur> <color>`) around `rect` with `corner_radius`.
/// Paint the surface itself afterwards.
///
/// egui's [`Shadow`] fades linearly, which is far too tight around an 8
/// point lamp; this paints the CSS profile ([`blurred_rect`]).
pub fn paint_glow(
    painter: &egui::Painter,
    rect: egui::Rect,
    corner_radius: f32,
    color: egui::Color32,
    blur: f32,
) {
    painter.add(blurred_rect(
        painter.ctx(),
        painter.pixels_per_point(),
        rect,
        corner_radius,
        color,
        blur,
    ));
}

/// `rect` with `corner_radius` filled in `color` and blurred as CSS blurs a
/// `box-shadow` of radius `blur`: by a Gaussian of standard deviation
/// `blur / 2`.
///
/// The blur is a mesh sampled at device pixel centres, scaled by the share
/// of the rectangle its rounded corners leave. The profile is separable, so
/// samples are only taken across the two edge transitions of each axis
/// (every pixel for small blurs, every quarter sigma for large ones); the
/// flat middle of a large surface needs none.
///
/// The mesh only depends on the rectangle's size, its offset within a
/// device pixel, the scale and the colour, so it is built once per
/// combination and moved into place.
pub fn blurred_rect(
    ctx: &egui::Context,
    pixels_per_point: f32,
    rect: egui::Rect,
    corner_radius: f32,
    color: egui::Color32,
    blur: f32,
) -> egui::Mesh {
    let anchor = (rect.min.to_vec2() * pixels_per_point).floor() / pixels_per_point;
    let local = rect.translate(-anchor);
    let key = BlurKey {
        rect: [local.min.x, local.min.y, local.width(), local.height()].map(f32::to_bits),
        shape: [corner_radius, blur, pixels_per_point].map(f32::to_bits),
        color,
    };
    // A frame cache: meshes no surface used last frame are dropped, so sizes
    // that come and go (a resized blade, a filtered popover) do not pile up.
    let mesh = ctx.memory_mut(|memory| {
        std::sync::Arc::clone(
            memory
                .caches
                .cache::<egui::cache::FrameCache<std::sync::Arc<egui::Mesh>, BlurMesher>>()
                .get(key),
        )
    });
    let mut mesh = (*mesh).clone();
    mesh.translate(anchor);
    mesh
}

/// What a [`blurred_rect`] mesh depends on: the rect within its device
/// pixel and its size, the corner radius, blur and scale, and the colour.
#[derive(Clone, Copy, Hash)]
struct BlurKey {
    rect: [u32; 4],
    shape: [u32; 3],
    color: egui::Color32,
}

/// Builds [`blurred_rect`] meshes for egui's frame cache.
#[derive(Default)]
struct BlurMesher;

impl egui::cache::ComputerMut<BlurKey, std::sync::Arc<egui::Mesh>> for BlurMesher {
    fn compute(&mut self, key: BlurKey) -> std::sync::Arc<egui::Mesh> {
        let [x, y, width, height] = key.rect.map(f32::from_bits);
        let [corner_radius, blur, pixels_per_point] = key.shape.map(f32::from_bits);
        std::sync::Arc::new(blurred_rect_mesh(
            egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, height)),
            corner_radius,
            key.color,
            blur,
            pixels_per_point,
        ))
    }
}

/// The sample positions of one axis of [`blurred_rect_mesh`]: device pixel
/// centres from `start` to `end` covering the transitions around `low` and
/// `high` (`step` pixels apart), skipping the flat stretch between them.
fn blur_samples(
    start: f32,
    end: f32,
    low: f32,
    high: f32,
    extent: f32,
    pixel: f32,
    step: usize,
) -> Vec<f32> {
    let origin = (start / pixel).floor() * pixel + pixel / 2.0;
    let count = ((end - origin) / pixel).ceil() as usize + 1;
    let position = |index: usize| origin + index as f32 * pixel;
    let in_transition = |index: usize| {
        let value = position(index);
        (value - low).abs() <= extent || (value - high).abs() <= extent
    };
    let mut samples = Vec::new();
    let mut run = 0;
    for index in 0..count {
        let first = index == 0 || !in_transition(index - 1);
        let last = index + 1 == count || !in_transition(index + 1);
        let keep = if in_transition(index) {
            if first {
                run = 0;
            }
            let keep = first || last || run % step == 0;
            run += 1;
            keep
        } else {
            index == 0 || index + 1 == count
        };
        if keep {
            samples.push(position(index));
        }
    }
    samples
}

/// The mesh of [`blurred_rect`] around `rect`.
fn blurred_rect_mesh(
    rect: egui::Rect,
    corner_radius: f32,
    color: egui::Color32,
    blur: f32,
    pixels_per_point: f32,
) -> egui::Mesh {
    if blur <= 0.0 {
        // No blur: the rounded rect itself.
        let mut mesh = egui::Mesh::default();
        mesh.add_colored_rect(rect, color);
        return mesh;
    }
    let sigma = blur / 2.0;
    let pixel = 1.0 / pixels_per_point;
    let area = rect.area();
    // Corners round at most to a pill, as egui and CSS clamp them.
    let corner_radius = corner_radius.min(rect.width().min(rect.height()) / 2.0);
    let corner_loss = (4.0 - std::f32::consts::PI) * corner_radius * corner_radius;
    let coverage = if area > 0.0 {
        ((area - corner_loss) / area).max(0.0)
    } else {
        0.0
    };
    let extent = BLUR_EXTENT_SIGMAS * sigma;
    let outer = rect.expand(extent);
    let step = ((sigma / BLUR_SAMPLES_PER_SIGMA) / pixel).floor().max(1.0) as usize;
    let xs = blur_samples(
        outer.left(),
        outer.right(),
        rect.left(),
        rect.right(),
        extent,
        pixel,
        step,
    );
    let ys = blur_samples(
        outer.top(),
        outer.bottom(),
        rect.top(),
        rect.bottom(),
        extent,
        pixel,
        step,
    );
    let center = rect.center();
    let half = rect.size() / 2.0;
    let profile_x: Vec<f32> = xs
        .iter()
        .map(|x| blurred_box(x - center.x, half.x, sigma))
        .collect();
    let profile_y: Vec<f32> = ys
        .iter()
        .map(|y| blurred_box(y - center.y, half.y, sigma) * coverage)
        .collect();

    // The profile is separable and peaks in the middle, so every coloured
    // vertex lies in the rows and columns that are coloured at their peak:
    // keep those plus one transparent sample on each side.
    let lit = |profile: &[f32], other: &[f32]| {
        let other_peak = other.iter().copied().fold(0.0, f32::max);
        let lit: Vec<bool> = profile
            .iter()
            .map(|value| color.gamma_multiply(value * other_peak) != egui::Color32::TRANSPARENT)
            .collect();
        let first = lit
            .iter()
            .position(|lit| *lit)
            .unwrap_or(0)
            .saturating_sub(1);
        let last = lit
            .iter()
            .rposition(|lit| *lit)
            .map_or(0, |last| (last + 1).min(profile.len() - 1));
        first..=last
    };
    let columns = lit(&profile_x, &profile_y);
    let rows = lit(&profile_y, &profile_x);

    let mut mesh = egui::Mesh::default();
    let width = columns.clone().count() as u32;
    for row in rows.clone() {
        for column in columns.clone() {
            mesh.colored_vertex(
                egui::pos2(xs[column], ys[row]),
                color.gamma_multiply(profile_x[column] * profile_y[row]),
            );
        }
    }
    let height = rows.count() as u32;
    for row in 0..height.saturating_sub(1) {
        for column in 0..width.saturating_sub(1) {
            let top_left = row * width + column;
            let bottom_left = top_left + width;
            mesh.add_triangle(top_left, top_left + 1, bottom_left);
            mesh.add_triangle(top_left + 1, bottom_left + 1, bottom_left);
        }
    }
    mesh
}

/// A box of half-width `half` centred on 0, blurred by a Gaussian of
/// standard deviation `sigma`, at `x`.
fn blurred_box(x: f32, half: f32, sigma: f32) -> f32 {
    let scale = 1.0 / (sigma * std::f32::consts::SQRT_2);
    0.5 * (erf((x + half) * scale) - erf((x - half) * scale))
}

/// The error function (Abramowitz and Stegun 7.1.26, error below 1.5e-7).
fn erf(x: f32) -> f32 {
    let sign = x.signum();
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let polynomial = t
        * (0.254_829_6
            + t * (-0.284_496_74 + t * (1.421_413_7 + t * (-1.453_152_1 + t * 1.061_405_4))));
    sign * (1.0 - polynomial * (-x * x).exp())
}

/// Width of the hairline ring of raised and overlay surfaces.
pub const RING_WIDTH: f32 = 1.0;

impl Elevation {
    /// Every raised level, in design order.
    pub const ALL: [Self; 4] = [Self::Raised, Self::Overlay, Self::Tooltip, Self::Drag];

    /// The CSS token name, for example `"shadow-raised"`.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Raised => "shadow-raised",
            Self::Overlay => "shadow-overlay",
            Self::Tooltip => "shadow-tooltip",
            Self::Drag => "shadow-drag",
        }
    }

    /// The level's geometry and palette references.
    pub const fn spec(self) -> ShadowSpec {
        match self {
            Self::Raised => ShadowSpec {
                offset_y: 24,
                blur: 60,
                color: PaletteKey::BlackA55,
                ring: Some(PaletteKey::WhiteA04),
            },
            Self::Overlay => ShadowSpec {
                offset_y: 30,
                blur: 70,
                color: PaletteKey::BlackA55,
                ring: Some(PaletteKey::WhiteA04),
            },
            Self::Tooltip => ShadowSpec {
                offset_y: 8,
                blur: 24,
                color: PaletteKey::BlackA45,
                ring: None,
            },
            Self::Drag => ShadowSpec {
                offset_y: 16,
                blur: 40,
                color: PaletteKey::BlackA55,
                ring: None,
            },
        }
    }
}

impl ShadowSpec {
    /// Resolve the spec against a palette lookup.
    pub fn resolve(self, palette: impl Fn(PaletteKey) -> egui::Color32) -> ShadowRole {
        ShadowRole {
            shadow: Shadow {
                offset: [0, self.offset_y],
                blur: egui_blur(self.blur),
                spread: 0,
                color: palette(self.color),
            },
            ring: self.ring.map(|ring| Stroke::new(RING_WIDTH, palette(ring))),
            blur: f32::from(self.blur),
        }
    }
}

impl ShadowRole {
    /// The ring as a frame stroke: the CSS `0 0 0 1px` ring for levels that
    /// have one, otherwise a transparent stroke of the same width, so a frame
    /// keeps its size whichever level it uses.
    pub fn outline(&self) -> Stroke {
        self.ring
            .unwrap_or(Stroke::new(RING_WIDTH, egui::Color32::TRANSPARENT))
    }

    /// Paint this elevation behind a surface occupying `rect` with
    /// `corner_radius`: the hairline ring just outside the surface, then the
    /// drop shadow over it (CSS paints the first `box-shadow` on top), blurred
    /// as CSS blurs it ([`blurred_rect`]). Paint the surface itself afterwards.
    pub fn paint(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        corner_radius: impl Into<egui::CornerRadius>,
    ) {
        painter.add(self.shape(
            painter.ctx(),
            painter.pixels_per_point(),
            rect,
            corner_radius,
        ));
    }

    /// The shapes [`Self::paint`] adds, for a surface laid out before its
    /// size is known (painted into a reserved shape slot).
    pub fn shape(
        &self,
        ctx: &egui::Context,
        pixels_per_point: f32,
        rect: egui::Rect,
        corner_radius: impl Into<egui::CornerRadius>,
    ) -> egui::Shape {
        let corner_radius = corner_radius.into();
        let [x, y] = self.shadow.offset;
        let spread = f32::from(self.shadow.spread);
        let shadow_rect = rect
            .translate(egui::vec2(f32::from(x), f32::from(y)))
            .expand(spread);
        let shadow: egui::Shape = blurred_rect(
            ctx,
            pixels_per_point,
            shadow_rect,
            // C20: a pill's `radius::FULL` is clamped to half its side, as
            // egui clamps the box's own corners.
            (corner_radius.average() + spread).min(shadow_rect.size().min_elem() / 2.0),
            self.shadow.color,
            self.blur,
        )
        .into();
        match self.ring {
            Some(ring) => egui::Shape::Vec(vec![
                egui::epaint::RectShape::stroke(
                    rect,
                    corner_radius,
                    ring,
                    egui::StrokeKind::Outside,
                )
                .into(),
                shadow,
            ]),
            None => shadow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erf_matches_reference_values() {
        for (x, expected) in [
            (0.0, 0.0),
            (0.5, 0.520_499_9),
            (1.0, 0.842_700_8),
            (2.0, 0.995_322_3),
        ] {
            assert!((erf(x) - expected).abs() < 1e-6, "erf({x})");
            assert!((erf(-x) + expected).abs() < 1e-6, "erf(-{x})");
        }
    }

    #[test]
    fn a_blurred_box_is_half_strength_at_its_edge_far_from_others() {
        assert!((blurred_box(100.0, 100.0, 4.0) - 0.5).abs() < 1e-4);
        assert!(blurred_box(0.0, 4.0, 4.0) < 1.0);
        assert!(blurred_box(20.0, 4.0, 4.0) < 1e-3);
    }

    /// The glow meshes painted for lamps at each of `rects`.
    fn glow_meshes(rects: &[egui::Rect]) -> Vec<egui::Mesh> {
        let ctx = egui::Context::default();
        let rects = rects.to_vec();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            for rect in &rects {
                paint_glow(
                    ui.painter(),
                    *rect,
                    3.0,
                    egui::Color32::from_rgba_unmultiplied(110, 231, 183, 140),
                    GLOW_BLUR,
                );
            }
        });
        output.textures_delta.clear();
        output
            .shapes
            .into_iter()
            .filter_map(|clipped| match clipped.shape {
                egui::Shape::Mesh(mesh) => Some((*mesh).clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn blurs_sample_device_pixel_centres_at_twice_the_scale() {
        let rect = egui::Rect::from_min_size(egui::pos2(10.25, 4.5), egui::vec2(120.0, 40.0));
        let color = crate::design::palette::BLACK_A55;
        let mesh = blurred_rect_mesh(rect, 8.0, color, 24.0, 2.0);
        for vertex in &mesh.vertices {
            for coordinate in [vertex.pos.x, vertex.pos.y] {
                let device = coordinate * 2.0 - 0.5;
                assert!((device - device.round()).abs() < 1e-3, "{coordinate}");
            }
        }
        // A blur of zero is the rectangle itself; huge radii clamp to a pill.
        assert_eq!(
            blurred_rect_mesh(rect, 4.0, color, 0.0, 2.0).vertices.len(),
            4
        );
        assert!(
            !blurred_rect_mesh(rect, 255.0, color, 24.0, 2.0)
                .vertices
                .is_empty()
        );
    }

    #[test]
    fn large_shadows_sample_only_their_edge_transitions() {
        // A blade: 720 × 1008 with the overlay blur (CSS 70, sigma 35).
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(720.0, 1008.0));
        let color = crate::design::palette::BLACK_A55;
        let mesh = blurred_rect_mesh(rect, 0.0, color, 70.0, 1.0);
        assert!(
            mesh.vertices.len() < 10_000,
            "{} vertices",
            mesh.vertices.len()
        );
        // CSS: half the colour on the edge, all of it well inside.
        let alpha_at = |pos: egui::Pos2| {
            mesh.vertices
                .iter()
                .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos)))
                .map(|vertex| vertex.color.a())
                .unwrap()
        };
        assert_eq!(alpha_at(egui::pos2(360.0, 504.0)), color.a());
        let edge = i32::from(alpha_at(egui::pos2(0.5, 504.0)));
        assert!(
            (edge - i32::from(color.a()) / 2).abs() <= 2,
            "edge alpha {edge}"
        );
    }

    #[test]
    fn glows_are_one_trimmed_mesh_moved_to_each_lamp() {
        let lamp =
            |x: f32, y: f32| egui::Rect::from_min_size(egui::pos2(x, y), egui::Vec2::splat(8.0));
        let meshes = glow_meshes(&[lamp(20.0, 20.0), lamp(120.0, 57.0)]);
        assert_eq!(meshes.len(), 2);
        let (first, second) = (&meshes[0], &meshes[1]);
        assert_eq!(first.indices, second.indices);
        for (a, b) in first.vertices.iter().zip(&second.vertices) {
            assert_eq!(a.color, b.color);
            assert_eq!(b.pos - a.pos, egui::vec2(100.0, 37.0));
        }
        // Untrimmed, the 3σ grid around an 8 point lamp is 33 × 33 vertices.
        assert!(
            first.vertices.len() < 33 * 33,
            "{} vertices",
            first.vertices.len()
        );
        // The border of the trimmed grid is transparent, so nothing is cut off.
        let left = first
            .vertices
            .iter()
            .map(|v| v.pos.x)
            .fold(f32::INFINITY, f32::min);
        assert!(
            first
                .vertices
                .iter()
                .filter(|v| v.pos.x == left)
                .all(|v| v.color == egui::Color32::TRANSPARENT)
        );
    }
}
