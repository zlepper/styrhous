//! The blade's own surface (C10): header, body and footer, and the compact
//! peek a history blade shows behind the active one.
//!
//! [`BladeStack`](super::BladeStack) lays its layers out with these; design
//! boards and previews paint them in place with [`BladeFrame`].

use egui::{
    Align, Color32, CornerRadius, Layout, Rect, Response, Sense, Ui, UiBuilder, pos2, vec2,
};

use super::interaction::HeaderAction;
use crate::a11y::{Name, describe_container};
use crate::control::with_opacity;
use crate::design::theme::{self, ThemeExt, ThemeScope};
use crate::design::type_scale::{self, Weight};
use crate::design::{radius, size, space};
use crate::icons::Icon;
use crate::{Button, ButtonSize, ButtonVariant, kind_badge};

/// Horizontal padding of the header.
pub(super) const HEADER_PADDING_X: f32 = space::S12;
/// Gap between the header's items.
const HEADER_GAP: f32 = space::S8;
/// Gap between the Back and Forward buttons.
const NAV_GAP: f32 = space::S2;
/// Margin on each side of the rule after the navigation.
const RULE_MARGIN: f32 = space::S4;
/// Opacity of a navigation button with nowhere to go.
const NAV_UNAVAILABLE_OPACITY: f32 = 0.35;
/// Padding of the body (`.blade-b`): 20 points top and bottom, 24 at the sides.
pub const BODY_PADDING: egui::Margin = egui::Margin::symmetric(24, 20);
/// Horizontal padding of the footer.
const FOOTER_PADDING_X: f32 = space::S24;
/// Horizontal padding of a peek's header.
const PEEK_PADDING_X: f32 = space::S12;
/// Gap between a peek's kind badge and name.
const PEEK_GAP: f32 = space::S4;

/// The Back and Forward state of a blade header, and the names its buttons
/// are read by.
#[derive(Clone, Copy, Debug)]
pub struct BladeNavigation<'a> {
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub back_label: &'a str,
    pub forward_label: &'a str,
    pub close_label: &'a str,
}

impl BladeNavigation<'_> {
    /// The navigation of the active blade.
    pub fn new(can_go_back: bool, can_go_forward: bool) -> Self {
        Self {
            can_go_back,
            can_go_forward,
            back_label: "Back",
            forward_label: "Forward",
            close_label: "Close blade",
        }
    }
}

/// A small ghost icon button of the header.
fn header_button(icon: Icon, label: &str) -> Button {
    Button::icon(icon, label)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
}

/// The header (`.blade-h`): Back and Forward, a rule, `content` (a kind
/// badge and title, then any actions) and Close, [`size::BLADE_HEADER`] tall
/// above a divider, filling the width of `ui`.
pub(super) fn show_header<H>(
    ui: &mut Ui,
    navigation: BladeNavigation<'_>,
    content: impl FnOnce(&mut Ui) -> H,
) -> (H, HeaderAction) {
    let theme = ui.tokens();
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::BLADE_HEADER),
        Sense::hover(),
    );
    let divider = Rect::from_min_max(pos2(rect.left(), rect.bottom() - size::HAIRLINE), rect.max);
    ui.painter().rect_filled(divider, 0, theme.divider.base);
    let row = Rect::from_min_max(
        pos2(rect.left() + HEADER_PADDING_X, rect.top()),
        pos2(rect.right() - HEADER_PADDING_X, divider.top()),
    );
    let mut row_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(row)
            .layout(Layout::left_to_right(Align::Center)),
    );
    row_ui.spacing_mut().item_spacing = vec2(HEADER_GAP, 0.0);
    let mut action = navigation_buttons(&mut row_ui, navigation, RULE_MARGIN);

    let content_rect = Rect::from_min_max(
        pos2(row_ui.cursor().left(), row.top()),
        pos2(row.right() - close_width() - HEADER_GAP, row.bottom()),
    );
    let mut content_ui = row_ui.new_child(
        UiBuilder::new()
            .max_rect(content_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    content_ui.spacing_mut().item_spacing = vec2(HEADER_GAP, 0.0);
    let inner = content(&mut content_ui);

    if close_button_at_end(&mut row_ui, row, navigation.close_label) {
        action = HeaderAction::Close;
    }
    (inner, action)
}

/// Back and Forward (an unavailable one faded to 35 %), then the rule that
/// ends a header's navigation, in `ui`'s horizontal row; returns the
/// navigation button that was clicked.
pub(super) fn navigation_buttons(
    ui: &mut Ui,
    navigation: BladeNavigation<'_>,
    rule_margin: f32,
) -> HeaderAction {
    let theme = ui.tokens();
    let mut action = HeaderAction::None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = NAV_GAP;
        for (enabled, icon, label, target) in [
            (
                navigation.can_go_back,
                Icon::ArrowLeft,
                navigation.back_label,
                HeaderAction::Back,
            ),
            (
                navigation.can_go_forward,
                Icon::ArrowRight,
                navigation.forward_label,
                HeaderAction::Forward,
            ),
        ] {
            let opacity = if enabled {
                1.0
            } else {
                NAV_UNAVAILABLE_OPACITY / ui.visuals().disabled_alpha()
            };
            let clicked = with_opacity(ui, opacity, |ui| {
                header_button(icon, label)
                    .enabled(enabled)
                    .show(ui)
                    .clicked()
            });
            if clicked {
                action = target;
            }
        }
    });
    ui.add_space(rule_margin);
    let (rule, _) =
        ui.allocate_exact_size(vec2(size::HAIRLINE, size::VERTICAL_RULE), Sense::hover());
    ui.painter().rect_filled(rule, 0, theme.divider.base);
    ui.add_space(rule_margin);
    action
}

/// The width of a header's Close button.
pub(super) fn close_width() -> f32 {
    ButtonSize::Sm.height()
}

/// The Close button of a header at the end of its `row`; returns whether
/// it was clicked.
pub(super) fn close_button_at_end(ui: &mut Ui, row: Rect, label: &str) -> bool {
    let mut close_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(row.right() - close_width(), row.top()),
                row.max,
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    Button::close(label).show(&mut close_ui).clicked()
}

/// A blade's title (`.t-title.trunc`), led by its resource `kind` badge and
/// filling the rest of the header. The title is cut with an ellipsis.
pub fn blade_title(ui: &mut Ui, kind: Option<&str>, title: &str) -> Response {
    title_before(ui, kind, title, 0.0)
}

/// A [`blade_title`] with `actions` (More, Refresh) at the end of the
/// header: they are laid out first, from the right, and the title takes the
/// room they leave.
pub fn blade_title_with_actions<R>(
    ui: &mut Ui,
    kind: Option<&str>,
    title: &str,
    actions: impl FnOnce(&mut Ui) -> R,
) -> R {
    let spacing = ui.spacing().item_spacing;
    let mut actions_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(ui.max_rect())
            .layout(Layout::right_to_left(Align::Center)),
    );
    actions_ui.spacing_mut().item_spacing = spacing;
    let inner = actions(&mut actions_ui);
    let trailing = actions_ui.min_rect().width() + spacing.x;
    title_before(ui, kind, title, trailing);
    inner
}

/// The kind badge and title, up to `trailing` points before the header's end.
fn title_before(ui: &mut Ui, kind: Option<&str>, title: &str, trailing: f32) -> Response {
    if let Some(kind) = kind {
        kind_badge(ui, kind);
    }
    let theme = ui.tokens();
    let role = type_scale::TITLE;
    let width = (ui.available_width() - trailing).max(0.0);
    let galley = role.galley_elided(ui.ctx(), title, theme.text.primary, width, 1);
    let row = ui.max_rect();
    let baseline = role.centred_baseline(row.height(), ui.pixels_per_point());
    let top = row.top() + baseline - role.css_baseline(ui.pixels_per_point());
    let rect = Rect::from_min_size(
        pos2(ui.cursor().left(), top),
        vec2(width, role.line_height()),
    );
    let response = type_scale::allocate_text_at(ui, rect, title);
    type_scale::paint_text(ui, &response, rect.min, galley, theme.text.primary);
    response
}

/// How much of the bottom of a blade body [`blade_body_footer`] takes: the
/// footer's height less the body's bottom padding it covers.
pub const BLADE_BODY_FOOTER: f32 = size::BLADE_FOOTER - BODY_PADDING.bottom as f32;

/// The blade footer ([`blade_footer`]) at the bottom of a [`BladeStack`](super::BladeStack)
/// body: it takes the last [`BLADE_BODY_FOOTER`] points of the body column
/// and runs out to the blade's edges over the body's padding. Leave that
/// much room under the body's content.
pub fn blade_body_footer<R>(ui: &mut Ui, add_buttons: impl FnOnce(&mut Ui) -> R) -> R {
    let column = ui.available_rect_before_wrap();
    let bottom = column.bottom() + f32::from(BODY_PADDING.bottom);
    let rect = Rect::from_min_max(
        pos2(
            column.left() - f32::from(BODY_PADDING.left),
            bottom - size::BLADE_FOOTER,
        ),
        pos2(column.right() + f32::from(BODY_PADDING.right), bottom),
    );
    ui.allocate_rect(
        Rect::from_min_max(pos2(column.left(), rect.top()), column.right_bottom()),
        Sense::hover(),
    );
    blade_footer(ui, rect, add_buttons)
}

/// The footer (`.blade-f`) for blades holding unsaved input:
/// [`size::BLADE_FOOTER`] tall on `surface-sunken` under a divider, with
/// `add_buttons` laid out from the right (add the primary action first).
pub fn blade_footer<R>(ui: &mut Ui, rect: Rect, add_buttons: impl FnOnce(&mut Ui) -> R) -> R {
    ui.painter()
        .add(crate::frames::sunken_footer(&ui.tokens(), rect));
    let row = Rect::from_min_max(
        pos2(rect.left() + FOOTER_PADDING_X, rect.top() + size::HAIRLINE),
        pos2(rect.right() - FOOTER_PADDING_X, rect.bottom()),
    );
    let mut footer_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(row)
            .layout(Layout::right_to_left(Align::Center)),
    );
    footer_ui.spacing_mut().item_spacing = vec2(space::S8, 0.0);
    add_buttons(&mut footer_ui)
}

/// Paint the surface of a blade: the overlay elevation and `fill`.
pub(super) fn paint_surface(ui: &Ui, rect: Rect, fill: Color32) {
    let theme = ui.tokens();
    theme
        .shadow
        .overlay
        .paint(ui.painter(), rect, radius::OVERLAY);
    ui.painter().rect_filled(rect, radius::OVERLAY, fill);
}

/// A blade painted in place at `rect` (design boards and previews): its
/// surface, header, body and optional footer. The live stack is
/// [`BladeStack`](super::BladeStack).
#[must_use = "call `show` to paint the blade"]
pub struct BladeFrame<'a> {
    rect: Rect,
    label: &'a str,
    navigation: BladeNavigation<'a>,
}

impl<'a> BladeFrame<'a> {
    /// A blade filling `rect`, read as the dialog `label` (its title).
    pub fn new(rect: Rect, label: &'a str, navigation: BladeNavigation<'a>) -> Self {
        Self {
            rect,
            label,
            navigation,
        }
    }

    /// Paint the blade: `header` fills the header between the navigation
    /// and Close, `body` the padded body and `footer` (if any) the footer's
    /// buttons from the right.
    pub fn show(
        self,
        ui: &mut Ui,
        header: impl FnOnce(&mut Ui),
        body: impl FnOnce(&mut Ui),
        footer: Option<&mut dyn FnMut(&mut Ui)>,
    ) {
        let mut blade_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(("blade-frame", self.label))
                .max_rect(self.rect)
                .layout(Layout::top_down(Align::Min)),
        );
        blade_ui.set_clip_rect(self.rect.intersect(ui.clip_rect()));
        describe_container(
            &blade_ui,
            egui::accesskit::Role::Dialog,
            Name::Label(self.label),
            Some(self.rect),
        );
        let theme = blade_ui.tokens();
        // The shadow lies outside the blade's clip.
        paint_surface(ui, self.rect, theme.surface.panel);
        theme::scope(&mut blade_ui, ThemeScope::Blade, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let _ = show_header(ui, self.navigation, header);
            let footer_height = if footer.is_some() {
                size::BLADE_FOOTER
            } else {
                0.0
            };
            let body_rect = Rect::from_min_max(
                ui.cursor().min,
                pos2(self.rect.right(), self.rect.bottom() - footer_height),
            )
            .shrink2(vec2(
                f32::from(BODY_PADDING.left),
                f32::from(BODY_PADDING.top),
            ));
            let mut body_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(body_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            body(&mut body_ui);
            if let Some(footer) = footer {
                let rect = Rect::from_min_max(
                    pos2(self.rect.left(), self.rect.bottom() - footer_height),
                    self.rect.max,
                );
                blade_footer(ui, rect, footer);
            }
        });
    }
}

/// A history blade peeking out from behind the next one (`.blade.peek`): a
/// sunken blade at `rect` whose compact header shows its `kind` badge and
/// `name` (cut to [`size::BLADE_PEEK_NAME`]), with `shade` laid over the
/// [`size::BLADE_PEEK`] points that show.
pub fn paint_blade_peek(ui: &mut Ui, rect: Rect, kind: Option<&str>, name: &str, shade: Color32) {
    paint_blade_peek_on(ui, rect, kind.map(|kind| (kind, None)), name, shade);
}

/// A [`paint_blade_peek`] whose kind badge is on the kind's category `fill`.
pub fn paint_blade_peek_on(
    ui: &mut Ui,
    rect: Rect,
    kind: Option<(&str, Option<Color32>)>,
    name: &str,
    shade: Color32,
) {
    let theme = ui.tokens();
    paint_surface(ui, rect, theme.surface.sunken);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), size::BLADE_PEEK_HEADER));
    ui.painter().rect_filled(
        Rect::from_min_max(
            pos2(header.left(), header.bottom() - size::HAIRLINE),
            header.max,
        ),
        0,
        theme.divider.base,
    );
    let name_role = type_scale::META.with_weight(Weight::Medium);
    let name_galley =
        name_role.galley_elided(ui.ctx(), name, theme.text.primary, size::BLADE_PEEK_NAME, 1);
    let badge_height = if kind.is_some() {
        size::PILL_HEIGHT + PEEK_GAP
    } else {
        0.0
    };
    let content_height = badge_height + name_role.line_height();
    let top = header.top() + (header.height() - size::HAIRLINE - content_height) / 2.0;
    let mut column = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(header.left() + PEEK_PADDING_X, top),
                pos2(header.right() - PEEK_PADDING_X, header.bottom()),
            ))
            .layout(Layout::top_down(Align::Min)),
    );
    column.spacing_mut().item_spacing = vec2(0.0, PEEK_GAP);
    if let Some((kind, fill)) = kind {
        match fill {
            Some(fill) => crate::badge::kind_badge_on(&mut column, kind, fill),
            None => kind_badge(&mut column, kind),
        };
    }
    let (name_rect, response) = type_scale::allocate_text(
        &mut column,
        vec2(name_galley.rect.width(), name_role.line_height()),
        name,
    );
    type_scale::paint_text(
        &column,
        &response,
        name_rect.min,
        name_galley,
        theme.text.primary,
    );

    let strip = Rect::from_min_size(rect.min, vec2(size::BLADE_PEEK, rect.height()));
    let corner = CornerRadius {
        nw: radius::OVERLAY,
        sw: radius::OVERLAY,
        ..CornerRadius::ZERO
    };
    ui.painter().rect_filled(strip, corner, shade);
}
