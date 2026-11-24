//! Document window chrome (C14): the [`WindowHeader`], [`Toolbar`] and
//! [`StatusBar`] that give the log viewer and the YAML editor one shape.
//!
//! - The header is [`size::HEADER_BAR`] tall on `surface-sunken`: an icon
//!   [`Tile`], the title (14/20 semibold) over a muted meta line, a state
//!   [`Pill`] and, on the right, the window's mode or primary actions. The
//!   title truncates first, then the meta line; the pill and the right side
//!   never shrink.
//! - The toolbar is [`size::TOOLBAR`] tall on `surface-canvas` with
//!   [`space::S16`] padding and [`space::S8`] between its 32 point controls:
//!   search → filter → view options on the left, navigation and mode
//!   toggles on the right.
//! - The status bar is [`size::STATUS_BAR`] tall on `surface-sunken`, 12/16
//!   muted items [`space::S16`] apart: what the surface holds on the left,
//!   position and format on the right.
//!
//! Each bar fills the available width and draws its own divider (below the
//! header and toolbar, above the status bar).

use egui::{
    Align, Color32, CornerRadius, Id, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2, pos2,
    vec2,
};

use crate::badge::Pill;
use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole};
use crate::design::{size, space, stroke};
use crate::icons::Icon;
use crate::interaction::PointingHand;
use crate::loading::Spinner;
use crate::measured::{remember_width, remembered_width};
use crate::tile::{Tile, TileSize};
use crate::title_stack::TitleStack;
use crate::tooltip::TooltipExt;

/// The window title (14/20 · 600).
const TITLE: TextRole = type_scale::BODY_STRONG;
/// Gap between the items of the header.
const HEADER_GAP: f32 = space::S12;
/// Gap between the items of the toolbar.
const TOOLBAR_GAP: f32 = space::S8;
/// Gap between the items of the status bar.
const STATUS_GAP: f32 = space::S16;
/// Gap between a spinner and its words in the toolbar.
const ACTIVITY_GAP: f32 = space::S6;
/// Gap between the problems icon and its count.
const PROBLEMS_GAP: f32 = space::S4;
/// Status bar text.
const STATUS_TEXT: TextRole = type_scale::META;

/// Which edge of a bar carries its divider.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Divider {
    Top,
    Bottom,
}

/// Allocate a bar `height` points tall across the available width, fill it
/// and draw its divider; returns its content rect (inside the divider and
/// [`space::S16`] padding).
fn bar(
    ui: &mut Ui,
    height: f32,
    fill: Color32,
    divider: Divider,
    corner_radius: CornerRadius,
) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let theme = ui.tokens();
    let mut content = rect.shrink2(vec2(space::S16, 0.0));
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, corner_radius, fill);
    }
    let line = match divider {
        Divider::Top => {
            content.min.y += size::HAIRLINE;
            rect.top() + size::HAIRLINE / 2.0
        }
        Divider::Bottom => {
            content.max.y -= size::HAIRLINE;
            rect.bottom() - size::HAIRLINE / 2.0
        }
    };
    ui.painter()
        .hline(rect.x_range(), line, stroke::divider(&theme));
    content
}

/// Lay out a bar's contents: `trailing` ends at the right edge of
/// `content`, `leading` gets the rest from its left edge, items `gap`
/// apart and vertically centred.
///
/// The trailing items' width is remembered under `id` from the previous
/// pass; when it changes the pass is discarded and laid out again, so the
/// two sides never overlap.
fn split_row<L, T>(
    ui: &mut Ui,
    id: Id,
    content: Rect,
    gap: f32,
    leading: impl FnOnce(&mut Ui) -> L,
    trailing: impl FnOnce(&mut Ui) -> T,
) -> (L, T) {
    let width_id = id.with("trailing-width");
    let remembered = remembered_width(ui, width_id).unwrap_or(0.0);
    let mut trailing_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(id.with("trailing"))
            // As wide as the trailing side may grow (so text never wraps
            // while the remembered width is stale), leaving the leading side
            // its minimum.
            .max_rect(Rect::from_min_size(
                pos2(content.right() - remembered, content.top()),
                vec2(
                    (content.width() - size::BAR_MIN_LEADING - gap).max(0.0),
                    content.height(),
                ),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    trailing_ui.spacing_mut().item_spacing = vec2(gap, 0.0);
    trailing_ui.set_clip_rect(content.intersect(ui.clip_rect()));
    let trailing = trailing(&mut trailing_ui);
    let used = trailing_ui.min_rect().width().max(0.0);
    let width = if trailing_ui.min_rect().is_positive() {
        used
    } else {
        0.0
    };
    remember_width(ui, width_id, width, "bar trailing width changed");

    let leading_right = if width > 0.0 {
        (content.right() - width - gap).max(content.left() + size::BAR_MIN_LEADING)
    } else {
        content.right()
    };
    let leading_rect = Rect::from_min_max(content.min, pos2(leading_right, content.bottom()));
    let mut leading_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(id.with("leading"))
            .max_rect(leading_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    leading_ui.spacing_mut().item_spacing = vec2(gap, 0.0);
    leading_ui.set_clip_rect(leading_rect.intersect(ui.clip_rect()));
    let leading = leading(&mut leading_ui);
    (leading, trailing)
}

/// The header of a document window.
#[must_use = "call `show` to add the header"]
pub struct WindowHeader<'a> {
    id: Id,
    icon: Icon,
    title: &'a str,
    meta: Option<&'a str>,
    status: Option<(&'a str, Tone)>,
    live_status: bool,
    corner_radius: CornerRadius,
}

/// The response of a [`WindowHeader`].
pub struct WindowHeaderResponse<R> {
    /// The state pill, for a tooltip with details.
    pub status: Option<Response>,
    /// What the actions returned.
    pub inner: R,
}

impl<'a> WindowHeader<'a> {
    /// A header for the window `title` led by `icon` (document-text for
    /// logs, pencil-square for the editor).
    pub fn new(
        id_salt: impl std::hash::Hash + std::fmt::Debug,
        icon: Icon,
        title: &'a str,
    ) -> Self {
        Self {
            id: Id::new(("window-header", id_salt)),
            icon,
            title,
            meta: None,
            status: None,
            live_status: false,
            corner_radius: CornerRadius::ZERO,
        }
    }

    /// The muted line under the title: "Pod logs · container api · kube-system".
    pub fn meta(mut self, meta: &'a str) -> Self {
        self.meta = Some(meta);
        self
    }

    /// The window's state pill: "● Following", "● Modified".
    pub fn status(mut self, text: &'a str, tone: Tone) -> Self {
        self.status = Some((text, tone));
        self
    }

    /// Reserve the width of a pill whose words change (Following →
    /// Reconnecting…), so nothing beside it jumps ([`Pill::live`]).
    pub fn live_status(mut self) -> Self {
        self.live_status = true;
        self
    }

    /// Round the header's fill (its top corners, when the window itself is
    /// a rounded card).
    pub fn corner_radius(mut self, corner_radius: impl Into<CornerRadius>) -> Self {
        self.corner_radius = corner_radius.into();
        self
    }

    /// Show the header with `actions` (the window's mode or primary actions)
    /// on its right, laid out left to right.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        actions: impl FnOnce(&mut Ui) -> R,
    ) -> WindowHeaderResponse<R> {
        let theme = ui.tokens();
        let content = bar(
            ui,
            size::HEADER_BAR,
            theme.surface.sunken,
            Divider::Bottom,
            self.corner_radius,
        );
        let id = self.id;
        let (status, inner) = split_row(
            ui,
            id,
            content,
            HEADER_GAP,
            |ui| self.show_identity(ui),
            actions,
        );
        WindowHeaderResponse { status, inner }
    }

    /// The tile, title and meta, and pill.
    fn show_identity(&self, ui: &mut Ui) -> Option<Response> {
        let row = ui.max_rect();
        Tile::new(self.icon).size(TileSize::Sm).show(ui);

        let pill = self.status.map(|(text, tone)| {
            let pill = Pill::new(text, tone);
            if self.live_status { pill.live() } else { pill }
        });
        let pill_width = pill
            .as_ref()
            .map_or(0.0, |pill| pill.size(ui).x + HEADER_GAP);
        let max_width = (row.right() - ui.cursor().left() - pill_width).max(0.0);
        let mut stack = TitleStack::new(self.id, self.title, TITLE);
        if let Some(meta) = self.meta {
            stack = stack.meta(meta, type_scale::META);
        }
        stack.show(ui, max_width);
        pill.map(|pill| pill.show(ui))
    }
}

/// The toolbar of a document window.
#[must_use = "call `show` to add the toolbar"]
pub struct Toolbar {
    id: Id,
}

impl Toolbar {
    pub fn new(id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            id: Id::new(("toolbar", id_salt)),
        }
    }

    /// Show the toolbar: `leading` (search, filter, view options) from the
    /// left and `trailing` (navigation, mode toggles) ending at the right,
    /// both laid out left to right.
    pub fn show<L, T>(
        self,
        ui: &mut Ui,
        leading: impl FnOnce(&mut Ui) -> L,
        trailing: impl FnOnce(&mut Ui) -> T,
    ) -> (L, T) {
        let theme = ui.tokens();
        let content = bar(
            ui,
            size::TOOLBAR,
            theme.surface.canvas,
            Divider::Bottom,
            CornerRadius::ZERO,
        );
        split_row(ui, self.id, content, TOOLBAR_GAP, leading, trailing)
    }
}

/// A vertical divider between toolbar groups (`.vr`).
pub fn toolbar_divider(ui: &mut Ui) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(size::HAIRLINE, size::TOOLBAR_DIVIDER), Sense::hover());
    ui.painter().rect_filled(rect, 0, ui.tokens().divider.base);
    response
}

/// Work in progress in a toolbar: a small spinner and muted words,
/// "Validating…".
pub fn toolbar_activity(ui: &mut Ui, text: &str) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = ACTIVITY_GAP;
        Spinner::new().size(size::SPINNER_SM).show(ui);
        type_scale::META.label(ui, text, ui.tokens().text.muted)
    })
    .inner
}

/// The status bar of a document window.
#[must_use = "call `show` to add the status bar"]
pub struct StatusBar {
    id: Id,
    corner_radius: CornerRadius,
}

impl StatusBar {
    pub fn new(id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            id: Id::new(("status-bar", id_salt)),
            corner_radius: CornerRadius::ZERO,
        }
    }

    /// Round the bar's fill (its bottom corners, when the window itself is
    /// a rounded card).
    pub fn corner_radius(mut self, corner_radius: impl Into<CornerRadius>) -> Self {
        self.corner_radius = corner_radius.into();
        self
    }

    /// Show the status bar: `leading` (lines, matches, backfill) from the
    /// left and `trailing` (selection, position, format) ending at the
    /// right. Add items with [`status_text`], [`status_number`],
    /// [`status_icon`] and [`problems_toggle`].
    pub fn show<L, T>(
        self,
        ui: &mut Ui,
        leading: impl FnOnce(&mut Ui) -> L,
        trailing: impl FnOnce(&mut Ui) -> T,
    ) -> (L, T) {
        let theme = ui.tokens();
        let content = bar(
            ui,
            size::STATUS_BAR,
            theme.surface.sunken,
            Divider::Top,
            self.corner_radius,
        );
        split_row(ui, self.id, content, STATUS_GAP, leading, trailing)
    }
}

/// A status bar item: muted 12/16 words.
pub fn status_text(ui: &mut Ui, text: &str) -> Response {
    STATUS_TEXT.label(ui, text, ui.tokens().text.muted)
}

/// A status bar count or position in tabular figures ("1,284 lines",
/// "Ln 4, Col 12"), so it does not jitter as it changes.
pub fn status_number(ui: &mut Ui, text: &str) -> Response {
    STATUS_TEXT
        .tabular()
        .label(ui, text, ui.tokens().text.muted)
}

/// A muted icon before a status bar item; it is decorative.
pub fn status_icon(ui: &mut Ui, icon: Icon) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size::ICON_SM), Sense::hover());
    icon.paint(ui, rect, ui.tokens().text.muted);
    response
}

/// A status bar item that reports success: a check in `success-fill` before muted
/// words ("Valid").
pub fn status_ok(ui: &mut Ui, text: &str) -> Response {
    let theme = ui.tokens();
    let galley = STATUS_TEXT.galley(ui.ctx(), text, theme.text.muted, f32::INFINITY);
    let size = vec2(
        size::ICON_SM + PROBLEMS_GAP + galley.rect.width(),
        galley.rect.height(),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let icon = Rect::from_min_size(
            pos2(rect.left(), rect.center().y - size::ICON_SM / 2.0),
            Vec2::splat(size::ICON_SM),
        );
        Icon::CheckCircle.paint(ui, icon, theme.success.fill);
    }
    type_scale::paint_text_as(
        ui,
        &response,
        pos2(rect.left() + size::ICON_SM + PROBLEMS_GAP, rect.top()),
        galley,
        theme.text.muted,
        egui::accesskit::Role::Label,
    );
    response
}

/// The problems count of the status bar, in `danger-text` with its icon.
/// Clicking it toggles the Problems panel.
pub fn problems_toggle(ui: &mut Ui, count: usize) -> Response {
    let theme = ui.tokens();
    let text = if count == 1 {
        "1 problem".to_owned()
    } else {
        format!("{count} problems")
    };
    let galley = STATUS_TEXT.galley(ui.ctx(), &text, theme.danger.text, f32::INFINITY);
    let size = vec2(
        size::ICON_SM + PROBLEMS_GAP + galley.rect.width(),
        galley.rect.height(),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.with_pointing_hand();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), &text)
    });
    if ui.is_rect_visible(rect) {
        let icon = Rect::from_min_size(
            pos2(rect.left(), rect.center().y - size::ICON_SM / 2.0),
            Vec2::splat(size::ICON_SM),
        );
        Icon::XCircle.paint(ui, icon, theme.danger.fill);
    }
    type_scale::paint_text_as(
        ui,
        &response,
        pos2(rect.left() + size::ICON_SM + PROBLEMS_GAP, rect.top()),
        galley,
        theme.danger.text,
        egui::accesskit::Role::Button,
    );
    response.tooltip("Show problems")
}
