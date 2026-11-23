//! Readouts (C19 `.ro`): the few numbers checked first, lifted out of the
//! property grid into a row of equal tiles: a caption in condensed caps
//! over a value in mono, the value toned by what it means.

use std::sync::Arc;

use egui::{Color32, Galley, Rect, Response, Sense, Ui, pos2, vec2};

use crate::design::theme::{Theme, ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole, Weight};
use crate::design::{radius, size, space};
use crate::tooltip::TooltipExt;

/// The caption of a readout (`.ro .k`): Plex Sans Condensed 11/14 caps.
const LABEL_ROLE: TextRole = type_scale::CAPTION_SM;
/// The value of an inspector-size readout (`.ro .v`): Plex Mono 15/20.
const VALUE_ROLE: TextRole = type_scale::MONO.with_size(15.0, 20.0);
/// The value of a gauge-size readout (`.ro.lg .v`): Plex Mono 24/28 · 500.
const GAUGE_ROLE: TextRole = type_scale::MONO
    .with_weight(Weight::Medium)
    .with_size(24.0, 28.0);
/// Gap between a readout's caption and value (`.ro` gap 2).
const LABEL_GAP: f32 = space::S2;
/// Gap between the tiles of a readout row (`.ros` gap 8).
const ROW_GAP: f32 = space::S8;

/// How big a readout's value is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadoutSize {
    /// Plex Mono 15/20: inspector headers.
    #[default]
    Inspector,
    /// Plex Mono 24/28: overviews and the cluster strip.
    Gauge,
}

impl ReadoutSize {
    const fn role(self) -> TextRole {
        match self {
            Self::Inspector => VALUE_ROLE,
            Self::Gauge => GAUGE_ROLE,
        }
    }
}

/// What a readout's value means, which tones the value (never the tile).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadoutTone {
    /// `text-primary`.
    #[default]
    Neutral,
    /// A healthy state word: `success-text`.
    Ok,
    /// Restarts, degraded: `warning-text`.
    Warning,
    /// Failing counts: `danger-text`.
    Problem,
    /// A value that links (node, owner): `accent-text`.
    Link,
}

impl ReadoutTone {
    fn color(self, theme: &Theme) -> Color32 {
        match self {
            Self::Neutral => theme.text.primary,
            Self::Ok => theme.tone(Tone::Success).text,
            Self::Warning => theme.tone(Tone::Warning).text,
            Self::Problem => theme.tone(Tone::Danger).text,
            Self::Link => theme.accent.text,
        }
    }
}

/// One readout: `label` over `value`, with an optional `unit` in 11 px
/// `text-muted` after it ("412m / 2", "38 ms").
#[derive(Clone, Copy, Debug)]
pub struct Readout<'a> {
    label: &'a str,
    value: &'a str,
    unit: Option<&'a str>,
    tone: ReadoutTone,
}

impl<'a> Readout<'a> {
    pub const fn new(label: &'a str, value: &'a str) -> Self {
        Self {
            label,
            value,
            unit: None,
            tone: ReadoutTone::Neutral,
        }
    }

    /// The unit after the value ("/ 2", "ms").
    pub const fn unit(mut self, unit: &'a str) -> Self {
        self.unit = Some(unit);
        self
    }

    pub const fn tone(mut self, tone: ReadoutTone) -> Self {
        self.tone = tone;
        self
    }

    /// The text the readout is read as: "Restarts: 15".
    fn spoken(&self) -> String {
        match self.unit {
            Some(unit) => format!("{}: {} {unit}", self.label, self.value),
            None => format!("{}: {}", self.label, self.value),
        }
    }
}

/// The value line of a readout: its galleys and line box.
struct ValueLine {
    value: Arc<Galley>,
    unit: Option<Arc<Galley>>,
    /// The line box's height (the unit's smaller font can make it taller).
    height: f32,
    /// The value's top, from the line box's top.
    value_top: f32,
    /// The unit's top, from the line box's top.
    unit_top: f32,
    /// The unit's left, from the value's left.
    unit_left: f32,
}

impl ValueLine {
    /// Lay `readout` out at `size` in `width`. The value and the unit share
    /// a baseline and the value's line height, as CSS sets an inline
    /// `<small>`: each is centred in its own line-height box on the
    /// baseline, and the line box spans both.
    fn new(ui: &Ui, readout: &Readout<'_>, size: ReadoutSize, width: f32) -> Self {
        let theme = ui.tokens();
        let ppp = ui.pixels_per_point();
        let role = size.role();
        let color = readout.tone.color(&theme);
        let unit_role = UNIT_ROLE.with_size(UNIT_ROLE.size(), role.line_height());
        let unit = readout
            .unit
            .map(|unit| unit_role.galley(ui.ctx(), unit, theme.text.muted, f32::INFINITY));
        let space = match &unit {
            Some(_) => role
                .galley(ui.ctx(), " ", color, f32::INFINITY)
                .rect
                .width(),
            None => 0.0,
        };
        let unit_width = unit.as_ref().map_or(0.0, |unit| unit.rect.width());
        let value = role.galley_elided(
            ui.ctx(),
            readout.value,
            color,
            (width - space - unit_width).max(0.0),
            1,
        );
        let value_baseline = role.css_baseline(ppp);
        let (height, value_top, unit_top) = match &unit {
            Some(_) => {
                let unit_baseline = unit_role.css_baseline(ppp);
                let above = value_baseline.max(unit_baseline);
                let below = (role.line_height() - value_baseline)
                    .max(unit_role.line_height() - unit_baseline);
                (above + below, above - value_baseline, above - unit_baseline)
            }
            None => (role.line_height(), 0.0, 0.0),
        };
        let unit_left = value.rect.width() + space;
        Self {
            value,
            unit,
            height,
            value_top,
            unit_top,
            unit_left,
        }
    }
}

/// The unit after a readout's value (`.ro .v small`): Plex Mono 11.
const UNIT_ROLE: TextRole = type_scale::MONO_HINT;

/// A row of 4–6 readouts (`.ros`): equal tiles 8 points apart across the
/// available width, never wrapping. A value that does not fit is cut with
/// an ellipsis and shown whole in a tooltip.
#[must_use = "call `show` to add the row"]
pub struct ReadoutRow<'r, 'a> {
    readouts: &'r [Readout<'a>],
    size: ReadoutSize,
    on_raised: bool,
}

impl<'r, 'a> ReadoutRow<'r, 'a> {
    pub fn new(readouts: &'r [Readout<'a>]) -> Self {
        Self {
            readouts,
            size: ReadoutSize::Inspector,
            on_raised: false,
        }
    }

    pub fn size(mut self, size: ReadoutSize) -> Self {
        self.size = size;
        self
    }

    /// Set the tiles on `surface-panel` instead of `surface-raised`, for a
    /// row on a raised surface or card (`.palette .ro`, `.card .ro`).
    pub fn on_raised_surface(mut self) -> Self {
        self.on_raised = true;
        self
    }

    /// The height of the row's tiles (the tallest value line sets it).
    pub fn height(&self, ui: &Ui) -> f32 {
        self.readouts
            .iter()
            .map(|readout| tile_height(&ValueLine::new(ui, readout, self.size, f32::INFINITY)))
            .fold(0.0, f32::max)
    }

    fn tile_width(&self, width: f32) -> f32 {
        let count = self.readouts.len().max(1) as f32;
        (width - ROW_GAP * (count - 1.0)) / count
    }

    /// Add the row across the available width.
    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let fill = if self.on_raised {
            theme.surface.panel
        } else {
            theme.surface.raised
        };
        let width = ui.available_width();
        let tile_width = self.tile_width(width);
        let content_width = (tile_width - 2.0 * size::READOUT_PADDING.x).max(0.0);
        let lines: Vec<ValueLine> = self
            .readouts
            .iter()
            .map(|readout| ValueLine::new(ui, readout, self.size, content_width))
            .collect();
        let height = lines.iter().map(tile_height).fold(0.0, f32::max);
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        for (index, (readout, line)) in self.readouts.iter().zip(lines).enumerate() {
            let left = rect.left() + index as f32 * (tile_width + ROW_GAP);
            let tile = Rect::from_min_size(pos2(left, rect.top()), vec2(tile_width, height));
            paint_tile(ui, tile, readout, line, fill, response.id.with(index));
        }
        response
    }
}

fn tile_height(line: &ValueLine) -> f32 {
    2.0 * size::READOUT_PADDING.y + LABEL_ROLE.line_height() + LABEL_GAP + line.height
}

/// Paint one tile, read as "label: value".
fn paint_tile(
    ui: &Ui,
    tile: Rect,
    readout: &Readout<'_>,
    line: ValueLine,
    fill: Color32,
    id: egui::Id,
) {
    let response = type_scale::interact_text(ui, tile, id, &readout.spoken());
    if line.value.elided {
        let _ = response.tooltip(readout.value);
    }
    if !ui.is_rect_visible(tile) {
        return;
    }
    let theme = ui.tokens();
    ui.painter().rect_filled(tile, radius::READOUT, fill);
    let content = tile.shrink2(size::READOUT_PADDING);
    // A caption longer than its tile runs past it, as in the design.
    let label = LABEL_ROLE.galley(ui.ctx(), readout.label, theme.text.muted, f32::INFINITY);
    type_scale::paint_galley(ui.painter(), content.min, label, theme.text.muted);
    let line_top = content.top() + LABEL_ROLE.line_height() + LABEL_GAP;
    type_scale::paint_galley(
        ui.painter(),
        pos2(content.left(), line_top + line.value_top),
        line.value,
        readout.tone.color(&theme),
    );
    if let Some(unit) = line.unit {
        type_scale::paint_galley(
            ui.painter(),
            pos2(content.left() + line.unit_left, line_top + line.unit_top),
            unit,
            theme.text.muted,
        );
    }
}
