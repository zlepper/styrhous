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
///
/// Every lamp of a size casts the same glow, and a table shows dozens, so
/// the profile is rasterised once into a white texture (one texel per device
/// pixel, holding exactly the samples [`blurred_rect`] would put on its
/// vertices) and each glow is a single tinted quad of it. Snap `rect` to the
/// device pixel grid (as lamps are): a glow at a new sub-pixel offset needs a
/// texture of its own.
pub fn paint_glow(
    painter: &egui::Painter,
    rect: egui::Rect,
    corner_radius: f32,
    color: egui::Color32,
    blur: f32,
) {
    if blur <= 0.0 {
        // No blur: the rounded rect itself.
        painter.rect_filled(rect, corner_radius, color);
        return;
    }
    let pixels_per_point = painter.pixels_per_point();
    let (anchor, local) = device_anchor(rect, pixels_per_point);
    let geometry = BlurGeometry::new(local, corner_radius, blur, pixels_per_point);
    let (texture, covered) = glow_texture(painter.ctx(), geometry);
    let mut mesh = egui::Mesh::with_texture(texture);
    mesh.add_rect_with_uv(
        covered.translate(anchor),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        color,
    );
    painter.add(mesh);
}

/// The device pixel corner at or before `rect`'s top left, and `rect`
/// relative to it, its offset rounded to [`SUB_PIXEL_STEPS`] first so float
/// noise around a pixel boundary picks the same corner.
fn device_anchor(rect: egui::Rect, pixels_per_point: f32) -> (egui::Vec2, egui::Rect) {
    let steps = (rect.min.to_vec2() * pixels_per_point * SUB_PIXEL_STEPS).round();
    let unit = pixels_per_point * SUB_PIXEL_STEPS;
    let whole = egui::vec2(
        steps.x.div_euclid(SUB_PIXEL_STEPS),
        steps.y.div_euclid(SUB_PIXEL_STEPS),
    );
    let offset = egui::vec2(
        steps.x.rem_euclid(SUB_PIXEL_STEPS),
        steps.y.rem_euclid(SUB_PIXEL_STEPS),
    );
    let anchor = whole / pixels_per_point;
    let local = egui::Rect::from_min_size((offset / unit).to_pos2(), rect.size());
    (anchor, local)
}

/// How finely the position of a blurred rect within its device pixel is
/// told apart, in steps per device pixel. Finer differences are invisible,
/// and coarser keys keep float noise (a lamp snapped to the pixel grid at a
/// fractional scale) from building a new blur every frame.
const SUB_PIXEL_STEPS: f32 = 64.0;

/// What a blur's geometry depends on: the rect relative to its device pixel
/// anchor and its size (both quantised to [`SUB_PIXEL_STEPS`]), the corner
/// radius, blur and scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BlurGeometry {
    offset: [i32; 2],
    size: [i32; 2],
    shape: [u32; 3],
}

impl BlurGeometry {
    fn new(local: egui::Rect, corner_radius: f32, blur: f32, pixels_per_point: f32) -> Self {
        let step =
            |coordinate: f32| (coordinate * pixels_per_point * SUB_PIXEL_STEPS).round() as i32;
        Self {
            offset: [step(local.min.x), step(local.min.y)],
            size: [step(local.width()), step(local.height())],
            shape: [corner_radius, blur, pixels_per_point].map(f32::to_bits),
        }
    }

    fn pixels_per_point(&self) -> f32 {
        f32::from_bits(self.shape[2])
    }

    /// The rect relative to its anchor, at the quantised offset.
    fn rect(&self) -> egui::Rect {
        let unit = self.pixels_per_point() * SUB_PIXEL_STEPS;
        let [width, height] = self.size.map(|steps| steps as f32 / unit);
        egui::Rect::from_min_size(
            egui::pos2(self.offset[0] as f32 / unit, self.offset[1] as f32 / unit),
            egui::vec2(width, height),
        )
    }

    fn corner_radius(&self) -> f32 {
        f32::from_bits(self.shape[0])
    }

    fn blur(&self) -> f32 {
        f32::from_bits(self.shape[1])
    }
}

/// A rasterised glow: its texture and where the texture goes relative to the
/// lamp's device pixel anchor. A glow texture depends on the geometry alone;
/// the quad tints it.
#[derive(Clone)]
struct GlowTexture {
    texture: egui::TextureHandle,
    rect: egui::Rect,
    last_used: u64,
}

/// Glow textures by geometry. Few exist (a lamp size per scale), and a
/// frame without lamps (a hidden table) should not reupload them, so one is
/// freed only once it has gone unused for [`GLOW_TEXTURE_IDLE_PASSES`];
/// egui's frame caches drop what the last frame did not use.
#[derive(Clone, Default)]
struct GlowTextures {
    textures: std::collections::HashMap<BlurGeometry, GlowTexture>,
    swept: u64,
}

/// Passes a glow texture may go unused before it is freed.
const GLOW_TEXTURE_IDLE_PASSES: u64 = 600;

fn glow_textures_id() -> egui::Id {
    egui::Id::new("components::design::elevation::glow_textures")
}

/// The texture of the glow of `geometry` and the rect it covers relative to
/// the anchor, rasterised on first use.
fn glow_texture(ctx: &egui::Context, geometry: BlurGeometry) -> (egui::TextureId, egui::Rect) {
    let pass = ctx.cumulative_pass_nr();
    let cached = ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<GlowTextures>(glow_textures_id());
        let textures = &mut cache.textures;
        if cache.swept != pass {
            cache.swept = pass;
            textures.retain(|_, glow| pass - glow.last_used <= GLOW_TEXTURE_IDLE_PASSES);
        }
        textures.get_mut(&geometry).map(|glow| {
            glow.last_used = pass;
            (glow.texture.id(), glow.rect)
        })
    });
    if let Some(cached) = cached {
        return cached;
    }
    // Loading a texture locks the context, so it happens outside `data_mut`.
    let (image, rect) = glow_image(geometry);
    let texture = ctx.load_texture("status-lamp-glow", image, egui::TextureOptions::LINEAR);
    let id = texture.id();
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<GlowTextures>(glow_textures_id());
        cache.textures.insert(
            geometry,
            GlowTexture {
                texture,
                rect,
                last_used: pass,
            },
        );
    });
    (id, rect)
}

/// The white glow of `geometry` with one texel per device pixel, and the
/// rect (relative to the anchor) it covers. Each texel holds the sample
/// [`blurred_rect_mesh`] takes at that device pixel's centre, so a quad of
/// it tinted with a colour paints what the mesh in that colour does.
fn glow_image(geometry: BlurGeometry) -> (egui::ColorImage, egui::Rect) {
    let pixels_per_point = geometry.pixels_per_point();
    let rect = geometry.rect();
    let blur = geometry.blur();
    let profile = BlurProfile::new(rect, geometry.corner_radius(), blur);
    let outer = rect.expand(profile.extent);
    let first = (outer.min.to_vec2() * pixels_per_point).floor();
    let last = (outer.max.to_vec2() * pixels_per_point).ceil();
    let [columns, rows] = [(last.x - first.x) as usize, (last.y - first.y) as usize];
    let centre = |index: usize, first: f32| (first + index as f32 + 0.5) / pixels_per_point;
    let xs: Vec<f32> = (0..columns)
        .map(|column| profile.x(centre(column, first.x)))
        .collect();
    let ys: Vec<f32> = (0..rows)
        .map(|row| profile.y(centre(row, first.y)))
        .collect();
    // colour-guard: allow-begin (a coverage mask: the quad tints it with the theme's glow role)
    let mask = egui::Color32::WHITE;
    // colour-guard: allow-end
    let pixels = ys
        .iter()
        .flat_map(|y| xs.iter().map(move |x| mask.gamma_multiply(x * y)))
        .collect();
    let image = egui::ColorImage::new([columns, rows], pixels);
    let covered = egui::Rect::from_min_max(
        (first / pixels_per_point).to_pos2(),
        (last / pixels_per_point).to_pos2(),
    );
    (image, covered)
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
    let (anchor, local) = device_anchor(rect, pixels_per_point);
    let key = BlurKey {
        geometry: BlurGeometry::new(local, corner_radius, blur, pixels_per_point),
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

/// What a [`blurred_rect`] mesh depends on: its geometry and colour.
#[derive(Clone, Copy, Hash)]
struct BlurKey {
    geometry: BlurGeometry,
    color: egui::Color32,
}

/// Builds [`blurred_rect`] meshes for egui's frame cache.
#[derive(Default)]
struct BlurMesher;

impl egui::cache::ComputerMut<BlurKey, std::sync::Arc<egui::Mesh>> for BlurMesher {
    fn compute(&mut self, key: BlurKey) -> std::sync::Arc<egui::Mesh> {
        let geometry = key.geometry;
        std::sync::Arc::new(blurred_rect_mesh(
            geometry.rect(),
            geometry.corner_radius(),
            key.color,
            geometry.blur(),
            geometry.pixels_per_point(),
        ))
    }
}

/// The CSS blur of a rounded rect (a Gaussian of standard deviation
/// `blur / 2`), as two separable profiles; the vertical one also carries the
/// share of the rect the rounded corners leave.
struct BlurProfile {
    centre: egui::Pos2,
    half: egui::Vec2,
    sigma: f32,
    coverage: f32,
    /// How far past the rect the blur is painted.
    extent: f32,
}

impl BlurProfile {
    fn new(rect: egui::Rect, corner_radius: f32, blur: f32) -> Self {
        let sigma = blur.max(0.0) / 2.0;
        let area = rect.area();
        // Corners round at most to a pill, as egui and CSS clamp them.
        let corner_radius = corner_radius.min(rect.width().min(rect.height()) / 2.0);
        let corner_loss = (4.0 - std::f32::consts::PI) * corner_radius * corner_radius;
        let coverage = if area > 0.0 {
            ((area - corner_loss) / area).max(0.0)
        } else {
            0.0
        };
        Self {
            centre: rect.center(),
            half: rect.size() / 2.0,
            sigma,
            coverage,
            extent: BLUR_EXTENT_SIGMAS * sigma,
        }
    }

    /// The horizontal profile at `x`.
    fn x(&self, x: f32) -> f32 {
        blurred_box(x - self.centre.x, self.half.x, self.sigma)
    }

    /// The vertical profile at `y`, scaled by the corner coverage.
    fn y(&self, y: f32) -> f32 {
        blurred_box(y - self.centre.y, self.half.y, self.sigma) * self.coverage
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
    let profile = BlurProfile::new(rect, corner_radius, blur);
    let pixel = 1.0 / pixels_per_point;
    let outer = rect.expand(profile.extent);
    let step = ((profile.sigma / BLUR_SAMPLES_PER_SIGMA) / pixel)
        .floor()
        .max(1.0) as usize;
    let xs = blur_samples(
        outer.left(),
        outer.right(),
        rect.left(),
        rect.right(),
        profile.extent,
        pixel,
        step,
    );
    let ys = blur_samples(
        outer.top(),
        outer.bottom(),
        rect.top(),
        rect.bottom(),
        profile.extent,
        pixel,
        step,
    );
    let profile_x: Vec<f32> = xs.iter().map(|x| profile.x(*x)).collect();
    let profile_y: Vec<f32> = ys.iter().map(|y| profile.y(*y)).collect();

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
    use egui::emath::GuiRounding as _;

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
        glow_meshes_at(&egui::Context::default(), rects, 1.0)
    }

    /// The glow meshes `ctx` paints for lamps at each of `rects` (snapped to
    /// the pixel grid, as lamps are) on a display scaled `pixels_per_point`.
    fn glow_meshes_at(
        ctx: &egui::Context,
        rects: &[egui::Rect],
        pixels_per_point: f32,
    ) -> Vec<egui::Mesh> {
        let rects = rects.to_vec();
        let input = egui::RawInput {
            viewports: std::iter::once((
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    native_pixels_per_point: Some(pixels_per_point),
                    ..Default::default()
                },
            ))
            .collect(),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            let pixels_per_point = ui.pixels_per_point();
            for rect in &rects {
                let snapped = rect.round_to_pixels(pixels_per_point);
                paint_glow(ui.painter(), snapped, 3.0, GLOW, GLOW_BLUR);
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

    const GLOW: egui::Color32 = egui::Color32::from_rgba_premultiplied(61, 127, 101, 140);

    fn lamp(x: f32, y: f32) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(x, y), egui::Vec2::splat(8.0))
    }

    #[test]
    fn glows_are_one_shared_texture_quad_per_lamp() {
        let meshes = glow_meshes(&[lamp(20.0, 20.0), lamp(120.0, 57.0)]);
        assert_eq!(meshes.len(), 2);
        let (first, second) = (&meshes[0], &meshes[1]);
        assert_eq!(first.vertices.len(), 4);
        assert_ne!(first.texture_id, egui::TextureId::default());
        assert_eq!(first.texture_id, second.texture_id);
        for (a, b) in first.vertices.iter().zip(&second.vertices) {
            assert_eq!(a.color, GLOW);
            assert_eq!(a.uv, b.uv);
            assert_eq!(b.pos - a.pos, egui::vec2(100.0, 37.0));
        }
    }

    #[test]
    fn the_glow_texture_holds_the_samples_the_mesh_takes() {
        for (rect, pixels_per_point) in [(lamp(20.0, 20.0), 1.0), (lamp(13.6, 7.2), 1.25)] {
            let (anchor, local) = device_anchor(rect, pixels_per_point);
            let geometry = BlurGeometry::new(local, 3.0, GLOW_BLUR, pixels_per_point);
            let (image, covered) = glow_image(geometry);
            let mesh = blurred_rect_mesh(
                geometry.rect(),
                3.0,
                egui::Color32::WHITE,
                GLOW_BLUR,
                pixels_per_point,
            );
            assert!(!mesh.vertices.is_empty());
            for vertex in &mesh.vertices {
                // The texel whose centre is the vertex.
                let texel = (vertex.pos - covered.min) * pixels_per_point - egui::vec2(0.5, 0.5);
                let [column, row] = [texel.x.round() as usize, texel.y.round() as usize];
                assert!((texel - egui::vec2(column as f32, row as f32)).length() < 1e-3);
                assert_eq!(
                    image.pixels[row * image.size[0] + column],
                    vertex.color,
                    "texel {column},{row} @{pixels_per_point}× (anchor {anchor:?})"
                );
            }
            // The texture covers whole device pixels.
            for coordinate in [covered.min.x, covered.min.y, covered.max.x, covered.max.y] {
                let device = coordinate * pixels_per_point;
                assert!((device - device.round()).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn float_noise_in_a_lamp_position_reuses_its_glow() {
        // A lamp snapped to the pixel grid at 1.25× lands a hair off it.
        let snapped = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::Vec2::splat(8.0));
        let noisy = snapped.translate(egui::vec2(1e-5, -1e-5));
        let key = |rect: egui::Rect| {
            let (_, local) = device_anchor(rect, 1.25);
            BlurGeometry::new(local, 3.0, GLOW_BLUR, 1.25)
        };
        assert_eq!(key(snapped), key(noisy));
        assert_ne!(key(snapped), key(snapped.translate(egui::vec2(0.4, 0.0))));
    }

    #[test]
    fn glows_sit_on_the_device_pixel_grid_centred_on_their_lamp_at_fractional_scales() {
        for pixels_per_point in [1.25, 1.5] {
            let ctx = egui::Context::default();
            let lamps = [lamp(13.6, 7.2), lamp(101.3, 44.9)];
            let meshes = glow_meshes_at(&ctx, &lamps, pixels_per_point);
            assert_eq!(meshes.len(), lamps.len());
            for (mesh, lamp) in meshes.iter().zip(lamps) {
                let lamp = lamp.round_to_pixels(pixels_per_point);
                let quad = egui::Rect::from_points(
                    &mesh
                        .vertices
                        .iter()
                        .map(|vertex| vertex.pos)
                        .collect::<Vec<_>>(),
                );
                for coordinate in [quad.min.x, quad.min.y, quad.max.x, quad.max.y] {
                    let device = coordinate * pixels_per_point;
                    assert!(
                        (device - device.round()).abs() < 1e-3,
                        "@{pixels_per_point}×: {quad:?} is off the pixel grid"
                    );
                }
                // The glow reaches as far past the lamp on every side, to the pixel.
                let pixel = 1.0 / pixels_per_point;
                let left = lamp.left() - quad.left();
                for reach in [
                    quad.right() - lamp.right(),
                    lamp.top() - quad.top(),
                    quad.bottom() - lamp.bottom(),
                ] {
                    assert!(
                        (reach - left).abs() <= pixel + 1e-3,
                        "@{pixels_per_point}×: {quad:?} is not centred on {lamp:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn each_scale_rasterises_its_glow_once_and_keeps_it() {
        let ctx = egui::Context::default();
        let lamps = [lamp(20.0, 20.0)];
        let texture =
            |pixels_per_point| glow_meshes_at(&ctx, &lamps, pixels_per_point)[0].texture_id;
        let at_one = texture(1.0);
        let at_two = texture(2.0);
        assert_ne!(at_one, at_two, "a scale has a texture of its own");
        assert_eq!(texture(1.0), at_one, "going back reuses the first texture");
        assert_eq!(texture(2.0), at_two);
    }
}
