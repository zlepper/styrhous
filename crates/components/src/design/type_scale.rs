//! Type scale: one [`TextRole`] per typographic purpose.
//!
//! Harbour uses four typefaces: Figtree for UI text, Bricolage Grotesque
//! ([`Typeface::Display`]) for headings, IBM Plex Mono for readouts and code,
//! and IBM Plex Sans Condensed ([`Typeface::Caption`]) for captions and column
//! heads. Each weight (and each Bricolage optical size) is its own registered
//! family (see [`super::fonts`]); bold is never synthesised.
//!
//! # Matching CSS text
//!
//! The design is CSS, which differs from egui's defaults in three ways the
//! helpers here reproduce:
//!
//! - **Line boxes with half-leading.** A CSS line is `line-height` tall and
//!   the glyph box (ascent + descent) sits centred in it. egui puts glyphs at
//!   the top of a row. [`TextRole::galley`] and [`TextRole::label`] lay text
//!   out in CSS line boxes, baseline exactly where Chromium puts it, for text
//!   that is positioned by its top edge (block flow, painter code).
//! - **Centred single lines.** Inside a flex box with `align-items: center`
//!   (buttons, pills, rows) CSS centres the line box. Painter code centres a
//!   [`TextRole::galley`] rect, which is that line box. For egui widgets,
//!   [`TextRole::rich_text`] shrinks the row symmetrically so egui's own
//!   centring lands the baseline on the same pixel.
//! - **Tracking and case.** `letter-spacing` is applied after every
//!   character, including the last; egui only spaces between characters, so
//!   [`TextRole::galley`] adds the trailing space to its width. Upper-case
//!   roles upper-case the string.
//!
//! Glyph rasterization (unhinted, coverage gamma) is configured in
//! [`super::theme::visuals`]; with it, text on the calibration board
//! (`docs/design-oracles/harbour/src/CAL.html`) lands on the same pixels as in
//! Chromium.
//!
//! OpenType features are not available in egui, so `tabular-nums` (the
//! design's `.tnum`) cannot be applied to Figtree, whose default figures are
//! proportional. Mono readouts are unaffected (IBM Plex Mono is monospaced).
//!
//! # Bootstrap frame
//!
//! egui panics when laying out text in a family that is not registered. The
//! Harbour families are installed by [`super::theme::configure_default`] and
//! become active one pass later, so every helper takes the [`Context`] and
//! falls back to egui's generic families until then (in practice only the
//! first frame of a test harness, which runs before its setup).
//! The check reads the font set, so resolve fonts *before* entering a
//! `ctx.fonts_mut(..)` closure, never inside it.

use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{Color32, Context, FontFamily, FontId, Galley, Response, RichText, TextFormat, Ui};

use super::fonts::{self, Face};

/// A typeface of the Harbour type system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Typeface {
    /// Figtree: controls, tables, menus, navigation and prose (400–700).
    Figtree,
    /// Bricolage Grotesque: display text, headings and titles (700, one
    /// optical-size instance per font size).
    Display,
    /// IBM Plex Mono: changing numbers, readouts and code (400–600).
    Mono,
    /// IBM Plex Sans Condensed: captions, column heads, readout labels (500, 600).
    Caption,
}

/// A font weight. Only weights shipped as real font files are used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Weight {
    /// 400: body text.
    Regular,
    /// 500: controls.
    Medium,
    /// 600: buttons, selected items, emphasised cells.
    SemiBold,
    /// 700: display, headings and titles.
    Bold,
}

impl Typeface {
    /// The font file for this typeface at `weight` and `size`.
    ///
    /// Weights a typeface does not ship resolve to its nearest real face,
    /// as CSS font matching does (IBM Plex Sans Condensed 400 renders with
    /// Medium, as on Google Fonts). Bricolage Grotesque only ships Bold, at
    /// the optical sizes in [`fonts::BRICOLAGE_BOLD`]; other sizes panic, at
    /// compile time for role constants.
    pub const fn face(self, weight: Weight, size: f32) -> &'static Face {
        match (self, weight) {
            (Self::Figtree, Weight::Regular) => &fonts::FIGTREE_REGULAR,
            (Self::Figtree, Weight::Medium) => &fonts::FIGTREE_MEDIUM,
            (Self::Figtree, Weight::SemiBold) => &fonts::FIGTREE_SEMIBOLD,
            (Self::Figtree, Weight::Bold) => &fonts::FIGTREE_BOLD,
            (Self::Display, _) => fonts::bricolage(size),
            (Self::Mono, Weight::Regular) => &fonts::MONO_REGULAR,
            (Self::Mono, Weight::Medium) => &fonts::MONO_MEDIUM,
            (Self::Mono, Weight::SemiBold | Weight::Bold) => &fonts::MONO_SEMIBOLD,
            (Self::Caption, Weight::Regular | Weight::Medium) => &fonts::CAPTION_MEDIUM,
            (Self::Caption, Weight::SemiBold | Weight::Bold) => &fonts::CAPTION_SEMIBOLD,
        }
    }

    /// egui's generic family closest to this typeface, used before the
    /// Harbour fonts are active.
    fn generic_family(self) -> FontFamily {
        match self {
            Self::Mono => FontFamily::Monospace,
            Self::Figtree | Self::Display | Self::Caption => FontFamily::Proportional,
        }
    }
}

/// Everything needed to set one typographic role.
///
/// Construct roles with the `const` constructors so an invalid combination
/// (a display size without a Bricolage instance) fails to compile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRole {
    typeface: Typeface,
    weight: Weight,
    size: f32,
    line_height: f32,
    letter_spacing_em: f32,
    uppercase: bool,
}

impl TextRole {
    /// A role in `typeface` at `weight`, `size` and `line_height` points.
    pub const fn new(typeface: Typeface, weight: Weight, size: f32, line_height: f32) -> Self {
        // Resolve the face now so a missing Bricolage size is a const error.
        let _ = typeface.face(weight, size);
        Self {
            typeface,
            weight,
            size,
            line_height,
            letter_spacing_em: 0.0,
            uppercase: false,
        }
    }

    /// The same role at another weight, for example [`UI`] at
    /// [`Weight::Medium`] for controls.
    pub const fn with_weight(self, weight: Weight) -> Self {
        Self::new(self.typeface, weight, self.size, self.line_height).with_case_and_tracking(self)
    }

    /// The same role at another size and line height, for the design's
    /// in-between sizes (Figtree 13.5, Plex Mono 10.5/11.5/12.5, keycaps).
    pub const fn with_size(self, size: f32, line_height: f32) -> Self {
        Self::new(self.typeface, self.weight, size, line_height).with_case_and_tracking(self)
    }

    /// The same role with CSS `letter-spacing` in em.
    pub const fn with_letter_spacing(self, letter_spacing_em: f32) -> Self {
        Self {
            letter_spacing_em,
            ..self
        }
    }

    /// The same role, set in upper case (CSS `text-transform: uppercase`).
    pub const fn uppercase(self) -> Self {
        Self {
            uppercase: true,
            ..self
        }
    }

    const fn with_case_and_tracking(self, source: Self) -> Self {
        Self {
            letter_spacing_em: source.letter_spacing_em,
            uppercase: source.uppercase,
            ..self
        }
    }

    pub const fn typeface(&self) -> Typeface {
        self.typeface
    }

    pub const fn weight(&self) -> Weight {
        self.weight
    }

    /// Font size in points.
    pub const fn size(&self) -> f32 {
        self.size
    }

    /// Line height in points (CSS `line-height`).
    pub const fn line_height(&self) -> f32 {
        self.line_height
    }

    /// Whether the text is set in upper case.
    pub const fn is_uppercase(&self) -> bool {
        self.uppercase
    }

    /// The font file this role renders with.
    pub const fn face(&self) -> &'static Face {
        self.typeface.face(self.weight, self.size)
    }

    /// The registered font family for this role.
    pub const fn family_name(&self) -> &'static str {
        self.face().family
    }

    /// The role's font in its registered family.
    ///
    /// Only valid once the Harbour fonts are active; for style tables set up
    /// together with the fonts. Everything drawn inside a pass uses
    /// [`Self::font_id`].
    pub fn registered_font_id(&self) -> FontId {
        FontId::new(self.size, FontFamily::Name(self.family_name().into()))
    }

    /// The role's font: its registered family, or egui's closest generic
    /// family while the Harbour fonts are not active yet (bootstrap frame).
    pub fn font_id(&self, ctx: &Context) -> FontId {
        if fonts::are_active(ctx) {
            self.registered_font_id()
        } else {
            FontId::new(self.size, self.typeface.generic_family())
        }
    }

    /// Extra letter spacing in points.
    pub fn letter_spacing(&self) -> f32 {
        self.size * self.letter_spacing_em
    }

    /// The text as this role displays it.
    pub fn apply_case<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        if self.uppercase {
            text.to_uppercase().into()
        } else {
            text.into()
        }
    }

    /// Rich text in this role's font, tracking, case and `color`: the way to
    /// put role text into egui labels and widgets.
    ///
    /// Rows keep egui's own height (the font's glyph box), so this suits
    /// text whose vertical position egui decides. For text that must sit on
    /// the design's baseline use [`Self::rich_text`] (single centred lines)
    /// or [`Self::galley`] (blocks).
    pub fn text(&self, ctx: &Context, text: &str, color: Color32) -> RichText {
        RichText::new(self.apply_case(text))
            .font(self.font_id(ctx))
            .extra_letter_spacing(self.letter_spacing())
            .color(color)
    }

    /// Single-line rich text in this role's font, tracking and case, for egui
    /// widgets and layouts that centre their text (buttons, checkboxes,
    /// horizontal rows).
    ///
    /// CSS centres the line box; egui centres the galley, whose glyphs sit
    /// at its top. The row height is therefore shrunk symmetrically by the
    /// half-leading shift ([`Self::centred_row_height`]), which puts the
    /// baseline exactly where Chromium does once centred. Use
    /// [`Self::galley`] for multi-line or top-aligned text.
    pub fn rich_text(&self, ctx: &Context, text: &str) -> RichText {
        RichText::new(self.apply_case(text))
            .font(self.font_id(ctx))
            .extra_letter_spacing(self.letter_spacing())
            .line_height(Some(self.centred_row_height(ctx)))
    }

    /// The galley height [`Self::rich_text`] uses: the line height minus
    /// twice the half-leading shift.
    pub fn centred_row_height(&self, ctx: &Context) -> f32 {
        self.line_height - 2.0 * self.half_leading_shift(ctx)
    }

    /// A text format with the role's font, tracking and line height, for
    /// building multi-section [`LayoutJob`]s. Lay such jobs out with
    /// [`Self::galley_from_job`] to get CSS half-leading.
    pub fn text_format(&self, ctx: &Context, color: Color32) -> TextFormat {
        TextFormat {
            font_id: self.font_id(ctx),
            line_height: Some(self.line_height),
            extra_letter_spacing: self.letter_spacing(),
            color,
            ..TextFormat::default()
        }
    }

    /// A single-section job of `text` in this role, wrapping at `wrap_width`.
    pub fn layout_job(
        &self,
        ctx: &Context,
        text: &str,
        color: Color32,
        wrap_width: f32,
    ) -> LayoutJob {
        let mut job = LayoutJob::single_section(
            self.apply_case(text).into_owned(),
            self.text_format(ctx, color),
        );
        job.wrap.max_width = wrap_width;
        job
    }

    /// `text` laid out in CSS line boxes: every row is [`Self::line_height`]
    /// tall and its baseline sits where Chromium puts it. The galley's rect
    /// is the stack of line boxes, its width includes the trailing letter
    /// spacing CSS adds after the last character.
    pub fn galley(
        &self,
        ctx: &Context,
        text: &str,
        color: Color32,
        wrap_width: f32,
    ) -> Arc<Galley> {
        self.galley_from_job(ctx, self.layout_job(ctx, text, color, wrap_width))
    }

    /// Lay out `job` (whose sections use this role's line height, see
    /// [`Self::text_format`]) in CSS line boxes, as [`Self::galley`].
    ///
    /// The trailing letter spacing is reserved before wrapping, so the
    /// galley never ends up wider than the job's wrap width.
    pub fn galley_from_job(&self, ctx: &Context, mut job: LayoutJob) -> Arc<Galley> {
        let shift = self.half_leading_shift(ctx);
        let trailing = self.letter_spacing();
        if job.wrap.max_width.is_finite() {
            job.wrap.max_width = (job.wrap.max_width - trailing).max(0.0);
        }
        let galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
        if shift == 0.0 && trailing == 0.0 || galley.is_empty() {
            return galley;
        }
        let mut galley = Arc::unwrap_or_clone(galley);
        for row in &mut galley.rows {
            row.pos.y += shift;
        }
        galley.mesh_bounds = galley.mesh_bounds.translate(egui::vec2(0.0, shift));
        galley.rect.max.x += trailing;
        Arc::new(galley)
    }

    /// Paint `text` as a CSS block: one line box per row from the top of the
    /// allocated space (see [`Self::galley`]).
    pub fn label(&self, ui: &mut Ui, text: &str, color: Color32) -> Response {
        let galley = self.galley(ui.ctx(), text, color, ui.available_width());
        ui.add(egui::Label::new(galley).selectable(false))
    }

    /// The baseline of this role's line box centred in a row `row_height`
    /// points tall (CSS `align-items: center`), from the row's top, on the
    /// device pixel grid.
    ///
    /// A row that mixes roles (a title next to a body-text subtitle) sets
    /// them on one baseline, as CSS `align-items: baseline` does: take the
    /// baseline of the row's largest role from here and lay every text out
    /// with [`Self::label_on_baseline`].
    pub fn centred_baseline(&self, row_height: f32, pixels_per_point: f32) -> f32 {
        let baseline = (row_height - self.line_height) / 2.0 + self.css_baseline(pixels_per_point);
        (baseline * pixels_per_point).round() / pixels_per_point
    }

    /// A single-line label in a horizontal row whose baseline sits
    /// `baseline` points below the top of the row ([`Self::centred_baseline`]).
    /// The label occupies its CSS line box, placed on that baseline.
    pub fn label_on_baseline(
        &self,
        ui: &mut Ui,
        text: &str,
        color: Color32,
        baseline: f32,
    ) -> Response {
        let galley = self.galley(ui.ctx(), text, color, f32::INFINITY);
        let row = ui.cursor();
        let top = row.top() + baseline - self.css_baseline(ui.ctx().pixels_per_point());
        let rect = egui::Rect::from_min_size(egui::pos2(row.left(), top), galley.rect.size());
        let response = ui.allocate_rect(rect, egui::Sense::hover());
        let label = galley.text().to_owned();
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), label.clone())
        });
        ui.painter().galley(rect.min, galley, color);
        response
    }

    /// Distance from the top of a CSS line box to the baseline, in points,
    /// as Chromium computes it at `pixels_per_point` device pixels per CSS
    /// pixel: ascent and descent are rounded to whole device pixels, the
    /// remaining leading is split evenly above and below, and the baseline
    /// snaps down to the device pixel grid.
    pub fn css_baseline(&self, pixels_per_point: f32) -> f32 {
        let metrics = self.face().metrics;
        let ascent = (metrics.ascent * self.size * pixels_per_point).round();
        let descent = (metrics.descent * self.size * pixels_per_point).round();
        let line_height = self.line_height * pixels_per_point;
        ((line_height - ascent - descent) / 2.0 + ascent).floor() / pixels_per_point
    }

    /// How far egui's glyphs must move down from the top of an egui row
    /// ([`Self::text_format`] sets the row height to the line height) to sit
    /// on [`Self::css_baseline`].
    fn half_leading_shift(&self, ctx: &Context) -> f32 {
        if !fonts::are_active(ctx) {
            return 0.0;
        }
        self.half_leading_shift_at(ctx.pixels_per_point())
    }

    fn half_leading_shift_at(&self, pixels_per_point: f32) -> f32 {
        // egui rounds the ascent to 1/32 pt, then the baseline to a device pixel.
        let ascent = (self.face().metrics.ascent * self.size * 32.0).round() / 32.0;
        let egui_baseline = (ascent * pixels_per_point).round() / pixels_per_point;
        self.css_baseline(pixels_per_point) - egui_baseline
    }
}

/// Display · 28/36 · 700, −0.01 em. Board titles and log viewer headers.
pub const DISPLAY: TextRole =
    TextRole::new(Typeface::Display, Weight::Bold, 28.0, 36.0).with_letter_spacing(-0.01);
/// Heading · 20/28 · 700. Empty-state titles.
pub const HEADING: TextRole = TextRole::new(Typeface::Display, Weight::Bold, 20.0, 28.0);
/// Title · 16/24 · 700. Blade, dialog and window titles.
pub const TITLE: TextRole = TextRole::new(Typeface::Display, Weight::Bold, 16.0, 24.0);
/// Body · 14/20 · 400. Descriptions and prose.
pub const BODY: TextRole = TextRole::new(Typeface::Figtree, Weight::Regular, 14.0, 20.0);
/// UI · 13/20 · 400 (500 for controls). Controls, tables, menus, navigation.
pub const UI: TextRole = TextRole::new(Typeface::Figtree, Weight::Regular, 13.0, 20.0);
/// Meta · 12/16 · 400/500. Labels, secondary lines, status bars.
pub const META: TextRole = TextRole::new(Typeface::Figtree, Weight::Regular, 12.0, 16.0);
/// Caption · 11/16 · 500, upper case, +0.1 em. Section, group and column labels.
pub const CAPTION: TextRole = TextRole::new(Typeface::Caption, Weight::Medium, 11.0, 16.0)
    .with_letter_spacing(0.1)
    .uppercase();
/// Mono · 13/20 · 400. Readouts and code.
pub const MONO: TextRole = TextRole::new(Typeface::Mono, Weight::Regular, 13.0, 20.0);
/// Mono small · 12/18 · 400. Dense readouts such as image references.
pub const MONO_SMALL: TextRole = TextRole::new(Typeface::Mono, Weight::Regular, 12.0, 18.0);
/// Keycap · Plex Mono 10/16 · 500. Shortcut keys (`.kbd`).
pub const KEY: TextRole = TextRole::new(Typeface::Mono, Weight::Medium, 10.0, 16.0);

/// Dialog title · Bricolage 26/32 · 700. The title of application dialogs.
pub const DIALOG_TITLE: TextRole = TextRole::new(Typeface::Display, Weight::Bold, 26.0, 32.0);
/// Card title · Bricolage 18/26 · 700. Titles of large option cards.
pub const CARD_TITLE: TextRole = TextRole::new(Typeface::Display, Weight::Bold, 18.0, 26.0);
/// Body strong · 14/20 · 600. Emphasised body text: field labels, toast titles.
pub const BODY_STRONG: TextRole = BODY.with_weight(Weight::SemiBold);
/// UI control · 13/20 · 500. Text inside controls.
pub const UI_CONTROL: TextRole = UI.with_weight(Weight::Medium);

/// Every named role, for exhaustive checks.
pub const ALL: [TextRole; 14] = [
    DISPLAY,
    HEADING,
    TITLE,
    BODY,
    UI,
    META,
    CAPTION,
    MONO,
    MONO_SMALL,
    KEY,
    DIALOG_TITLE,
    CARD_TITLE,
    BODY_STRONG,
    UI_CONTROL,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::fonts::family;

    #[test]
    fn unshipped_weights_resolve_to_real_faces() {
        assert_eq!(
            TextRole::new(Typeface::Display, Weight::Regular, 20.0, 28.0).family_name(),
            HEADING.family_name()
        );
        assert_eq!(
            MONO.with_weight(Weight::Bold).family_name(),
            family::MONO_SEMIBOLD
        );
        assert_eq!(
            CAPTION.with_weight(Weight::Regular).family_name(),
            family::CAPTION_MEDIUM
        );
        assert_eq!(
            UI.with_weight(Weight::Medium).family_name(),
            family::FIGTREE_MEDIUM
        );
    }

    #[test]
    fn display_roles_pick_the_instance_for_their_size() {
        assert_eq!(DISPLAY.face().key, "bricolage-bold-opsz28");
        assert_eq!(HEADING.face().key, "bricolage-bold-opsz20");
        assert_eq!(TITLE.face().key, "bricolage-bold-opsz16");
        assert_eq!(CARD_TITLE.face().key, "bricolage-bold-opsz18");
        assert_eq!(DIALOG_TITLE.face().key, "bricolage-bold-opsz26");
        assert!(std::panic::catch_unwind(|| TITLE.with_size(24.0, 32.0)).is_err());
    }

    #[test]
    fn derived_roles_keep_case_and_tracking() {
        let semibold = CAPTION.with_weight(Weight::SemiBold);
        assert!(semibold.is_uppercase());
        assert_eq!(semibold.letter_spacing(), CAPTION.letter_spacing());
        assert_eq!(CAPTION.apply_case("Namespace"), "NAMESPACE");
        assert_eq!(BODY.apply_case("Namespace"), "Namespace");
        assert!((CAPTION.letter_spacing() - 1.1).abs() < 1e-6);
    }

    #[test]
    fn helpers_fall_back_to_generic_families_until_fonts_are_active() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            assert_eq!(MONO.font_id(ui.ctx()).family, FontFamily::Monospace);
            assert_eq!(TITLE.font_id(ui.ctx()).family, FontFamily::Proportional);
            // Laying out in the bootstrap frame must not panic.
            let _ = TITLE.galley(ui.ctx(), "Title", Color32::WHITE, f32::INFINITY);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn galleys_stack_css_line_boxes() {
        let ctx = Context::default();
        crate::design::theme::configure_default(&ctx);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            for role in ALL {
                let format = role.text_format(ui.ctx(), Color32::WHITE);
                assert_eq!(
                    format.font_id.family,
                    FontFamily::Name(role.family_name().into())
                );
                assert_eq!(format.line_height, Some(role.line_height()));

                let galley =
                    role.galley(ui.ctx(), "Harbour\nHarbour", Color32::WHITE, f32::INFINITY);
                assert_eq!(galley.rows.len(), 2);
                assert_eq!(galley.rect.height(), 2.0 * role.line_height(), "{role:?}");
                let pitch = galley.rows[1].pos.y - galley.rows[0].pos.y;
                assert_eq!(pitch, role.line_height(), "{role:?}");
            }
        });
        output.textures_delta.clear();
    }

    /// Runs one pass of a configured context at `pixels_per_point`.
    fn run_at(ctx: &Context, pixels_per_point: f32, f: impl FnMut(&mut Ui)) {
        let mut input = egui::RawInput::default();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(pixels_per_point);
        let mut output = ctx.run_ui(input, f);
        output.textures_delta.clear();
    }

    #[test]
    fn baselines_round_in_device_pixels() {
        // Figtree 14/20: 13.3 + 3.5 points of glyph box.
        assert_eq!(BODY.css_baseline(1.0), 14.0);
        assert_eq!(BODY.css_baseline(2.0), 15.0);
        for role in ALL {
            for pixels_per_point in [1.0, 1.5, 2.0] {
                let baseline = role.css_baseline(pixels_per_point) * pixels_per_point;
                assert_eq!(baseline, baseline.round(), "{role:?} at {pixels_per_point}");
            }
        }
    }

    #[test]
    fn galleys_put_the_baseline_where_chromium_does_at_any_scale() {
        let ctx = Context::default();
        crate::design::theme::configure_default(&ctx);
        for pixels_per_point in [1.0, 2.0] {
            // The first pass at a new scale may still use the old font atlas.
            run_at(&ctx, pixels_per_point, |_| {});
            run_at(&ctx, pixels_per_point, |ui| {
                for role in ALL {
                    let galley = role.galley(ui.ctx(), "Harbour", Color32::WHITE, f32::INFINITY);
                    let row = &galley.rows[0];
                    let baseline = row.pos.y + row.glyphs[0].pos.y;
                    assert_eq!(
                        baseline,
                        role.css_baseline(pixels_per_point),
                        "{role:?} at {pixels_per_point}"
                    );
                }
            });
        }
    }

    #[test]
    fn tracked_galleys_never_exceed_their_wrap_width() {
        let ctx = Context::default();
        crate::design::theme::configure_default(&ctx);
        run_at(&ctx, 1.0, |_| {});
        run_at(&ctx, 1.0, |ui| {
            let text = "Namespace column head that wraps";
            let trailing = CAPTION.letter_spacing();
            let content_width = |text: &str| {
                let galley = CAPTION.galley(ui.ctx(), text, Color32::WHITE, f32::INFINITY);
                galley.rows[0].glyphs.last().expect("a glyph").max_x()
            };
            // Without trailing spacing; egui keeps text on a row up to half a
            // point past the wrap width.
            let full = content_width(text);
            // The longest word fits every width, so rows never have to overflow.
            let longest = content_width("Namespace") + trailing;
            for wrap_width in [longest + 0.5, longest + 20.5, full + trailing - 1.0] {
                let galley = CAPTION.galley(ui.ctx(), text, Color32::WHITE, wrap_width);
                assert!(galley.rows.len() > 1, "wraps at {wrap_width}");
                assert!(
                    galley.rect.width() <= wrap_width,
                    "{} > {wrap_width}",
                    galley.rect.width()
                );
            }
            // Text that fits with its trailing spacing stays on one row and keeps it.
            let fitting = CAPTION.galley(ui.ctx(), text, Color32::WHITE, full + trailing);
            assert_eq!(fitting.rows.len(), 1);
            assert!(fitting.rect.width() >= full + trailing - 1.0 / 32.0);
        });
    }

    #[test]
    fn text_carries_font_tracking_case_and_colour_without_a_line_height() {
        let ctx = Context::default();
        crate::design::theme::configure_default(&ctx);
        run_at(&ctx, 1.0, |_| {});
        run_at(&ctx, 1.0, |ui| {
            let style = ui.style().clone();
            let text = CAPTION.text(ui.ctx(), "Namespace", Color32::RED);
            assert_eq!(text.text(), "NAMESPACE");
            let job = egui::WidgetText::from(text).into_layout_job(
                &style,
                egui::FontSelection::Default,
                egui::Align::Center,
            );
            let format = &job.sections[0].format;
            assert_eq!(format.font_id, CAPTION.registered_font_id());
            assert_eq!(format.extra_letter_spacing, CAPTION.letter_spacing());
            assert_eq!(format.color, Color32::RED);
            assert_eq!(format.line_height, None);
        });
    }

    #[test]
    fn mixed_roles_share_one_baseline_in_a_row() {
        let ctx = Context::default();
        crate::design::theme::configure_default(&ctx);
        run_at(&ctx, 1.0, |_| {});
        run_at(&ctx, 1.0, |ui| {
            let row = egui::Rect::from_min_size(egui::pos2(0.0, 10.0), egui::vec2(600.0, 56.0));
            let mut row_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            let baseline = TITLE.centred_baseline(row_ui.available_height(), 1.0);
            // Title 16/24 centred in 56: (56 - 24) / 2 + 17.
            assert_eq!(baseline, 33.0);
            let title = TITLE.label_on_baseline(&mut row_ui, "Logs", Color32::WHITE, baseline);
            let body = BODY.label_on_baseline(&mut row_ui, "Container", Color32::WHITE, baseline);
            assert_eq!(
                title.rect.top() + TITLE.css_baseline(1.0),
                row.top() + baseline
            );
            assert_eq!(
                body.rect.top() + BODY.css_baseline(1.0),
                row.top() + baseline
            );
            assert_eq!(title.rect.height(), TITLE.line_height());
            assert!(body.rect.left() > title.rect.right());
        });
    }
}
