//! Capsules: status pills, kind and count badges, and chips (C01 "Status
//! pills" and "Badges & indicators", C13 chips), plus the well frame of
//! pill trays.
//!
//! Every capsule is a fully rounded box of a fixed height holding an
//! optional status lamp and one run of text, padded on both sides. They are
//! read-only: none of them is clickable.

use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{
    Color32, Frame, Galley, Pos2, Rect, Response, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use crate::design::theme::{Theme, ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space};
use crate::loading::Spinner;
use crate::status::{lamp_rect, paint_status_dot};
use crate::tooltip::TooltipExt;

/// The shared geometry of every capsule.
pub(crate) struct Capsule {
    pub(crate) height: f32,
    pub(crate) padding_x: f32,
    pub(crate) min_width: f32,
    pub(crate) fill: Color32,
    pub(crate) centred: bool,
    pub(crate) dot: Option<Tone>,
    /// A small spinner leads instead of a lamp (work in progress).
    pub(crate) spinner: bool,
    pub(crate) text: Arc<Galley>,
}

impl Capsule {
    /// A [`size::PILL_HEIGHT`] capsule (pill, kind badge, chip) on `fill`.
    pub(crate) fn pill(fill: Color32, text: Arc<Galley>) -> Self {
        Self {
            height: size::PILL_HEIGHT,
            padding_x: size::PILL_PADDING_X,
            min_width: 0.0,
            fill,
            centred: false,
            dot: None,
            spinner: false,
            text,
        }
    }

    /// The width of the lamp and its gap before the text, if any.
    fn dot_width(&self) -> f32 {
        if self.spinner {
            return size::SPINNER_SM + size::STATUS_DOT_GAP;
        }
        self.dot
            .map_or(0.0, |_| size::STATUS_DOT + size::STATUS_DOT_GAP)
    }

    fn content_width(&self) -> f32 {
        self.dot_width() + self.text.rect.width()
    }

    fn size(&self) -> Vec2 {
        let width = (self.content_width() + 2.0 * self.padding_x).max(self.min_width);
        vec2(width, self.height)
    }

    /// Distance from the capsule's top to its text baseline.
    fn baseline(&self) -> f32 {
        let row = &self.text.rows[0];
        let glyph_baseline = row.glyphs.first().map_or(0.0, |glyph| glyph.pos.y);
        (self.height - self.text.rect.height()) / 2.0 + row.pos.y + glyph_baseline
    }

    fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) = type_scale::allocate_text(ui, self.size(), self.text.text());
        self.paint(ui, rect, Some(&response));
        response
    }

    /// Show the capsule at the cursor with its text baseline `baseline`
    /// points below the top of the row (see [`TextRole::centred_baseline`]).
    fn show_on_baseline(self, ui: &mut Ui, baseline: f32) -> Response {
        let row = ui.cursor();
        let top = row.top() + baseline - self.baseline();
        let rect = Rect::from_min_size(pos2(row.left(), top), self.size());
        let response = type_scale::allocate_text_at(ui, rect, self.text.text());
        self.paint(ui, rect, Some(&response));
        response
    }

    /// Paint into `rect`; with a `response`, the text is exposed as its
    /// label's accessibility text runs.
    pub(crate) fn paint(self, ui: &Ui, rect: Rect, response: Option<&Response>) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter();
        painter.rect_filled(rect, radius::FULL, self.fill);
        let left = if self.centred {
            rect.center().x - self.content_width() / 2.0
        } else {
            rect.left() + self.padding_x
        };
        if self.spinner {
            let lead = Rect::from_center_size(
                pos2(left + size::SPINNER_SM / 2.0, rect.center().y),
                Vec2::splat(size::SPINNER_SM),
            );
            Spinner::new().size(size::SPINNER_SM).paint(ui, lead);
        } else if let Some(tone) = self.dot {
            paint_status_dot(
                painter,
                &ui.tokens(),
                lamp_rect(left, rect.center().y),
                tone,
            );
        }
        let text_pos = Pos2::new(
            left + self.dot_width(),
            rect.center().y - self.text.rect.height() / 2.0,
        );
        match response {
            Some(response) => {
                type_scale::paint_text(ui, response, text_pos, self.text, Color32::PLACEHOLDER);
            }
            None => type_scale::paint_galley(painter, text_pos, self.text, Color32::PLACEHOLDER),
        }
    }
}

fn galley(ui: &Ui, role: TextRole, text: &str, color: Color32) -> Arc<Galley> {
    role.galley(ui.ctx(), text, color, f32::INFINITY)
}

/// A status pill (`.pill`) for live state in headers: a lamp and words on
/// the tone's tint, "● Following", "● Modified".
///
/// Pills share the tone mapping of [`crate::StatusLabel`]. A pill whose
/// words change over time should be [`Pill::live`], so nothing to its right
/// jumps when it does.
#[must_use = "call `show` to add the pill"]
pub struct Pill<'a> {
    text: &'a str,
    tone: Tone,
    dot: bool,
    busy: bool,
    min_width: f32,
}

impl<'a> Pill<'a> {
    /// `text` with a lamp, in `tone`.
    pub fn new(text: &'a str, tone: Tone) -> Self {
        Self {
            text,
            tone,
            dot: true,
            busy: false,
            min_width: 0.0,
        }
    }

    /// Show a small spinner in place of the lamp: the state is work in
    /// progress ("Spooling…").
    pub fn busy(mut self) -> Self {
        self.busy = true;
        self
    }

    /// Leave out the lamp.
    pub fn without_dot(mut self) -> Self {
        self.dot = false;
        self
    }

    /// Reserve [`size::LIVE_PILL_MIN_WIDTH`], for a pill whose words change
    /// (Following → Reconnecting…). Longer words still grow it.
    pub fn live(mut self) -> Self {
        self.min_width = size::LIVE_PILL_MIN_WIDTH;
        self
    }

    // C14
    /// The pill's size, for laying out a row around it.
    pub fn size(&self, ui: &Ui) -> Vec2 {
        self.capsule(ui).size()
    }

    fn capsule(&self, ui: &Ui) -> Capsule {
        let colors = ui.tokens().tone(self.tone);
        Capsule {
            min_width: self.min_width,
            // A live pill wider than its words keeps them centred.
            centred: self.min_width > 0.0,
            dot: self.dot.then_some(self.tone),
            spinner: self.busy,
            ..Capsule::pill(
                colors.tint,
                galley(ui, type_scale::PILL, self.text, colors.text),
            )
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        self.capsule(ui).show(ui)
    }

    /// Show the pill in a row of text with its words on the row's
    /// `baseline`, `baseline` points below the row's top (see
    /// [`TextRole::centred_baseline`] and [`TextRole::label_on_baseline`]).
    pub fn show_on_baseline(self, ui: &mut Ui, baseline: f32) -> Response {
        self.capsule(ui).show_on_baseline(ui, baseline)
    }
}

fn kind_capsule(ui: &Ui, kind: &str) -> Capsule {
    let theme = ui.tokens();
    Capsule::pill(
        theme.accent.fill,
        galley(ui, type_scale::BADGE, kind, theme.text.on_fill),
    )
}

/// The width of [`kind_badge`] for `kind`, for laying out a title next to it.
pub fn kind_badge_width(ui: &Ui, kind: &str) -> f32 {
    kind_capsule(ui, kind).size().x
}

/// The resource kind badge (`.kind`) before a blade title:
/// "Deployment" on the accent fill. Never clickable.
pub fn kind_badge(ui: &mut Ui, kind: &str) -> Response {
    kind_capsule(ui, kind).show(ui)
}

// C19
/// A [`kind_badge`] on `fill`, the kind's category colour (`.kind` with
/// `background: var(--category-n)`), its text still `text-on-fill`.
pub fn kind_badge_on(ui: &mut Ui, kind: &str, fill: Color32) -> Response {
    Capsule {
        fill,
        ..kind_capsule(ui, kind)
    }
    .show(ui)
}

// C15
/// A [`kind_badge`] [`size::COMPLETION_KIND_BADGE`] tall, in a completion's
/// documentation.
pub fn small_kind_badge(ui: &mut Ui, kind: &str) -> Response {
    Capsule {
        height: size::COMPLETION_KIND_BADGE,
        ..kind_capsule(ui, kind)
    }
    .show(ui)
}

/// The largest count a [`CountBadge`] shows; larger counts read "99+".
pub const COUNT_BADGE_CAP: usize = 99;

/// A count badge (`.count`): red for unread problems, accent for
/// informational counts, neutral for plain counts in tabs and panels.
///
/// A badge shows any count it is given, zero included; hide it at the call
/// site when nothing is unread.
#[must_use = "call `show` or `paint` to draw the badge"]
pub struct CountBadge {
    count: usize,
    tone: Tone,
    on_pill: bool,
}

impl CountBadge {
    /// A badge for unread problems ([`Tone::Danger`]).
    pub fn new(count: usize) -> Self {
        Self {
            count,
            tone: Tone::Danger,
            on_pill: false,
        }
    }

    /// Show the count in `tone`: [`Tone::Danger`] (the default),
    /// [`Tone::Accent`] or [`Tone::Neutral`] ([`Theme::solid`] colours).
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Show a neutral count on a selected `pill-on-bg` pill (a selected
    /// tab): `tint-on-fill` behind `pill-on-text`.
    pub fn on_pill(mut self) -> Self {
        self.on_pill = true;
        self
    }

    /// The badge's words: the count, capped at "99+".
    pub fn text(&self) -> String {
        if self.count > COUNT_BADGE_CAP {
            format!("{COUNT_BADGE_CAP}+")
        } else {
            self.count.to_string()
        }
    }

    fn capsule(&self, ui: &Ui) -> Capsule {
        let theme = ui.tokens();
        let (fill, color) = if self.on_pill {
            (theme.on_fill.tint, theme.components.pill_on_text)
        } else {
            theme.solid(self.tone)
        };
        Capsule {
            height: size::COUNT_BADGE,
            padding_x: size::COUNT_BADGE_PADDING_X,
            min_width: size::COUNT_BADGE,
            fill,
            centred: true,
            dot: None,
            spinner: false,
            text: galley(ui, type_scale::COUNT, &self.text(), color),
        }
    }

    /// The badge's size, for placing it with [`Self::paint`].
    pub fn size(&self, ui: &Ui) -> Vec2 {
        self.capsule(ui).size()
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        self.capsule(ui).show(ui)
    }

    /// Paint the badge into `rect` (sized by [`Self::size`]) on top of
    /// another control, such as a rail button, cut out of it by a ring of
    /// [`size::COUNT_BADGE_RING`] in `surface`, the colour behind the
    /// control (`box-shadow: 0 0 0 2px`). Include the count in the control's
    /// own accessible label.
    pub fn paint(self, ui: &Ui, rect: Rect, surface: Color32) {
        ui.painter().rect_stroke(
            rect,
            radius::FULL,
            Stroke::new(size::COUNT_BADGE_RING, surface),
            StrokeKind::Outside,
        );
        self.capsule(ui).paint(ui, rect, None);
    }
}

/// A key–value chip (`.chip`) in Plex Mono on the well: "53/TCP", or
/// "k8s-app=" (muted) followed by "coredns".
#[must_use = "call `show` to add the chip"]
pub struct Chip<'a> {
    key: Option<&'a str>,
    value: &'a str,
    copy_label: Option<&'a str>,
    copy_text: Option<&'a str>,
    quiet_copy: bool,
    forced_state: Option<crate::interaction::ControlState>,
}

impl<'a> Chip<'a> {
    /// A chip showing `value`.
    pub fn new(value: &'a str) -> Self {
        Self {
            key: None,
            value,
            copy_label: None,
            copy_text: None,
            quiet_copy: false,
            forced_state: None,
        }
    }

    /// Prefix the value with `key` in muted text (include the separator,
    /// for example `"app="`).
    pub fn key(mut self, key: &'a str) -> Self {
        self.key = Some(key);
        self
    }

    /// Make the chip a copy control for its value (see [`crate::Copyable`]),
    /// read as "Copy `label` `key``value`" ("Copy Args -conf"): it reserves
    /// room for a copy icon that shows on hover, and a click copies the value.
    pub fn copyable(mut self, label: &'a str) -> Self {
        self.copy_label = Some(label);
        self
    }

    /// [`Self::copyable`] without the room for a copy icon: the chip keeps its size,
    /// a click copies and the tooltip says "Copy".
    pub fn copyable_quietly(mut self, label: &'a str) -> Self {
        self.copy_label = Some(label);
        self.quiet_copy = true;
        self
    }

    /// Copy `text` instead of the value (for example `key=value` for a
    /// label chip); only used with [`Self::copyable`].
    pub fn copy_as(mut self, text: &'a str) -> Self {
        self.copy_text = Some(text);
        self
    }

    /// Paint a copyable chip in `state` instead of the pointer's (design
    /// boards and snapshots).
    pub fn force_state(mut self, state: crate::interaction::ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let role = type_scale::CHIP;
        // One run of text; the key and value are flex items `gap: 4px` apart.
        let mut job = LayoutJob::default();
        let mut value_gap = 0.0;
        if let Some(key) = self.key {
            job.append(key, 0.0, role.text_format(ui.ctx(), theme.text.muted));
            value_gap = space::S4;
        }
        job.append(
            self.value,
            value_gap,
            role.text_format(ui.ctx(), theme.text.primary),
        );
        // C13: one chip truncates at its maximum width, with a tooltip.
        let icon_slot = self
            .copy_label
            .filter(|_| !self.quiet_copy)
            .map_or(0.0, |_| space::S4 + size::ICON_SM);
        job.wrap.max_width = size::CHIP_MAX_WIDTH - 2.0 * size::PILL_PADDING_X - icon_slot;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let text = role.galley_from_job(ui.ctx(), job);
        let words = format!("{}{}", self.key.unwrap_or_default(), self.value);
        let full = text.elided.then(|| words.clone());
        let Some(label) = self.copy_label else {
            let response = Capsule::pill(theme.components.well_bg, text).show(ui);
            return match full {
                Some(full) => response.tooltip(full),
                None => response,
            };
        };
        let capsule = Capsule::pill(theme.components.well_bg, text);
        let size = capsule.size() + vec2(icon_slot, 0.0);
        // Chips of one group share their label; name each by its words too.
        let name = format!("Copy {label} {words}");
        crate::copy::copy_button(
            ui,
            size,
            &name,
            self.copy_text.unwrap_or(self.value),
            self.forced_state,
            |ui, rect, active| {
                let label = type_scale::allocate_text_at(ui, rect, &words);
                let fill = if active {
                    theme.control.fill_hover
                } else {
                    capsule.fill
                };
                Capsule { fill, ..capsule }.paint(ui, rect, Some(&label));
                if active && !self.quiet_copy && ui.is_rect_visible(rect) {
                    crate::copy::paint_copy_icon(ui, rect, size::PILL_PADDING_X, rect.height());
                }
                full.or_else(|| self.quiet_copy.then(|| format!("Copy {words}")))
            },
        )
    }
}

/// Where an indicator (a count badge, a lamp, a check badge) sits on a
/// control: a `size` box in `corner` of `anchor`, `inset` points in from its
/// edges (negative to overhang them). Centred axes ignore the inset.
pub fn corner_rect(anchor: Rect, corner: egui::Align2, size: Vec2, inset: Vec2) -> Rect {
    let place = |align: egui::Align, min: f32, max: f32, extent: f32, inset: f32| match align {
        egui::Align::Min => min + inset,
        egui::Align::Center => (min + max - extent) / 2.0,
        egui::Align::Max => max - inset - extent,
    };
    let left = place(corner.x(), anchor.left(), anchor.right(), size.x, inset.x);
    let top = place(corner.y(), anchor.top(), anchor.bottom(), size.y, inset.y);
    Rect::from_min_size(pos2(left, top), size)
}

// C13
/// Gap between chips, along and across their rows.
pub const CHIP_GAP: f32 = space::S6;

/// Lay out `add_chips` left to right, wrapping onto new rows, chips
/// [`CHIP_GAP`] apart in both directions (`.row` with `flex-wrap: wrap`).
pub fn chip_flow<R>(ui: &mut Ui, add_chips: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(CHIP_GAP);
        add_chips(ui)
    })
    .inner
}

/// A [`chip_flow`] led by a caption [`size::CHIP_ROW_LABEL`] wide, centred
/// on the first row of chips: "COMMAND  /coredns" (C13 "Chips & code").
pub fn labelled_chips<R>(ui: &mut Ui, label: &str, add_chips: impl FnOnce(&mut Ui) -> R) -> R {
    labelled_chips_with_label_width(ui, label, size::CHIP_ROW_LABEL, add_chips)
}

/// A [`labelled_chips`] row whose caption is `label_width` wide (the
/// inspector's container rows are narrower than the data-display board's).
pub fn labelled_chips_with_label_width<R>(
    ui: &mut Ui,
    label: &str,
    label_width: f32,
    add_chips: impl FnOnce(&mut Ui) -> R,
) -> R {
    chip_flow(ui, |ui| {
        let size = vec2(label_width, size::PILL_HEIGHT);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let role = type_scale::CAPTION;
        let color = ui.tokens().text.muted;
        let galley = role.galley(ui.ctx(), label, color, size.x);
        let pos = pos2(rect.left(), rect.center().y - galley.rect.height() / 2.0);
        type_scale::text_at(
            ui,
            ui.id().with(("chip-label", label)),
            pos,
            galley,
            label,
            color,
        );
        add_chips(ui)
    })
}

/// The frame of a pill tray (`.grp`, `.seg`): the fully rounded `well-bg`
/// well that toggle groups and segmented controls sit in. Add the tray's
/// inner padding at the call site.
pub fn well_frame(theme: &Theme) -> Frame {
    Frame::new()
        .fill(theme.components.well_bg)
        .corner_radius(radius::FULL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::setup_egui;
    use egui::epaint::{RectShape, Shape};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn text_width(harness: &Harness<'_>, role: TextRole, text: &str) -> f32 {
        role.galley(&harness.ctx, text, Color32::PLACEHOLDER, f32::INFINITY)
            .rect
            .width()
    }

    fn harness(add: impl FnMut(&mut Ui) + 'static) -> Harness<'static> {
        let mut add = add;
        let mut harness = Harness::builder().build_ui(move |ui| {
            ui.horizontal(|ui| add(ui));
        });
        setup_egui(&mut harness);
        harness.run();
        harness
    }

    #[test]
    fn the_well_is_a_full_pill_in_well_bg() {
        let theme = crate::test_support::distinct_role_theme();
        let frame = well_frame(&theme);
        assert_eq!(frame.fill, theme.components.well_bg);
        assert_eq!(frame.corner_radius, radius::FULL.into());
    }

    #[test]
    fn counts_cap_at_ninety_nine_plus_and_show_zero() {
        assert_eq!(CountBadge::new(0).text(), "0");
        assert_eq!(CountBadge::new(3).text(), "3");
        assert_eq!(CountBadge::new(99).text(), "99");
        assert_eq!(CountBadge::new(100).text(), "99+");
    }

    #[test]
    fn count_badges_fill_with_their_tones_solid_colours() {
        let mut harness = harness(|ui| {
            CountBadge::new(0).tone(Tone::Success).show(ui);
        });
        crate::test_support::install_distinct_role_theme(&harness.ctx);
        harness.run();
        let theme = harness.ctx.tokens();
        let zero = harness.get_by_label("0").rect();
        assert_eq!(zero.height(), size::COUNT_BADGE);
        assert!(zero.width() >= size::COUNT_BADGE, "at least a circle");
        let output = harness.output();
        let fills: Vec<Color32> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Rect(RectShape { fill, .. }) => Some(*fill),
                _ => None,
            })
            .collect();
        assert!(fills.contains(&theme.solid(Tone::Success).0));
    }

    #[test]
    fn capsules_are_padded_rows_of_fixed_height() {
        let harness = harness(|ui| {
            Pill::new("Following", Tone::Success).show(ui);
            Pill::new("Modified", Tone::Warning).without_dot().show(ui);
            kind_badge(ui, "Deployment");
            CountBadge::new(3).show(ui);
            Chip::new("coredns").key("k8s-app=").show(ui);
        });

        let following = harness.get_by_label("Following").rect();
        assert_eq!(following.height(), size::PILL_HEIGHT);
        let expected = 2.0 * size::PILL_PADDING_X
            + size::STATUS_DOT
            + size::STATUS_DOT_GAP
            + text_width(&harness, type_scale::PILL, "Following");
        assert!((following.width() - expected).abs() < 1e-3);

        let without_dot = harness.get_by_label("Modified").rect();
        let expected =
            2.0 * size::PILL_PADDING_X + text_width(&harness, type_scale::PILL, "Modified");
        assert!((without_dot.width() - expected).abs() < 1e-3);

        assert_eq!(
            harness.get_by_label("Deployment").rect().height(),
            size::PILL_HEIGHT
        );
        let count = harness.get_by_label("3").rect();
        assert_eq!(count.size(), Vec2::splat(size::COUNT_BADGE));
        let chip = harness.get_by_label("k8s-app=coredns").rect();
        assert_eq!(chip.height(), size::PILL_HEIGHT);
        let expected = 2.0 * size::PILL_PADDING_X
            + space::S4
            + text_width(&harness, type_scale::CHIP, "k8s-app=")
            + text_width(&harness, type_scale::CHIP, "coredns");
        assert!(
            (chip.width() - expected).abs() < 0.1,
            "{} != {expected}",
            chip.width()
        );
    }

    #[test]
    fn live_pills_reserve_a_minimum_width_and_grow_past_it() {
        let harness = harness(|ui| {
            Pill::new("Live", Tone::Warning).live().show(ui);
            Pill::new("Previous logs unavailable", Tone::Danger)
                .live()
                .show(ui);
        });
        assert_eq!(
            harness.get_by_label("Live").rect().width(),
            size::LIVE_PILL_MIN_WIDTH
        );
        assert!(
            harness
                .get_by_label("Previous logs unavailable")
                .rect()
                .width()
                > size::LIVE_PILL_MIN_WIDTH
        );
    }

    #[test]
    fn pills_on_a_baseline_share_it_with_the_text_beside_them() {
        let harness = harness(|ui| {
            let baseline = type_scale::BODY.centred_baseline(56.0, 1.0);
            type_scale::BODY.label_on_baseline(ui, "settings", Color32::PLACEHOLDER, baseline);
            Pill::new("Modified", Tone::Warning).show_on_baseline(ui, baseline);
        });
        let label = harness.get_by_label("settings").rect();
        let pill = harness.get_by_label("Modified").rect();
        let label_baseline = label.top() + type_scale::BODY.css_baseline(1.0);
        let pill_text_top = pill.center().y - type_scale::PILL.line_height() / 2.0;
        let pill_baseline = pill_text_top + type_scale::PILL.css_baseline(1.0);
        assert_eq!(pill_baseline, label_baseline);
    }

    #[test]
    fn a_copyable_chip_copies_its_value_and_keeps_its_label_inside() {
        let mut harness = harness(|ui| {
            Chip::new("/etc/coredns/Corefile").copyable("Args").show(ui);
            Chip::new("53/UDP").show(ui);
        });
        let chip = harness
            .get_by_label("Copy Args /etc/coredns/Corefile")
            .rect();
        let plain = harness.get_by_label("53/UDP").rect();
        assert_eq!(chip.height(), size::PILL_HEIGHT);
        assert_eq!(chip.center().y, plain.center().y);
        assert!(chip.right() <= plain.left());
        let copied = crate::test_support::click_and_collect_copies(&mut harness, chip.center());
        assert_eq!(copied, ["/etc/coredns/Corefile"]);
        assert!(harness.query_by_label("/etc/coredns/Corefile").is_some());
    }

    #[test]
    fn a_label_chip_copies_its_key_and_value() {
        let mut harness = harness(|ui| {
            Chip::new("web")
                .key("app=")
                .copyable("Labels")
                .copy_as("app=web")
                .show(ui);
        });
        let chip = harness.get_by_label("Copy Labels app=web").rect().center();
        let copied = crate::test_support::click_and_collect_copies(&mut harness, chip);
        assert_eq!(copied, ["app=web"]);
    }

    #[test]
    fn copyable_chips_wrap_like_plain_ones() {
        const ARGS: [&str; 4] = ["--first-flag", "--second-flag", "--third-flag", "--fourth"];
        let mut harness = Harness::builder().build_ui(|ui| {
            ui.set_max_width(240.0);
            chip_flow(ui, |ui| {
                for arg in ARGS {
                    Chip::new(arg).copyable("Args").show(ui);
                }
            });
        });
        setup_egui(&mut harness);
        harness.run();
        let rects: Vec<Rect> = ARGS
            .iter()
            .map(|arg| harness.get_by_label(&format!("Copy Args {arg}")).rect())
            .collect();
        let left = rects[0].left();
        assert!(rects.iter().all(|rect| rect.right() <= left + 240.0));
        assert!(
            rects.last().unwrap().top() > rects[0].top(),
            "the chips wrap"
        );
    }
}
