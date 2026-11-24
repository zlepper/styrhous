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
//! design's `.tnum`) is a role flag instead: [`TextRole::tabular`] switches
//! Figtree to its Figtree Tabular family, whose digits are the `tnum` glyphs.
//! Mono readouts need nothing (IBM Plex Mono is monospaced).
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
    tabular: bool,
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
            tabular: false,
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

    /// The same role with tabular figures (CSS `font-variant-numeric:
    /// tabular-nums`): every digit is as wide as the others, so counts and
    /// readouts that change do not jitter. IBM Plex Mono's figures already
    /// are; Figtree switches to its Figtree Tabular family
    /// ([`fonts::FIGTREE_TABULAR_REGULAR`] and siblings).
    pub const fn tabular(self) -> Self {
        Self {
            tabular: true,
            ..self
        }
    }

    /// Whether the role sets tabular figures ([`Self::tabular`]).
    pub const fn is_tabular(&self) -> bool {
        self.tabular
    }

    const fn with_case_and_tracking(self, source: Self) -> Self {
        Self {
            letter_spacing_em: source.letter_spacing_em,
            uppercase: source.uppercase,
            tabular: source.tabular,
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
        match (self.typeface, self.tabular) {
            (Typeface::Figtree, true) => match self.weight {
                Weight::Regular => &fonts::FIGTREE_TABULAR_REGULAR,
                Weight::Medium => &fonts::FIGTREE_TABULAR_MEDIUM,
                Weight::SemiBold => &fonts::FIGTREE_TABULAR_SEMIBOLD,
                Weight::Bold => &fonts::FIGTREE_TABULAR_BOLD,
            },
            _ => self.typeface.face(self.weight, self.size),
        }
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

    /// `text` wrapped to `width` on at most `max_rows` CSS line boxes, the
    /// last one elided (`galley.elided` says whether it was). A single row
    /// breaks anywhere, like `text-overflow: ellipsis`.
    pub fn galley_elided(
        &self,
        ctx: &Context,
        text: &str,
        color: Color32,
        width: f32,
        max_rows: usize,
    ) -> Arc<Galley> {
        let mut job = self.layout_job(ctx, text, color, width);
        job.wrap.max_rows = max_rows;
        job.wrap.break_anywhere = max_rows == 1;
        self.galley_from_job(ctx, job)
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
    /// allocated space (see [`Self::galley`]), at its exact horizontal
    /// position ([`paint_galley`]).
    pub fn label(&self, ui: &mut Ui, text: &str, color: Color32) -> Response {
        let galley = self.galley(ui.ctx(), text, color, ui.available_width());
        let (rect, response) = allocate_text(ui, galley.rect.size(), galley.text());
        paint_text(ui, &response, rect.min, galley, color);
        response
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
        let response = allocate_text_at(ui, rect, galley.text());
        paint_text(ui, &response, rect.min, galley, color);
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

    /// How far egui's glyphs must move down from the top of an egui row (whose height is
    /// the line height) to sit where Chromium puts them in a line box, for text laid out
    /// by egui itself (an editable [`egui::TextEdit`]) rather than with [`Self::galley`].
    pub fn row_shift(&self, ctx: &Context) -> f32 {
        self.half_leading_shift(ctx)
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

/// Paint `galley` with its top-left corner at `pos`, keeping the horizontal
/// sub-pixel offset of `pos` the way Chromium positions text.
///
/// egui snaps every galley to the device pixel grid, so text whose CSS box
/// starts at a fractional x (after a pill, a lamp or proportional text)
/// would land up to half a pixel off. A single-row galley is laid out again
/// with the fraction as leading space, so egui rasterizes its glyphs at the
/// right sub-pixel offsets; its wrapping (and so any elision) is kept.
///
/// egui also rounds every glyph to the pixel grid relative to the start of
/// its paragraph, so in a wrapped paragraph the rounding of a later line
/// depends on where it wrapped. Chromium lays every line out on its own, so
/// each row of a multi-row galley is laid out again as its own line
/// ([`relayout_rows`]) and painted like a single-row galley. Galleys whose
/// rows cannot be mapped back to their text (elided rows) are painted
/// snapped as egui paints them. Vertical positions always snap, as Chromium
/// snaps baselines.
pub fn paint_galley(painter: &egui::Painter, pos: egui::Pos2, galley: Arc<Galley>, color: Color32) {
    if galley.rows.len() > 1 {
        match relayout_rows(painter.ctx(), &galley) {
            Some(rows) => {
                for (offset, row) in rows {
                    paint_galley(painter, pos + offset, row, color);
                }
            }
            None => painter.galley(pos, galley, color),
        }
        return;
    }
    let pixels_per_point = painter.pixels_per_point();
    let device_x = pos.x * pixels_per_point;
    if device_x == device_x.round() || galley.is_empty() {
        painter.galley(pos, galley, color);
        return;
    }
    let left = device_x.floor() / pixels_per_point;
    match relayout_with_leading_space(painter.ctx(), &galley, pos.x - left) {
        Some(relaid) => painter.galley(egui::pos2(left, pos.y), relaid, color),
        None => painter.galley(pos, galley, color),
    }
}

/// `galley` (a single row) laid out again with `leading` points of extra
/// space before its first glyph and its wrap width widened by as much,
/// keeping its vertical placement; `None` if it has several rows.
fn relayout_with_leading_space(
    ctx: &Context,
    galley: &Galley,
    leading: f32,
) -> Option<Arc<Galley>> {
    let [row] = galley.rows.as_slice() else {
        return None;
    };
    let mut job = (*galley.job).clone();
    job.sections.first_mut()?.leading_space += leading;
    job.wrap.max_width += leading;
    let mut relaid = Arc::unwrap_or_clone(ctx.fonts_mut(|fonts| fonts.layout_job(job)));
    let [relaid_row] = relaid.rows.as_mut_slice() else {
        return None;
    };
    let shift = row.pos.y - relaid_row.pos.y;
    relaid_row.pos.y += shift;
    relaid.mesh_bounds = relaid.mesh_bounds.translate(egui::vec2(0.0, shift));
    Some(Arc::new(relaid))
}

/// Every non-empty row of `galley` laid out again as a line of its own, with
/// the offset from the galley's origin to paint it at so its glyphs keep
/// their row's position and baseline. `None` when a row's glyphs do not
/// match the job's text (an elided row), or the job justifies its rows or
/// sets a minimum first-row height, which a single line cannot reproduce.
fn relayout_rows(ctx: &Context, galley: &Galley) -> Option<Vec<(egui::Vec2, Arc<Galley>)>> {
    let source = &galley.job;
    if source.justify || source.first_row_min_height > 0.0 {
        return None;
    }
    let mut chars = source.text.char_indices();
    let mut rows = Vec::with_capacity(galley.rows.len());
    for row in &galley.rows {
        let mut job = LayoutJob::default();
        // The section and leading space of the run being collected.
        let mut run: Option<(usize, f32, String)> = None;
        for glyph in &row.glyphs {
            let (byte, chr) = chars.next()?;
            if chr != glyph.chr {
                return None;
            }
            let section_index = source.sections.iter().position(|section| {
                section
                    .byte_range
                    .contains(&egui::epaint::text::ByteIndex(byte))
            })?;
            match &mut run {
                Some((current, _, text)) if *current == section_index => text.push(chr),
                _ => {
                    let section = &source.sections[section_index];
                    // egui puts a section's leading space before its first
                    // character; a row keeps its own indent.
                    let leading = if run.is_none() {
                        glyph.pos.x
                    } else if section.byte_range.start.0 == byte {
                        section.leading_space
                    } else {
                        0.0
                    };
                    if let Some((current, leading, text)) =
                        run.replace((section_index, leading, chr.to_string()))
                    {
                        job.append(&text, leading, source.sections[current].format.clone());
                    }
                }
            }
        }
        // Skip the row's newline.
        for _ in row.glyphs.len()..row.char_count_including_newline().0 {
            chars.next()?;
        }
        let Some((current, leading, text)) = run else {
            continue;
        };
        job.append(&text, leading, source.sections[current].format.clone());
        let relaid = ctx.fonts_mut(|fonts| fonts.layout_job(job));
        let relaid_row = relaid.rows.first()?;
        let offset = egui::vec2(row.pos.x, row.pos.y - relaid_row.pos.y);
        rows.push((offset, relaid));
    }
    Some(rows)
}

/// The sense of painted text: hover only, or focusable while a screen reader
/// is on, as [`egui::Label`] does.
fn text_sense(ui: &Ui) -> egui::Sense {
    if ui.memory(|memory| memory.options.screen_reader) {
        egui::Sense::focusable_noninteractive()
    } else {
        egui::Sense::hover()
    }
}

/// Allocate `size` for painted text read as the label `text`.
pub(crate) fn allocate_text(ui: &mut Ui, size: egui::Vec2, text: &str) -> (egui::Rect, Response) {
    let (rect, response) = ui.allocate_exact_size(size, text_sense(ui));
    describe_text(ui, &response, text);
    (rect, response)
}

/// Allocate exactly `rect` for painted text read as the label `text`.
pub(crate) fn allocate_text_at(ui: &mut Ui, rect: egui::Rect, text: &str) -> Response {
    let response = ui.allocate_rect(rect, text_sense(ui));
    describe_text(ui, &response, text);
    response
}

/// Sense `rect` (already inside allocated space, such as a count inside a
/// field) as painted text read as the label `text`, without moving the
/// cursor.
pub(crate) fn interact_text(ui: &Ui, rect: egui::Rect, id: egui::Id, text: &str) -> Response {
    let response = ui.interact(rect, id, text_sense(ui));
    describe_text(ui, &response, text);
    response
}

/// Paint `galley` with its top-left at `pos` inside space that is already
/// allocated (a row painted by hand), read as the label `text` under `id`,
/// without moving the cursor.
pub(crate) fn text_at(
    ui: &Ui,
    id: egui::Id,
    pos: egui::Pos2,
    galley: Arc<Galley>,
    text: &str,
    color: Color32,
) -> Response {
    let rect = egui::Rect::from_min_size(pos, galley.rect.size());
    let response = interact_text(ui, rect, id, text);
    paint_text(ui, &response, pos, galley, color);
    response
}

fn describe_text(ui: &Ui, response: &Response, text: &str) {
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), text));
}

/// Paint `galley` at `pos` ([`paint_galley`]) as the text of `response`'s
/// label, exposing it as accessibility text runs the way [`egui::Label`]
/// does. Nothing is painted or exposed outside the visible area.
pub(crate) fn paint_text(
    ui: &Ui,
    response: &Response,
    pos: egui::Pos2,
    galley: Arc<Galley>,
    color: Color32,
) {
    paint_text_as(
        ui,
        response,
        pos,
        galley,
        color,
        egui::accesskit::Role::Label,
    );
}

/// [`paint_text`] for a widget whose own role is `role` (a button, link or
/// tab whose words are its label), so the text runs do not turn it into a
/// label.
pub(crate) fn paint_text_as(
    ui: &Ui,
    response: &Response,
    pos: egui::Pos2,
    galley: Arc<Galley>,
    color: Color32,
    role: egui::accesskit::Role,
) {
    if !ui.is_rect_visible(galley.rect.translate(pos.to_vec2())) {
        return;
    }
    expose_text_runs_as(ui, response, pos, &galley, role);
    paint_galley(ui.painter(), pos, galley, color);
}

/// [`paint_text`] for selectable text (a value part of which can be
/// copied): `response` must sense clicks and drags. A single row keeps its
/// exact sub-pixel position, like [`paint_galley`].
pub(crate) fn paint_selectable_text(
    ui: &Ui,
    response: &Response,
    pos: egui::Pos2,
    galley: Arc<Galley>,
    color: Color32,
) {
    if !ui.is_rect_visible(galley.rect.translate(pos.to_vec2())) {
        return;
    }
    let original = (pos, galley.clone());
    let pixels_per_point = ui.pixels_per_point();
    let left = (pos.x * pixels_per_point).floor() / pixels_per_point;
    let (pos, galley) = match relayout_with_leading_space(ui.ctx(), &galley, pos.x - left) {
        Some(relaid) if left != pos.x => (egui::pos2(left, pos.y), relaid),
        _ => (pos, galley),
    };
    egui::text_selection::LabelSelectionState::label_text_selection(
        ui,
        response,
        pos,
        galley,
        color,
        egui::Stroke::NONE,
    );
    // egui exposes the runs at the glyphs' position; put them back on the CSS line box, as
    // every other text does.
    expose_text_runs(ui, response, original.0, &original.1);
}

/// Expose `galley`, painted at `pos`, as the accessibility text runs of
/// `response`'s label. The runs cover the CSS line boxes: the half-leading
/// shift that moved the rows' glyphs down ([`TextRole::galley`]) is undone.
pub(crate) fn expose_text_runs(ui: &Ui, response: &Response, pos: egui::Pos2, galley: &Galley) {
    expose_text_runs_as(ui, response, pos, galley, egui::accesskit::Role::Label);
}

/// [`expose_text_runs`] for a widget whose own role is `role` (a checkbox
/// that is read as its label).
pub(crate) fn expose_text_runs_as(
    ui: &Ui,
    response: &Response,
    pos: egui::Pos2,
    galley: &Galley,
    role: egui::accesskit::Role,
) {
    let shift = galley.rows.first().map_or(0.0, |row| row.pos.y);
    let pos = pos - egui::vec2(0.0, shift);
    egui::text_selection::accesskit_text::update_accesskit_for_text_widget(
        ui.ctx(),
        response.id,
        None,
        role,
        egui::emath::TSTransform::from_translation(pos.to_vec2()),
        galley,
    );
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
/// Mono row · 12/20 · 400. Mono values in 13/20 rows: UIDs and IPs in a
/// property grid, completion options, "Ln n" in the Problems panel.
pub const MONO_ROW: TextRole = MONO.with_size(12.0, 20.0);
/// Mono row strong · 12/20 · 500. File names and completion names.
pub const MONO_ROW_STRONG: TextRole = MONO_ROW.with_weight(Weight::Medium);
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
/// Button · 13/20 · 600 (`.btn`). Labels of medium buttons and links.
pub const BUTTON: TextRole = UI_STRONG;
/// UI strong · 13/20 · 600. Titles of callouts, toasts, empty states and card
/// headers (`.w6`).
pub const UI_STRONG: TextRole = UI.with_weight(Weight::SemiBold);
/// Small button · 12/16 · 600 (`.btn.sm`).
pub const BUTTON_SM: TextRole = META.with_weight(Weight::SemiBold);
/// Large button · 14/20 · 600 (`.btn.lg`).
pub const BUTTON_LG: TextRole = BODY_STRONG;
/// Small segment · 12/20 · 500 (`.seg.sm .seg-i`).
pub const SEGMENT_SM: TextRole = UI_CONTROL.with_size(12.0, 20.0);

/// Small field · 12/20 · 400 (`.inp.sm`, `.sel` at 28 points).
pub const FIELD_SM: TextRole = UI.with_size(12.0, 20.0);
/// Field label · 12/16 · 600 (`.field > .lab`).
pub const FIELD_LABEL: TextRole = META.with_weight(Weight::SemiBold);

/// Status pill · 12/16 · 600 (`.pill`).
pub const PILL: TextRole = META.with_weight(Weight::SemiBold);
/// Badge · 11/16 · 600 (`.kind`). Resource kind badges.
pub const BADGE: TextRole = TextRole::new(Typeface::Figtree, Weight::SemiBold, 11.0, 16.0);
/// Count · 10/16 · 700 (`.count`). Count badges.
pub const COUNT: TextRole = TextRole::new(Typeface::Figtree, Weight::Bold, 10.0, 16.0);
/// Chip · Plex Mono 11.5/16 · 400 (`.chip`). Key–value chips.
pub const CHIP: TextRole = MONO.with_size(11.5, 16.0);
/// Avatar initials · Bricolage 13/16 · 700 (`.av`).
pub const AVATAR: TextRole = TextRole::new(Typeface::Display, Weight::Bold, 13.0, 16.0);

// C16
/// Meta row · 12/20 · 400. Meta text in a row of 13/20 controls (`.tmeta`,
/// group headers), which keeps the body's 20 point line box.
pub const META_ROW: TextRole = META.with_size(12.0, 20.0);
// C20
/// Log · Plex Mono 12.5/22 · 400 (`.lpanel`). Log viewer lines.
pub const LOG: TextRole = MONO.with_size(12.5, 22.0);
// C18–C20
/// Caption small · condensed 11/14 · 500, upper case. Readout and tile
/// captions (`.ro .k`, `.ctile .mt`, `.lhead .ls`).
pub const CAPTION_SM: TextRole = CAPTION.with_size(11.0, 14.0);
/// Mono hint · Plex Mono 11/20 · 400. Units, counts and hints set in a
/// 13/20 row (`.ro .v small`, `.ctile .nn`).
pub const MONO_HINT: TextRole = MONO.with_size(11.0, 20.0);
/// Mono count · Plex Mono 10.5/20 · 400. Small counts in a row (`.si .c`,
/// `.srcp .n`).
pub const MONO_COUNT: TextRole = MONO.with_size(10.5, 20.0);

/// Every named role, for exhaustive checks.
pub const ALL: [TextRole; 28] = [
    DISPLAY,
    HEADING,
    TITLE,
    BODY,
    UI,
    META,
    CAPTION,
    MONO,
    MONO_SMALL,
    MONO_ROW,
    MONO_ROW_STRONG,
    KEY,
    DIALOG_TITLE,
    CARD_TITLE,
    BODY_STRONG,
    UI_CONTROL,
    BUTTON,
    UI_STRONG,
    BUTTON_SM,
    BUTTON_LG,
    SEGMENT_SM,
    FIELD_SM,
    FIELD_LABEL,
    PILL,
    BADGE,
    COUNT,
    CHIP,
    AVATAR,
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
    fn tabular_roles_use_tabular_figures() {
        assert_eq!(
            META.tabular().family_name(),
            family::FIGTREE_TABULAR_REGULAR
        );
        assert_eq!(
            UI.with_weight(Weight::SemiBold).tabular().family_name(),
            family::FIGTREE_TABULAR_SEMIBOLD
        );
        // Derived roles keep the flag; mono figures are tabular already.
        assert!(META.tabular().with_weight(Weight::Bold).is_tabular());
        assert_eq!(MONO.tabular().family_name(), MONO.family_name());

        let ctx = Context::default();
        fonts::install(&ctx);
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
        let width = |role: TextRole, text: &str| {
            role.galley(&ctx, text, Color32::WHITE, f32::INFINITY)
                .rect
                .width()
        };
        assert_ne!(width(META, "111"), width(META, "000"));
        assert_eq!(width(META.tabular(), "111"), width(META.tabular(), "000"));
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

    /// The text shapes painted by `paint` in a pass at `pixels_per_point`
    /// (after a warm-up pass at that scale).
    fn painted_texts(
        ctx: &Context,
        pixels_per_point: f32,
        mut paint: impl FnMut(&Ui),
    ) -> Vec<egui::epaint::TextShape> {
        run_at(ctx, pixels_per_point, |_| {});
        let mut input = egui::RawInput::default();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(pixels_per_point);
        let mut output = ctx.run_ui(input, |ui| paint(ui));
        output.textures_delta.clear();
        output
            .shapes
            .into_iter()
            .filter_map(|clipped| match clipped.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    fn configured() -> Context {
        let ctx = Context::default();
        crate::design::theme::configure_default(&ctx);
        ctx
    }

    #[test]
    fn galleys_at_whole_pixels_pass_through_unchanged() {
        let ctx = configured();
        for (pixels_per_point, x) in [(1.0, 10.0), (1.5, 10.0), (2.0, 10.5)] {
            let mut original = None;
            let texts = painted_texts(&ctx, pixels_per_point, |ui| {
                let galley = UI.galley(ui.ctx(), "Running", Color32::WHITE, f32::INFINITY);
                original = Some(galley.clone());
                paint_galley(ui.painter(), egui::pos2(x, 4.0), galley, Color32::WHITE);
            });
            assert_eq!(texts.len(), 1);
            assert_eq!(texts[0].pos, egui::pos2(x, 4.0));
            assert!(Arc::ptr_eq(&texts[0].galley, &original.expect("painted")));
        }
    }

    #[test]
    fn single_row_galleys_keep_their_sub_pixel_offset() {
        let ctx = configured();
        // (scale, x, expected snapped left edge)
        for (pixels_per_point, x, left) in [
            (1.0, 10.4, 10.0),
            (1.5, 10.5, 10.0),
            (2.0, 10.25, 10.0),
            (1.0, -3.4, -4.0),
        ] {
            let mut original = None;
            let texts = painted_texts(&ctx, pixels_per_point, |ui| {
                let galley = UI.galley(ui.ctx(), "Running", Color32::WHITE, f32::INFINITY);
                original = Some(galley.clone());
                paint_galley(ui.painter(), egui::pos2(x, 4.0), galley, Color32::WHITE);
            });
            let original = original.expect("painted");
            let painted = &texts[0];
            assert_eq!(painted.pos, egui::pos2(left, 4.0), "at {pixels_per_point}");
            assert_eq!(painted.galley.text(), original.text());
            assert_eq!(
                painted.galley.rows[0].pos.y, original.rows[0].pos.y,
                "keeps the baseline"
            );
            let grown = painted.galley.rows[0].rect().width() - original.rows[0].rect().width();
            assert!(
                (grown - (x - left)).abs() < 0.05,
                "grew {grown} at {pixels_per_point}"
            );
            // egui snaps glyph quads to pixels and rasterizes each glyph at
            // a quarter-pixel bin: the offset shows as other glyph bitmaps.
            let first_uv = |galley: &Galley| galley.rows[0].visuals.mesh.vertices[0].uv;
            assert_ne!(
                first_uv(&painted.galley),
                first_uv(&original),
                "at {pixels_per_point}"
            );
        }
    }

    #[test]
    fn wrapped_rows_are_painted_as_lines_of_their_own() {
        let ctx = configured();
        let text = "Pressed-state controls for view options share one frame";
        let mut original = None;
        let texts = painted_texts(&ctx, 1.0, |ui| {
            let galley = UI.galley(ui.ctx(), text, Color32::WHITE, 120.0);
            assert!(galley.rows.len() > 2, "wraps");
            original = Some(galley.clone());
            paint_galley(ui.painter(), egui::pos2(10.0, 4.0), galley, Color32::WHITE);
        });
        let original = original.expect("painted");
        assert_eq!(texts.len(), original.rows.len(), "one painted line per row");
        for (painted, row) in texts.iter().zip(&original.rows) {
            let line = UI.galley(
                &ctx,
                &row.glyphs.iter().map(|glyph| glyph.chr).collect::<String>(),
                Color32::WHITE,
                f32::INFINITY,
            );
            let painted_xs: Vec<f32> = painted.galley.rows[0]
                .glyphs
                .iter()
                .map(|g| g.pos.x)
                .collect();
            let line_xs: Vec<f32> = line.rows[0].glyphs.iter().map(|g| g.pos.x).collect();
            assert_eq!(
                painted_xs, line_xs,
                "glyphs round from the line's own start"
            );
            let baseline = |galley: &Galley, row: usize| {
                galley.rows[row].pos.y + galley.rows[row].glyphs[0].pos.y
            };
            assert_eq!(
                painted.pos.y + baseline(&painted.galley, 0),
                4.0 + row.pos.y + row.glyphs[0].pos.y,
                "the line keeps its row's baseline"
            );
        }
    }

    #[test]
    fn multi_section_rows_keep_their_formats() {
        let ctx = configured();
        let texts = painted_texts(&ctx, 1.0, |ui| {
            let mut job = LayoutJob::default();
            job.append("key=", 0.0, UI.text_format(ui.ctx(), Color32::RED));
            job.append(
                "value\nsecond",
                0.0,
                UI.text_format(ui.ctx(), Color32::GREEN),
            );
            let galley = UI.galley_from_job(ui.ctx(), job);
            paint_galley(ui.painter(), egui::pos2(10.0, 4.0), galley, Color32::WHITE);
        });
        assert_eq!(texts.len(), 2);
        assert_eq!(
            texts[0].galley.job.sections.len(),
            2,
            "both formats survive"
        );
        assert_eq!(texts[0].galley.job.text, "key=value");
        assert_eq!(texts[1].galley.job.text, "second");
    }

    /// The absolute x of every painted glyph, row by row.
    fn painted_glyph_xs(texts: &[egui::epaint::TextShape]) -> Vec<Vec<f32>> {
        texts
            .iter()
            .flat_map(|text| {
                text.galley.rows.iter().map(move |row| {
                    row.glyphs
                        .iter()
                        .map(|glyph| text.pos.x + row.pos.x + glyph.pos.x)
                        .collect()
                })
            })
            .collect()
    }

    #[test]
    fn elided_multi_row_galleys_fall_back_to_egui_painting() {
        let ctx = configured();
        let texts = painted_texts(&ctx, 1.0, |ui| {
            let text = "A long message that wraps over several rows and is cut off after two";
            let mut job = UI.layout_job(ui.ctx(), text, Color32::WHITE, 120.0);
            job.wrap.max_rows = 2;
            let galley = UI.galley_from_job(ui.ctx(), job);
            assert!(galley.elided);
            paint_galley(ui.painter(), egui::pos2(10.0, 4.0), galley, Color32::WHITE);
        });
        assert_eq!(texts.len(), 1, "painted as one galley");
        let last = texts[0].galley.rows[1].glyphs.last().expect("glyphs").chr;
        assert_eq!(last, '…');
    }

    #[test]
    fn aligned_and_indented_rows_keep_their_positions() {
        let ctx = configured();
        for (halign, leading_space) in [(egui::Align::Center, 0.0), (egui::Align::LEFT, 12.0)] {
            let mut original = None;
            let texts = painted_texts(&ctx, 1.0, |ui| {
                let mut job = UI.layout_job(
                    ui.ctx(),
                    "Centred text over a few rows",
                    Color32::WHITE,
                    90.0,
                );
                job.halign = halign;
                job.sections[0].leading_space = leading_space;
                let galley = UI.galley_from_job(ui.ctx(), job);
                original = Some(galley.clone());
                paint_galley(ui.painter(), egui::pos2(10.0, 4.0), galley, Color32::WHITE);
            });
            let original = original.expect("painted");
            assert!(original.rows.len() > 1, "wraps");
            assert_eq!(texts.len(), original.rows.len());
            let expected: Vec<Vec<f32>> = original
                .rows
                .iter()
                .map(|row| {
                    row.glyphs
                        .iter()
                        .map(|glyph| 10.0 + row.pos.x + glyph.pos.x)
                        .collect()
                })
                .collect();
            for (painted, expected) in painted_glyph_xs(&texts).iter().zip(&expected) {
                assert_eq!(painted.len(), expected.len());
                for (painted, expected) in painted.iter().zip(expected) {
                    assert!(
                        (painted - expected).abs() <= 1.0,
                        "{halign:?}: {painted} vs {expected}"
                    );
                }
            }
        }
    }

    #[test]
    fn blank_lines_and_multibyte_text_map_back_to_their_rows() {
        let ctx = configured();
        let text = "Größe naïve 日本語\n\nlast";
        let texts = painted_texts(&ctx, 1.0, |ui| {
            let galley = UI.galley(ui.ctx(), text, Color32::WHITE, f32::INFINITY);
            assert_eq!(galley.rows.len(), 3);
            paint_galley(ui.painter(), egui::pos2(10.0, 4.0), galley, Color32::WHITE);
        });
        let lines: Vec<&str> = texts
            .iter()
            .map(|text| text.galley.job.text.as_str())
            .collect();
        assert_eq!(
            lines,
            ["Größe naïve 日本語", "last"],
            "the blank row paints nothing"
        );
        assert!(
            texts[1].pos.y + texts[1].galley.rows[0].pos.y
                > texts[0].pos.y + texts[0].galley.rows[0].pos.y + UI.line_height(),
            "the last row stays below the blank one"
        );
    }

    #[test]
    fn wrapped_rows_at_a_fractional_x_place_each_row_at_it() {
        let ctx = configured();
        let texts = painted_texts(&ctx, 1.0, |ui| {
            let galley = UI.galley(
                ui.ctx(),
                "Wrapped over a couple of rows",
                Color32::WHITE,
                80.0,
            );
            paint_galley(ui.painter(), egui::pos2(10.4, 4.0), galley, Color32::WHITE);
        });
        assert!(texts.len() > 1);
        for text in &texts {
            assert_eq!(text.pos.x, 10.0, "snapped left edge");
            let line = UI.galley(&ctx, &text.galley.job.text, Color32::WHITE, f32::INFINITY);
            let grown = text.galley.rows[0].rect().width() - line.rows[0].rect().width();
            assert!(
                (grown - 0.4).abs() < 0.05,
                "the fraction leads the row: {grown}"
            );
        }
    }

    #[test]
    fn elided_status_labels_stay_elided_at_fractional_offsets() {
        let ctx = configured();
        let text = "CrashLoopBackOff: back-off 5m0s restarting failed container";
        let wrap_width = 99.0;
        for (pixels_per_point, x) in [(1.0, 8.5), (1.5, 8.5), (2.0, 8.25), (1.0, -3.4)] {
            let texts = painted_texts(&ctx, pixels_per_point, |ui| {
                let mut job = UI.layout_job(ui.ctx(), text, Color32::WHITE, wrap_width);
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
                let galley = UI.galley_from_job(ui.ctx(), job);
                assert!(galley.elided);
                paint_galley(ui.painter(), egui::pos2(x, 4.0), galley, Color32::WHITE);
            });
            let painted = &texts[0];
            assert!(painted.galley.elided);
            let last = painted.galley.rows[0].glyphs.last().expect("glyphs").chr;
            assert_eq!(last, '…', "at {pixels_per_point}");
            let bounds = painted.galley.mesh_bounds.translate(painted.pos.to_vec2());
            assert!(
                bounds.right() <= x + wrap_width + 0.5,
                "{bounds:?} overflows {wrap_width} from {x} at {pixels_per_point}"
            );
        }
    }

    #[test]
    fn labels_are_read_as_their_text_with_text_runs() {
        use egui_kittest::kittest::{NodeT as _, Queryable as _};
        let mut harness = egui_kittest::Harness::builder().build_ui(|ui| {
            ui.set_width(120.0);
            BODY.label(ui, "Container start", Color32::WHITE);
            BODY.label(
                ui,
                "A paragraph long enough to wrap onto rows",
                Color32::WHITE,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        let single = harness.get_by_label("Container start");
        assert_eq!(single.accesskit_node().role(), egui::accesskit::Role::Label);
        assert_eq!(single.value().as_deref(), Some("Container start"));
        assert_eq!(single.rect().height(), BODY.line_height());
        let wrapped = harness.get_by_label("A paragraph long enough to wrap onto rows");
        let rows = wrapped.rect().height() / BODY.line_height();
        assert!(rows >= 2.0, "wraps");
        let runs = wrapped.accesskit_node().children().count();
        assert_eq!(runs as f32, rows, "one text run per row");
    }
}
