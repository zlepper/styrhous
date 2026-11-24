//! The floating workspace sidebar (C09 `.side`): the cluster card, the kind
//! filter, captions and the kind list, and the Notifications/Settings footer.

use egui::{
    Align, Color32, Layout, Mesh, Rect, Response, Sense, Ui, UiBuilder, Vec2, WidgetInfo,
    WidgetType, pos2, vec2,
};

use crate::design::theme::{self, ThemeExt, ThemeScope, Tone};
use crate::design::type_scale::{self, TextRole, Typeface, Weight};
use crate::design::{radius, size, space};
use crate::icons::Icon;
use crate::interaction::{ControlState, Interaction};
use crate::tooltip::TooltipExt;
use crate::{Avatar, CountBadge, Keycap, PointingHand, TextInput};

/// Gap between the items of a row (`.sv`, `.ccard`).
const ITEM_GAP: f32 = space::ICON_TEXT;
/// Horizontal margin around a kind's colour square (`.kc` margin 0 4).
const SWATCH_MARGIN: f32 = space::S4;
/// Gap between the cluster card's avatar and its text.
const CARD_GAP: f32 = space::S12;
/// Gap above the kind filter.
const FILTER_TOP: f32 = space::S12;
/// Gap between the footer's items.
const FOOTER_GAP: f32 = space::S4;
/// Gap above the footer.
const FOOTER_TOP: f32 = space::S8;
/// Padding of a section caption (`.scap`: 14 8 6).
const CAPTION_PADDING: egui::Margin = egui::Margin {
    left: 8,
    right: 8,
    top: 14,
    bottom: 6,
};

/// A kind item's label (`.sv`: Figtree 13.5 medium).
const ITEM: TextRole = type_scale::UI_CONTROL.with_size(13.5, 20.0);
/// A group or child item's label (13 medium).
const ITEM_SM: TextRole = type_scale::UI_CONTROL;
/// An API group's name (Plex Mono 11.5 medium).
const API_GROUP: TextRole = type_scale::MONO
    .with_weight(Weight::Medium)
    .with_size(11.5, 20.0);
/// A kind count (`.c`: Plex Mono 11).
const COUNT: TextRole = type_scale::MONO.with_size(11.0, 20.0);
/// A section caption (`.scap`: 11/14, +0.1 em, upper case).
const CAPTION: TextRole = type_scale::CAPTION.with_size(11.0, 14.0);
/// The cluster card's name (`.ccard .nm`: Bricolage 15/20 bold).
const CLUSTER_NAME: TextRole = TextRole::new(Typeface::Display, Weight::Bold, 15.0, 20.0);
/// The cluster card's meta line (`.ccard .mt`: 11/14, +0.06 em, upper case).
const CLUSTER_META: TextRole = type_scale::CAPTION
    .with_weight(Weight::Regular)
    .with_size(11.0, 14.0)
    .with_letter_spacing(0.06);

/// What leads a [`SideItem`].
#[derive(Clone, Copy, Debug)]
enum Lead {
    /// A kind's colour square (`.kc`).
    Kind(Color32),
    /// A group's disclosure chevron.
    Chevron { expanded: bool },
    /// A footer item's icon.
    Icon(Icon),
    /// Nothing: a child item, indented under its group's label.
    Indent,
    /// An empty slot as wide as a group's chevron, so a kind without a
    /// colour lines up with the groups and their children.
    Blank,
}

/// An item of the sidebar (`.sv`): a kind with its colour, a group of kinds,
/// a child kind, an API group of custom resources, or a footer action. Counts
/// sit at the right in mono; a selected item uses `nav-item-selected` with its
/// count in `nav-accent`, never a solid accent fill.
#[must_use = "call `show` to add the item"]
pub struct SideItem<'a> {
    label: &'a str,
    lead: Lead,
    role: TextRole,
    height: f32,
    secondary: bool,
    count: Option<usize>,
    count_tone: Option<Tone>,
    alert_count: Option<usize>,
    lamp: Option<Tone>,
    selected: bool,
    forced_state: Option<ControlState>,
    width: Width,
}

/// How wide a [`SideItem`] is.
#[derive(Clone, Copy, Debug)]
enum Width {
    Fill,
    Hug,
    Exact(f32),
}

impl<'a> SideItem<'a> {
    fn new(label: &'a str, lead: Lead, role: TextRole, height: f32) -> Self {
        Self {
            label,
            lead,
            role,
            height,
            secondary: false,
            count: None,
            count_tone: None,
            alert_count: None,
            lamp: None,
            selected: false,
            forced_state: None,
            width: Width::Fill,
        }
    }

    /// A resource kind led by its category colour (`theme.category.nth`).
    pub fn kind(label: &'a str, color: Color32) -> Self {
        Self::new(label, Lead::Kind(color), ITEM, size::SIDE_ITEM)
    }

    /// A group of kinds (Workloads, Network), collapsed or expanded.
    pub fn group(label: &'a str, expanded: bool) -> Self {
        Self {
            secondary: true,
            ..Self::new(
                label,
                Lead::Chevron { expanded },
                ITEM_SM,
                size::SIDE_ITEM_SM,
            )
        }
    }

    /// A group of custom resources named by its API group, in mono.
    pub fn api_group(group: &'a str, expanded: bool) -> Self {
        Self {
            role: API_GROUP,
            ..Self::group(group, expanded)
        }
    }

    /// A resource kind without a category colour, its label lined up with the
    /// group labels around it.
    pub fn plain(label: &'a str) -> Self {
        Self::new(label, Lead::Blank, ITEM, size::SIDE_ITEM)
    }

    /// A kind inside an expanded group, indented under the group's label.
    pub fn child(label: &'a str) -> Self {
        Self::new(label, Lead::Indent, ITEM_SM, size::SIDE_ITEM_SM)
    }

    /// A footer action (Notifications, Settings), led by its icon.
    pub fn action(icon: Icon, label: &'a str) -> Self {
        Self::new(label, Lead::Icon(icon), ITEM, size::SIDE_ITEM)
    }

    /// The number of resources, at the right in mono.
    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    /// Show the count in `tone`'s text colour (danger for a kind with
    /// problems).
    pub fn count_tone(mut self, tone: Tone) -> Self {
        self.count_tone = Some(tone);
        self
    }

    /// An unread count in a danger count badge at the right (Notifications).
    pub fn alert_count(mut self, count: usize) -> Self {
        self.alert_count = Some(count);
        self
    }

    /// A status lamp on the top-right corner of a footer item's icon (an
    /// update or licence that needs attention on Settings).
    pub fn lamp(mut self, tone: Tone) -> Self {
        self.lamp = Some(tone);
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Paint `state` whatever the pointer does (design boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    /// Size the item to its content instead of filling the row (the footer's
    /// Settings).
    pub fn hug(mut self) -> Self {
        self.width = Width::Hug;
        self
    }

    /// Make the item exactly `width` points wide (the footer's
    /// Notifications, which takes the room Settings leaves).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Width::Exact(width);
        self
    }

    /// The width of the item hugging its content.
    pub fn content_width(&self, ui: &Ui) -> f32 {
        let label = self
            .label_role()
            .galley(
                ui.ctx(),
                self.label,
                ui.tokens().text.primary,
                f32::INFINITY,
            )
            .rect
            .width();
        let count = self.count.map_or(0.0, |count| {
            COUNT
                .galley(
                    ui.ctx(),
                    &count.to_string(),
                    ui.tokens().text.primary,
                    f32::INFINITY,
                )
                .rect
                .width()
        });
        let badge = self
            .alert_count
            .map_or(0.0, |count| CountBadge::new(count).size(ui).x);
        let trailing = count + badge;
        let trailing_gap = if trailing > 0.0 { ITEM_GAP } else { 0.0 };
        2.0 * size::SIDE_ITEM_PADDING_X + self.lead_width() + label + trailing_gap + trailing
    }

    fn label_role(&self) -> TextRole {
        if self.selected {
            self.role.with_weight(Weight::SemiBold)
        } else {
            self.role
        }
    }

    fn lead_width(&self) -> f32 {
        match self.lead {
            Lead::Kind(_) => size::KIND_SWATCH + 2.0 * SWATCH_MARGIN + ITEM_GAP,
            Lead::Chevron { .. } | Lead::Blank => size::ICON_XS + ITEM_GAP,
            Lead::Icon(_) => size::ICON_SM + ITEM_GAP,
            Lead::Indent => size::SIDE_CHILD_INDENT - size::SIDE_ITEM_PADDING_X,
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let role = self.label_role();
        let label_color = if self.selected {
            theme.text.primary
        } else if self.secondary {
            theme.nav.text_secondary
        } else {
            theme.nav.text
        };
        let count = self.count.map(|count| {
            let color = match (self.selected, self.count_tone) {
                (true, _) => theme.nav.accent,
                (false, Some(tone)) => theme.tone(tone).text,
                (false, None) => theme.nav.text_muted,
            };
            (
                COUNT.galley(ui.ctx(), &count.to_string(), color, f32::INFINITY),
                color,
            )
        });
        let badge = self
            .alert_count
            .map(|count| CountBadge::new(count).tone(Tone::Danger));
        let width = match self.width {
            Width::Fill => ui.available_width(),
            Width::Hug => self.content_width(ui),
            Width::Exact(width) => width,
        };
        // The label takes what the lead, the count and the padding leave.
        let trailing = count
            .as_ref()
            .map_or(0.0, |(galley, _)| galley.rect.width())
            + badge.as_ref().map_or(0.0, |badge| badge.size(ui).x);
        let trailing_gap = if trailing > 0.0 { ITEM_GAP } else { 0.0 };
        let label_width = match self.width {
            Width::Hug => f32::INFINITY,
            Width::Fill | Width::Exact(_) => (width
                - 2.0 * size::SIDE_ITEM_PADDING_X
                - self.lead_width()
                - trailing
                - trailing_gap)
                .max(0.0),
        };
        let label = role.galley_elided(ui.ctx(), self.label, label_color, label_width, 1);
        let elided = label.elided;
        let (rect, response) = ui.allocate_exact_size(vec2(width, self.height), Sense::click());
        let response = response.with_pointing_hand();
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::Button,
                ui.is_enabled(),
                self.selected,
                self.label,
            )
        });
        if let Lead::Chevron { expanded } = self.lead {
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_expanded(expanded));
        }
        let response = if elided {
            response.tooltip(self.label)
        } else {
            response
        };
        let description = match (self.count, self.alert_count) {
            (_, Some(unread)) => Some(format!("{unread} unread")),
            (Some(count), None) => Some(count.to_string()),
            (None, None) => None,
        };
        if let Some(description) = description {
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_description(description));
        }
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let state = Interaction::of(&response, self.forced_state, false);
        let fill = if self.selected {
            theme.nav.item_selected
        } else if state.is_active() {
            theme.nav.item_hover
        } else {
            Color32::TRANSPARENT
        };
        if fill.a() > 0 {
            ui.painter().rect_filled(rect, radius::SIDE_ITEM, fill);
        }

        let centre = rect.center().y;
        let mut x = rect.left() + size::SIDE_ITEM_PADDING_X;
        let lead_color = if self.selected {
            theme.nav.accent
        } else {
            theme.nav.text_muted
        };
        match self.lead {
            Lead::Kind(color) => {
                let swatch = Rect::from_center_size(
                    pos2(x + SWATCH_MARGIN + size::KIND_SWATCH / 2.0, centre),
                    Vec2::splat(size::KIND_SWATCH),
                );
                ui.painter().rect_filled(swatch, radius::KIND_SWATCH, color);
            }
            Lead::Chevron { expanded } => {
                let icon = if expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                };
                let glyph = Rect::from_min_size(
                    pos2(x, centre - size::ICON_XS / 2.0),
                    Vec2::splat(size::ICON_XS),
                );
                icon.paint(ui, glyph, lead_color);
            }
            Lead::Icon(icon) => {
                let glyph = Rect::from_min_size(
                    pos2(x, centre - size::ICON_SM / 2.0),
                    Vec2::splat(size::ICON_SM),
                );
                icon.paint(ui, glyph, lead_color);
                if let Some(tone) = self.lamp {
                    let lamp =
                        Rect::from_center_size(glyph.right_top(), Vec2::splat(size::STATUS_DOT));
                    crate::paint_status_dot(ui.painter(), &theme, lamp, tone);
                }
            }
            Lead::Indent | Lead::Blank => {}
        }
        x += self.lead_width();
        let baseline = role.centred_baseline(self.height, ui.pixels_per_point());
        let top = rect.top() + baseline - role.css_baseline(ui.pixels_per_point());
        type_scale::expose_text_runs_as(
            ui,
            &response,
            pos2(x, top),
            &label,
            egui::accesskit::Role::Button,
        );
        type_scale::paint_galley(ui.painter(), pos2(x, top), label, label_color);

        let mut right = rect.right() - size::SIDE_ITEM_PADDING_X;
        if let Some(badge) = badge {
            let badge_size = badge.size(ui);
            let badge_rect = Rect::from_min_size(
                pos2(right - badge_size.x, centre - badge_size.y / 2.0),
                badge_size,
            );
            badge.paint(ui, badge_rect, fill);
            right = badge_rect.left();
        }
        if let Some((galley, color)) = count {
            let baseline = COUNT.centred_baseline(self.height, ui.pixels_per_point());
            let pos = pos2(
                right - galley.rect.width(),
                rect.top() + baseline - COUNT.css_baseline(ui.pixels_per_point()),
            );
            type_scale::paint_galley(ui.painter(), pos, galley, color);
        }
        response
    }
}

/// A section caption of the sidebar (`.scap`): "PINNED", "KUBERNETES 54",
/// with an optional `detail` (a count) at the right.
pub fn side_caption(ui: &mut Ui, label: &str, detail: Option<&str>) {
    let theme = ui.tokens();
    let color = theme.nav.text_muted;
    let height = CAPTION_PADDING.sum().y + CAPTION.line_height();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let top = rect.top() + f32::from(CAPTION_PADDING.top);
    let place = |ui: &mut Ui, text: &str, align_right: bool| {
        let galley = CAPTION.galley(ui.ctx(), text, color, f32::INFINITY);
        let left = if align_right {
            rect.right() - f32::from(CAPTION_PADDING.right) - galley.rect.width()
        } else {
            rect.left() + f32::from(CAPTION_PADDING.left)
        };
        let text_rect = Rect::from_min_size(pos2(left, top), galley.rect.size());
        let id = ui.id().with(("side-caption", text));
        let response = type_scale::interact_text(ui, text_rect, id, galley.text());
        type_scale::paint_text(ui, &response, text_rect.min, galley, color);
    };
    place(ui, label, false);
    if let Some(detail) = detail {
        place(ui, detail, true);
    }
}

/// What the cluster card shows.
enum CardContent<'a> {
    /// A connected or connecting cluster: its initials avatar.
    Cluster {
        initials: &'a str,
        avatar: usize,
        status: Tone,
    },
    /// No cluster chosen yet: a neutral server tile and the switcher's key.
    Empty { key: &'a str },
}

/// The cluster card (`.ccard`) on top of the sidebar: the cluster's avatar
/// with its connection lamp, name and meta line, and the up-down chevron of
/// the switcher it opens (C18).
#[must_use = "call `show` to add the card"]
pub struct ClusterCard<'a> {
    name: &'a str,
    meta: &'a str,
    content: CardContent<'a>,
    forced_state: Option<ControlState>,
}

impl<'a> ClusterCard<'a> {
    /// The card of cluster `name` with its `meta` line ("KIND · LOCAL · 2 of
    /// 4"), its `initials` on avatar colour `avatar` and a `status` lamp.
    pub fn new(
        name: &'a str,
        meta: &'a str,
        initials: &'a str,
        avatar: usize,
        status: Tone,
    ) -> Self {
        Self {
            name,
            meta,
            content: CardContent::Cluster {
                initials,
                avatar,
                status,
            },
            forced_state: None,
        }
    }

    /// The card before a cluster is chosen: `title` ("Choose a cluster"),
    /// `meta` ("4 contexts") and the switcher's shortcut `key`.
    pub fn empty(title: &'a str, meta: &'a str, key: &'a str) -> Self {
        Self {
            name: title,
            meta,
            content: CardContent::Empty { key },
            forced_state: None,
        }
    }

    /// Paint `state` whatever the pointer does (design boards).
    pub fn force_state(mut self, state: ControlState) -> Self {
        self.forced_state = Some(state);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let height = size::AVATAR + 2.0 * size::CLUSTER_CARD_PADDING;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
        let response = response.with_pointing_hand();
        response.widget_info(|| {
            WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), "Switch cluster")
        });
        let description = format!("{}, {}", self.name, self.meta);
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_description(description));
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let state = Interaction::of(&response, self.forced_state, false);
        let fill = if state.is_active() {
            theme.surface.raised_hover
        } else {
            theme.surface.raised
        };
        ui.painter().rect_filled(rect, radius::CLUSTER_CARD, fill);
        let inner = rect.shrink(size::CLUSTER_CARD_PADDING);
        let avatar = Rect::from_min_size(inner.min, Vec2::splat(size::AVATAR));
        let trailing_width = match &self.content {
            CardContent::Cluster {
                initials,
                avatar: index,
                status,
            } => {
                Avatar::new(initials)
                    .category(*index)
                    .status(*status)
                    .paint(ui, avatar.center());
                let chevron = Rect::from_center_size(
                    pos2(inner.right() - size::ICON_SM / 2.0, inner.center().y),
                    Vec2::splat(size::ICON_SM),
                );
                Icon::ChevronUpDown.paint(ui, chevron, theme.nav.text_muted);
                size::ICON_SM
            }
            CardContent::Empty { key } => {
                ui.painter()
                    .rect_filled(avatar, radius::AVATAR, theme.control.fill_strong);
                let glyph = Rect::from_center_size(avatar.center(), Vec2::splat(size::ICON));
                Icon::ServerStack.paint(ui, glyph, theme.text.muted);
                let keycap = Keycap::new(key);
                let key_size = keycap.size(ui);
                keycap.paint(
                    ui,
                    Rect::from_min_size(
                        pos2(
                            inner.right() - key_size.x,
                            inner.center().y - key_size.y / 2.0,
                        ),
                        key_size,
                    ),
                );
                key_size.x
            }
        };
        let text_left = avatar.right() + CARD_GAP;
        let text_width = (inner.right() - trailing_width - CARD_GAP - text_left).max(0.0);
        let text_height = CLUSTER_NAME.line_height() + CLUSTER_META.line_height();
        let top = inner.center().y - text_height / 2.0;
        let name =
            CLUSTER_NAME.galley_elided(ui.ctx(), self.name, theme.text.primary, text_width, 1);
        let meta =
            CLUSTER_META.galley_elided(ui.ctx(), self.meta, theme.nav.text_muted, text_width, 1);
        let name_pos = pos2(text_left, top);
        let meta_pos = pos2(text_left, top + CLUSTER_NAME.line_height());
        // The name is the text of the card; the meta line is its description.
        type_scale::expose_text_runs_as(
            ui,
            &response,
            name_pos,
            &name,
            egui::accesskit::Role::Button,
        );
        type_scale::paint_galley(ui.painter(), name_pos, name, theme.text.primary);
        type_scale::paint_galley(ui.painter(), meta_pos, meta, theme.nav.text_muted);
        response
    }
}

/// The sidebar's kind filter: a 34 point field with a funnel, a placeholder
/// naming how many kinds there are and the `G` shortcut, which focuses it
/// while nothing else has focus (S09).
pub fn kind_filter(ui: &mut Ui, query: &mut String, placeholder: &str) -> Response {
    let focus = ui.is_enabled()
        && ui.memory(|memory| memory.focused().is_none())
        && ui.input_mut(|input| {
            crate::interaction::consume_key_with_text(input, egui::Modifiers::NONE, egui::Key::G)
        });
    ui.add_space(FILTER_TOP);
    TextInput::new(query)
        .request_focus(focus)
        .id_salt("sidebar-kind-filter")
        .accessibility_label("Filter kinds")
        .placeholder(placeholder)
        .leading_icon(Icon::Funnel)
        .icon_size(size::ICON_SM)
        .height(size::SIDEBAR_FILTER)
        .trailing_keycap("G")
        .show(ui)
}

/// The floating workspace sidebar (`.side`): [`size::SIDEBAR`] wide on
/// `nav-surface` with [`radius::SIDEBAR`] corners, its contents
/// [`size::SIDEBAR_PADDING`] in. The body scrolls under a fade; the footer
/// stays at the bottom.
#[must_use = "call `show` to add the sidebar"]
pub struct FloatingSidebar {
    rect: Rect,
}

impl FloatingSidebar {
    /// A sidebar filling `rect` (12 points in from the window's edges).
    pub fn new(rect: Rect) -> Self {
        Self { rect }
    }

    /// Paint the sidebar with `body` (card, filter, sections) and `footer`
    /// (its items left to right, the last one hugging its content).
    pub fn show<R>(
        self,
        ui: &mut Ui,
        body: impl FnOnce(&mut Ui) -> R,
        footer: impl FnOnce(&mut Ui),
    ) -> R {
        let theme = ui.tokens();
        ui.painter()
            .rect_filled(self.rect, radius::SIDEBAR, theme.nav.surface);
        let mut side = ui.new_child(
            UiBuilder::new()
                .id_salt("floating-sidebar")
                .max_rect(self.rect)
                .layout(Layout::top_down(Align::Min)),
        );
        side.set_clip_rect(self.rect.intersect(ui.clip_rect()));
        crate::a11y::describe_container(
            &side,
            egui::accesskit::Role::Navigation,
            crate::a11y::Name::Label("Workspace"),
            Some(self.rect),
        );
        theme::scope(&mut side, ThemeScope::Sidebar, |ui| {
            let inner = self.rect.shrink(size::SIDEBAR_PADDING);
            let footer_top = inner.bottom() - size::SIDE_ITEM;
            let body_rect =
                Rect::from_min_max(inner.min, pos2(inner.right(), footer_top - FOOTER_TOP));
            let mut body_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(body_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            body_ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
            body_ui.spacing_mut().item_spacing = Vec2::ZERO;
            let inner_result = body(&mut body_ui);

            let fade = Rect::from_min_max(
                pos2(
                    self.rect.left(),
                    body_rect.bottom() + FOOTER_TOP - size::SIDEBAR_FADE,
                ),
                pos2(self.rect.right(), body_rect.bottom() + FOOTER_TOP),
            );
            paint_fade(ui, fade, theme.nav.surface);

            let mut footer_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(Rect::from_min_max(
                        pos2(inner.left(), footer_top),
                        inner.max,
                    ))
                    .layout(Layout::left_to_right(Align::Center)),
            );
            footer_ui.spacing_mut().item_spacing = vec2(FOOTER_GAP, 0.0);
            footer(&mut footer_ui);
            inner_result
        })
    }
}

/// A vertical fade from transparent to `color` over `rect` (CSS
/// `linear-gradient(transparent, color)`).
fn paint_fade(ui: &Ui, rect: Rect, color: Color32) {
    let mut mesh = Mesh::default();
    let clear = color.gamma_multiply(0.0);
    mesh.colored_vertex(rect.left_top(), clear);
    mesh.colored_vertex(rect.right_top(), clear);
    mesh.colored_vertex(rect.left_bottom(), color);
    mesh.colored_vertex(rect.right_bottom(), color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    ui.painter().add(mesh);
}

/// The sidebar's footer row (`.side-foot`): `fill` takes the room `hug`
/// (sized to its content) leaves, [`FOOTER_GAP`] apart. Returns both
/// responses.
pub fn side_footer(ui: &mut Ui, fill: SideItem<'_>, hug: SideItem<'_>) -> (Response, Response) {
    let hug = hug.hug();
    let fill_width = (ui.available_width() - hug.content_width(ui) - FOOTER_GAP).max(0.0);
    ui.spacing_mut().item_spacing.x = FOOTER_GAP;
    let fill = fill.width(fill_width).show(ui);
    let hug = hug.show(ui);
    (fill, hug)
}
