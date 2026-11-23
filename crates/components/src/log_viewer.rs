//! The parts of the log viewer (C20): what you are looking at
//! ([`LogHeader`]), how busy it was ([`VolumeStrip`] with its
//! [`SourcePill`]s and [`VolumeBars`]), the lines themselves ([`LogLine`]
//! with its [`LevelBadge`]) and the tools floating over them
//! ([`FloatingToolbar`]).

use egui::{Align, Color32, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::badge::{Capsule, Pill, kind_badge_on};
use crate::code_surface::{CodeSpan, SpanStyle, paint_spans};
use crate::design::theme::{self, Theme, ThemeExt, ThemeScope, Tone};
use crate::design::type_scale::{self, TextRole, Weight};
use crate::design::{elevation, radius, size, space};
use crate::frames::{raised_frame, show_elevated};
use crate::search::format_count;
use crate::tooltip::TooltipExt;

/// The title of a log window (`.lhead .lt`): Bricolage 28/34.
const TITLE_ROLE: TextRole = type_scale::DISPLAY
    .with_size(28.0, 34.0)
    .with_letter_spacing(0.0);
/// The caption line under it (`.lhead .ls`): condensed 11/14 caps, +0.08 em.
const CAPTION_ROLE: TextRole = type_scale::CAPTION_SM.with_letter_spacing(0.08);
/// Gap between the parts of the header (`.lhead` gap 14).
const HEADER_GAP: f32 = 14.0;
/// A source pill's name (`.srcp`): Figtree 12 · 500 in a 20 point line box.
const SOURCE_ROLE: TextRole = type_scale::META_ROW.with_weight(Weight::Medium);
/// A source pill's count or state (`.srcp .n`): Plex Mono 10.5.
const SOURCE_DETAIL_ROLE: TextRole = type_scale::MONO_COUNT;
/// Gap between a source pill's parts and between source pills.
const SOURCE_GAP: f32 = space::S6;
/// Gap between the volume strip's caption and its pills (`.row` gap 8).
const STRIP_GAP: f32 = space::S8;
/// Gap between the volume strip's caption row and its bars.
const STRIP_BARS_GAP: f32 = 14.0;
/// A log line's level (`.lv`): Plex Mono 10.5 · 500.
const LEVEL_ROLE: TextRole = type_scale::MONO
    .with_weight(Weight::Medium)
    .with_size(10.5, 16.0);
/// Gap between the parts of a log line (`.ll` gap 8).
const LINE_GAP: f32 = space::S8;

/// The severity of a log line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    /// The level's word, as the badge shows it.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }

    /// The badge's fill and text colour.
    fn colors(self, theme: &Theme) -> (Color32, Color32) {
        match self {
            Self::Error => (theme.danger.tint_hover, theme.code.error),
            Self::Warn => (theme.warning.tint, theme.code.warning),
            Self::Info => (theme.info.tint, theme.code.info),
            Self::Debug => (theme.control.fill_strong, theme.code.debug),
        }
    }
}

/// A level badge (`.lv`): the level's word centred in a
/// [`size::LEVEL_BADGE`] pill on its tint — ERROR danger, WARN warning,
/// INFO info, DEBUG neutral.
pub struct LevelBadge(pub LogLevel);

impl LevelBadge {
    /// Add the badge, read as its word.
    pub fn show(self, ui: &mut Ui) -> Response {
        let word = self.0.word();
        let (rect, response) =
            type_scale::allocate_text(ui, vec2(size::LEVEL_BADGE, size::LOG_PILL), word);
        self.paint(ui, rect, &response);
        response
    }

    fn paint(&self, ui: &Ui, rect: Rect, response: &Response) {
        let (fill, color) = self.0.colors(&ui.tokens());
        paint_centred_pill(ui, rect, fill, LEVEL_ROLE, self.0.word(), color, response);
    }
}

/// Paint `text` in `role` centred on a pill `rect` filled with `fill`,
/// exposed as `response`'s label.
fn paint_centred_pill(
    ui: &Ui,
    rect: Rect,
    fill: Color32,
    role: TextRole,
    text: &str,
    color: Color32,
    response: &Response,
) {
    let text = role.galley(ui.ctx(), text, color, f32::INFINITY);
    Capsule {
        centred: true,
        ..Capsule::pill(fill, text)
    }
    .paint(ui, rect, Some(response));
}

/// The state of one source (a replica) of a log window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceState {
    /// Streaming: the success lamp and the line count.
    Live,
    /// Reconnecting: the warning lamp with its ring.
    Reconnecting,
    /// Offline: a neutral lamp, the name muted.
    Offline,
}

/// A source pill (`.srcp`): lamp, the source's name and its line count or
/// state on the well. Clicking it hides or shows the source.
#[must_use = "call `show` to add the pill"]
pub struct SourcePill<'a> {
    name: &'a str,
    state: SourceState,
    detail: String,
}

impl<'a> SourcePill<'a> {
    /// A live source with `lines` lines.
    pub fn live(name: &'a str, lines: usize) -> Self {
        Self {
            name,
            state: SourceState::Live,
            detail: format_count(lines),
        }
    }

    /// A reconnecting source.
    pub fn reconnecting(name: &'a str) -> Self {
        Self {
            name,
            state: SourceState::Reconnecting,
            detail: "reconnecting".to_owned(),
        }
    }

    /// An offline source.
    pub fn offline(name: &'a str) -> Self {
        Self {
            name,
            state: SourceState::Offline,
            detail: "offline".to_owned(),
        }
    }

    fn widths(&self, ui: &Ui) -> (f32, f32) {
        let width = |role: TextRole, text: &str| {
            role.galley(ui.ctx(), text, Color32::PLACEHOLDER, f32::INFINITY)
                .rect
                .width()
        };
        (
            width(SOURCE_ROLE, self.name),
            width(SOURCE_DETAIL_ROLE, &self.detail),
        )
    }

    /// The pill's width.
    pub fn width(&self, ui: &Ui) -> f32 {
        let (name, detail) = self.widths(ui);
        2.0 * size::SOURCE_PILL_PADDING_X + size::STATUS_DOT + name + detail + 2.0 * SOURCE_GAP
    }

    /// Add the pill; a click asks to hide or show the source.
    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let (name_width, _) = self.widths(ui);
        let (rect, response) =
            ui.allocate_exact_size(vec2(self.width(ui), size::SOURCE_PILL), Sense::click());
        let label = format!("{} · {}", self.name, self.detail);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &label)
        });
        let response = response.tooltip("Hide or show this source");
        if !ui.is_rect_visible(rect.expand(size::LAMP_RING + elevation::GLOW_BLUR)) {
            return response;
        }
        ui.painter()
            .rect_filled(rect, radius::FULL, theme.components.well_bg);
        let center_y = rect.center().y;
        let mut left = rect.left() + size::SOURCE_PILL_PADDING_X;
        let lamp = crate::status::lamp_rect(left, center_y);
        match self.state {
            SourceState::Live => {
                crate::status::paint_status_dot(ui.painter(), &theme, lamp, Tone::Success);
            }
            SourceState::Reconnecting => paint_ringed_lamp(ui, &theme, lamp, Tone::Warning),
            SourceState::Offline => {
                crate::status::paint_status_dot(ui.painter(), &theme, lamp, Tone::Neutral);
            }
        }
        left += size::STATUS_DOT + SOURCE_GAP;
        let name_color = match self.state {
            SourceState::Offline => theme.text.muted,
            SourceState::Live | SourceState::Reconnecting => theme.text.primary,
        };
        let name = SOURCE_ROLE.galley(ui.ctx(), self.name, name_color, f32::INFINITY);
        let top = center_y - SOURCE_ROLE.line_height() / 2.0;
        type_scale::paint_galley(ui.painter(), pos2(left, top), name, name_color);
        left += name_width + SOURCE_GAP;
        let detail =
            SOURCE_DETAIL_ROLE.galley(ui.ctx(), &self.detail, theme.text.muted, f32::INFINITY);
        type_scale::paint_galley(ui.painter(), pos2(left, top), detail, theme.text.muted);
        response
    }
}

/// A lit lamp with a halo ring in its tone (`.dot.ring`).
fn paint_ringed_lamp(ui: &Ui, theme: &Theme, lamp: Rect, tone: Tone) {
    let halo = match tone {
        Tone::Danger => theme.danger.halo,
        Tone::Accent => theme.focus.halo,
        _ => theme.warning.halo,
    };
    crate::status::paint_ringed_status_dot(ui.painter(), theme, lamp, tone, halo);
}

/// One bar of a [`VolumeBars`] chart: its height as a fraction of the
/// chart's, and whether it holds errors or warnings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeBar {
    pub height: f32,
    pub tone: Option<Tone>,
}

/// What [`VolumeBars`] did this frame.
pub struct VolumeBarsResponse {
    pub response: Response,
    /// How many bars the visible window was dragged by (scroll the log as
    /// far).
    pub window_dragged: f32,
}

/// Log volume over time (`.vbars`): [`size::VOLUME_BAR`] bars
/// [`size::VOLUME_BAR_GAP`] apart standing on the baseline, in
/// `divider-strong`, errors `danger-fill` and warnings `warning-fill`, and
/// the window on screen outlined in `accent-fill`. Drag the window to
/// scroll.
#[must_use = "call `show` to add the bars"]
pub struct VolumeBars<'a> {
    bars: &'a [VolumeBar],
    height: f32,
    window: Option<(f32, f32)>,
}

impl<'a> VolumeBars<'a> {
    pub fn new(bars: &'a [VolumeBar]) -> Self {
        Self {
            bars,
            height: size::VOLUME_BARS,
            window: None,
        }
    }

    /// The chart's height ([`size::VOLUME_BARS`] by default).
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Outline the visible window from bar `start` over `length` bars.
    pub fn window(mut self, start: f32, length: f32) -> Self {
        self.window = Some((start, length));
        self
    }

    /// The chart's width.
    pub fn width(&self) -> f32 {
        let count = self.bars.len() as f32;
        (count * (size::VOLUME_BAR + size::VOLUME_BAR_GAP) - size::VOLUME_BAR_GAP).max(0.0)
    }

    pub fn show(self, ui: &mut Ui) -> VolumeBarsResponse {
        let (rect, response) =
            ui.allocate_exact_size(vec2(self.width(), self.height), Sense::hover());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), "Log volume")
        });
        let pitch = size::VOLUME_BAR + size::VOLUME_BAR_GAP;
        let mut window_dragged = 0.0;
        if let Some((start, length)) = self.window {
            let outline = Rect::from_min_size(
                pos2(
                    rect.left() + start * pitch,
                    rect.top() - size::VOLUME_WINDOW_OUTSET,
                ),
                vec2(
                    length * pitch,
                    self.height + 2.0 * size::VOLUME_WINDOW_OUTSET,
                ),
            );
            let drag = ui
                .interact(outline, response.id.with("window"), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::Grab);
            drag.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Other,
                    ui.is_enabled(),
                    "Visible window",
                )
            });
            window_dragged = drag.drag_delta().x / pitch;
            self.paint(ui, rect, Some(outline));
        } else {
            self.paint(ui, rect, None);
        }
        VolumeBarsResponse {
            response,
            window_dragged,
        }
    }

    fn paint(&self, ui: &Ui, rect: Rect, window: Option<Rect>) {
        if !ui.is_rect_visible(rect.expand(size::VOLUME_WINDOW_OUTSET)) {
            return;
        }
        let theme = ui.tokens();
        let painter = ui.painter();
        let pitch = size::VOLUME_BAR + size::VOLUME_BAR_GAP;
        for (index, bar) in self.bars.iter().enumerate() {
            let height = (bar.height.clamp(0.0, 1.0) * self.height).round();
            let left = rect.left() + index as f32 * pitch;
            let bar_rect = Rect::from_min_max(
                pos2(left, rect.bottom() - height),
                pos2(left + size::VOLUME_BAR, rect.bottom()),
            );
            let fill = match bar.tone {
                Some(tone) => theme.tone(tone).fill,
                None => theme.divider.strong,
            };
            painter.rect_filled(bar_rect, radius::VOLUME_BAR, fill);
        }
        if let Some(outline) = window {
            painter.rect_stroke(
                outline,
                radius::VOLUME_WINDOW,
                egui::Stroke::new(size::CONTROL_STROKE, theme.accent.fill),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// The volume strip (`.vstrip`): a card with the caption ("Volume · 15 min
/// · 41 errors") and the source pills on one row, the [`VolumeBars`] under
/// them.
#[must_use = "call `show` to add the strip"]
pub struct VolumeStrip<'a> {
    caption: &'a str,
    sources: Vec<SourcePill<'a>>,
    bars: VolumeBars<'a>,
}

/// What a [`VolumeStrip`] did this frame.
pub struct VolumeStripResponse {
    /// The source whose pill was clicked: hide or show it.
    pub source_clicked: Option<usize>,
    /// How many bars the visible window was dragged by.
    pub window_dragged: f32,
}

impl<'a> VolumeStrip<'a> {
    pub fn new(caption: &'a str, bars: VolumeBars<'a>) -> Self {
        Self {
            caption,
            sources: Vec::new(),
            bars,
        }
    }

    pub fn source(mut self, source: SourcePill<'a>) -> Self {
        self.sources.push(source);
        self
    }

    /// Add the strip across the available width.
    pub fn show(self, ui: &mut Ui) -> VolumeStripResponse {
        let theme = ui.tokens();
        let padding = size::VOLUME_STRIP_PADDING;
        let height = padding.sum().y + size::SOURCE_PILL + STRIP_BARS_GAP + self.bars.height;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter()
                .rect_filled(rect, radius::CARD, theme.surface.panel);
        }
        let content = rect - padding;
        let row = Rect::from_min_size(content.min, vec2(content.width(), size::SOURCE_PILL));
        let caption =
            type_scale::CAPTION.galley(ui.ctx(), self.caption, theme.text.muted, f32::INFINITY);
        let caption_pos = pos2(row.left(), row.center().y - caption.rect.height() / 2.0);
        type_scale::text_at(
            ui,
            ui.id().with(("volume-caption", self.caption)),
            caption_pos,
            caption,
            self.caption,
            theme.text.muted,
        );

        let mut pills = ui.new_child(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::right_to_left(Align::Center)),
        );
        pills.spacing_mut().item_spacing = vec2(STRIP_GAP, 0.0);
        let mut source_clicked = None;
        for (index, source) in self.sources.into_iter().enumerate().rev() {
            if source.show(&mut pills).clicked() {
                source_clicked = Some(index);
            }
        }
        let bars_rect = Rect::from_min_max(
            pos2(content.left(), row.bottom() + STRIP_BARS_GAP),
            content.max,
        );
        let mut bars_ui = ui.new_child(UiBuilder::new().max_rect(bars_rect));
        let bars = self.bars.show(&mut bars_ui);
        VolumeStripResponse {
            source_clicked,
            window_dragged: bars.window_dragged,
        }
    }
}

/// The source of a log line: its name in a pill of the `n`th log source
/// colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogSource<'a> {
    pub name: &'a str,
    pub color: usize,
}

/// One log line (`.ll`), [`size::LOG_ROW`] tall in [`type_scale::LOG`]:
/// line number, time, source pill (multi-source windows only), level
/// badge and message. Matches are underlined ([`crate::CodeMark::Underline`]),
/// the active match filled ([`crate::CodeMark::ActiveMatch`]). Error lines
/// are tinted `log-row-error`; the current line is `code-line-highlight`.
#[must_use = "call `show` to add the line"]
pub struct LogLine<'a> {
    number: Option<usize>,
    time: Option<&'a str>,
    source: Option<LogSource<'a>>,
    level: Option<LogLevel>,
    message: Vec<CodeSpan<'a>>,
    current: bool,
}

impl<'a> LogLine<'a> {
    pub fn new(message: impl IntoIterator<Item = CodeSpan<'a>>) -> Self {
        Self {
            number: None,
            time: None,
            source: None,
            level: None,
            message: message.into_iter().collect(),
            current: false,
        }
    }

    pub fn number(mut self, number: usize) -> Self {
        self.number = Some(number);
        self
    }

    pub fn time(mut self, time: &'a str) -> Self {
        self.time = Some(time);
        self
    }

    pub fn source(mut self, name: &'a str, color: usize) -> Self {
        self.source = Some(LogSource { name, color });
        self
    }

    pub fn level(mut self, level: LogLevel) -> Self {
        self.level = Some(level);
        self
    }

    /// The current line (`code-line-highlight`).
    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    /// Add the line across the available width.
    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let role = type_scale::LOG;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), size::LOG_ROW), Sense::hover());
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let tint = if self.current {
            Some(theme.code.line_highlight)
        } else if self.level == Some(LogLevel::Error) {
            Some(theme.log.row_error)
        } else {
            None
        };
        if let Some(tint) = tint {
            ui.painter().rect_filled(rect, 0, tint);
        }
        let painter = ui.painter_at(rect);
        let center_y = rect.center().y;
        let mut left = rect.left() + size::LOG_ROW_PADDING_X;
        let id = response.id;
        if let Some(number) = self.number {
            let text = number.to_string();
            let galley = role.galley(ui.ctx(), &text, theme.code.gutter, f32::INFINITY);
            let pos = pos2(
                left + size::LOG_LINE_NUMBER - galley.rect.width(),
                rect.top(),
            );
            type_scale::text_at(ui, id.with("number"), pos, galley, &text, theme.code.gutter);
            left += size::LOG_LINE_NUMBER + LINE_GAP;
        }
        if let Some(time) = self.time {
            let galley = role.galley(ui.ctx(), time, theme.code.text_muted, f32::INFINITY);
            let width = galley.rect.width();
            type_scale::text_at(
                ui,
                id.with("time"),
                pos2(left, rect.top()),
                galley,
                time,
                theme.code.text_muted,
            );
            left += width + LINE_GAP;
        }
        if let Some(source) = self.source {
            let name =
                type_scale::BADGE.galley(ui.ctx(), source.name, theme.text.on_fill, f32::INFINITY);
            let width = (name.rect.width() + 2.0 * size::LOG_SOURCE_PADDING_X)
                .max(size::LOG_SOURCE_MIN_WIDTH);
            let pill = Rect::from_min_size(
                pos2(left, center_y - size::LOG_PILL / 2.0),
                vec2(width, size::LOG_PILL),
            );
            let fill = [theme.log.source_1, theme.log.source_2][source.color % 2];
            let label = type_scale::interact_text(ui, pill, id.with("source"), source.name);
            paint_centred_pill(
                ui,
                pill,
                fill,
                type_scale::BADGE,
                source.name,
                theme.text.on_fill,
                &label,
            );
            left += width + LINE_GAP;
        }
        if let Some(level) = self.level {
            let badge = Rect::from_min_size(
                pos2(left, center_y - size::LOG_PILL / 2.0),
                vec2(size::LEVEL_BADGE, size::LOG_PILL),
            );
            let label = type_scale::interact_text(ui, badge, id.with("level"), level.word());
            LevelBadge(level).paint(ui, badge, &label);
            left += size::LEVEL_BADGE + LINE_GAP;
        }
        let style = SpanStyle {
            role,
            match_radius: radius::LOG_MATCH,
        };
        paint_spans(
            ui,
            &painter,
            &style,
            &self.message,
            None,
            pos2(left, rect.top()),
            id.with("message"),
        );
        response
    }
}

/// How a log window's stream is doing, shown as the header's pill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogStream {
    /// Following new lines.
    Following,
    /// Spooling the history before following.
    Spooling,
    /// `live` of `total` sources streaming.
    Streaming { live: usize, total: usize },
    /// No stream (the container has no logs, or they cannot be read).
    Unavailable,
}

impl LogStream {
    /// The pill's words and tone.
    pub fn pill(self) -> (String, Tone) {
        match self {
            Self::Following => ("Following".to_owned(), Tone::Success),
            Self::Spooling => ("Spooling".to_owned(), Tone::Info),
            // Every source streaming, and at least one.
            Self::Streaming { live, total } if total > 0 && live >= total => {
                (format!("{live} of {total} streaming"), Tone::Success)
            }
            Self::Streaming { live, total } => {
                (format!("{live} of {total} streaming"), Tone::Warning)
            }
            Self::Unavailable => ("Unavailable".to_owned(), Tone::Danger),
        }
    }
}

/// The header of a log window (`.lhead`): kind badge · title (Bricolage
/// 28) over a caption line (container · line count) · stream pill, then
/// the window's own controls at the end (Current / Previous instance).
#[must_use = "call `show` to add the header"]
pub struct LogHeader<'a> {
    kind: &'a str,
    kind_fill: Option<Color32>,
    title: &'a str,
    caption: &'a str,
    stream: LogStream,
}

impl<'a> LogHeader<'a> {
    pub fn new(kind: &'a str, title: &'a str, caption: &'a str, stream: LogStream) -> Self {
        Self {
            kind,
            kind_fill: None,
            title,
            caption,
            stream,
        }
    }

    /// The kind badge's fill (the kind's category colour).
    pub fn kind_fill(mut self, fill: Color32) -> Self {
        self.kind_fill = Some(fill);
        self
    }

    /// Add the header across the available width, `add_trailing` laying
    /// the window's controls out from its end.
    pub fn show(self, ui: &mut Ui, add_trailing: impl FnOnce(&mut Ui)) -> Response {
        let theme = ui.tokens();
        let height = TITLE_ROLE.line_height() + CAPTION_ROLE.line_height();
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let mut trailing = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::right_to_left(Align::Center)),
        );
        add_trailing(&mut trailing);
        let trailing_left = trailing.min_rect().left().min(rect.right());

        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        row.spacing_mut().item_spacing = vec2(HEADER_GAP, 0.0);
        let fill = self.kind_fill.unwrap_or(theme.accent.fill);
        kind_badge_on(&mut row, self.kind, fill);
        let (text, tone) = self.stream.pill();
        let pill = Pill::new(&text, tone);
        let pill_width = pill.size(&row).x;
        let left = row.cursor().left();
        let title_room = (trailing_left - HEADER_GAP - pill_width - HEADER_GAP - left).max(0.0);
        let title =
            TITLE_ROLE.galley_elided(row.ctx(), self.title, theme.text.primary, title_room, 1);
        let caption =
            CAPTION_ROLE.galley_elided(row.ctx(), self.caption, theme.text.muted, title_room, 1);
        let width = title.rect.width().max(caption.rect.width());
        let (block, _) = row.allocate_exact_size(vec2(width, height), Sense::hover());
        type_scale::text_at(
            &row,
            response.id.with("title"),
            block.min,
            title,
            self.title,
            theme.text.primary,
        );
        type_scale::text_at(
            &row,
            response.id.with("caption"),
            pos2(block.left(), block.top() + TITLE_ROLE.line_height()),
            caption,
            self.caption,
            theme.text.muted,
        );
        pill.show(&mut row);
        response
    }
}

/// A floating toolbar (`.ftool`): a pill bar on `surface-raised` with the
/// raised shadow holding the log window's search and view toggles, its
/// fields and wells on `surface-panel`.
pub struct FloatingToolbar;

impl FloatingToolbar {
    /// Add the toolbar at the cursor, as wide as its controls.
    pub fn show<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        let theme = ui.tokens();
        let frame = raised_frame(&theme)
            .corner_radius(radius::FULL)
            .inner_margin(egui::Margin::same(size::FLOATING_TOOLBAR_PADDING as i8));
        show_elevated(ui, frame, theme.shadow.raised, |ui| {
            theme::scope(ui, ThemeScope::FloatingToolbar, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(space::S6, 0.0);
                    add_contents(ui)
                })
                .inner
            })
        })
        .inner
    }

    /// Float the toolbar centred over the bottom of `window`
    /// ([`size::FLOATING_TOOLBAR_INSET`] above it), as wide as `width`.
    pub fn show_over<R>(
        ui: &mut Ui,
        window: Rect,
        width: f32,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let height = size::CONTROL_LG + 2.0 * size::FLOATING_TOOLBAR_PADDING;
        let rect = Rect::from_center_size(
            pos2(
                window.center().x,
                window.bottom() - size::FLOATING_TOOLBAR_INSET - height / 2.0,
            ),
            vec2(width, height),
        );
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        Self::show(&mut child, add_contents)
    }
}
