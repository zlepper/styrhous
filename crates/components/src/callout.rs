//! Callouts (`.callout`): an inline, tinted note inside the content it
//! explains. Never dismissible.

use egui::{Align, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole};
use crate::design::{radius, size, space};
use crate::icons::Icon;

/// Horizontal padding of a callout.
const PADDING_X: f32 = space::S16;
/// Vertical padding of a callout.
const PADDING_Y: f32 = space::S12;
/// Gap between the icon and the text.
const ICON_GAP: f32 = space::ICON_TEXT;
/// Gap between the title and the body.
const TITLE_TO_BODY: f32 = space::S2;

/// The title of callouts and toasts (`.w6` at 13/20).
pub(crate) const TITLE: TextRole = type_scale::UI_STRONG;

/// The icon of a status in `tone`: the information circle for accent,
/// info and neutral notes, the triangle for warnings, the exclamation circle
/// for errors and the check circle for successes.
pub fn tone_icon(tone: Tone) -> Icon {
    match tone {
        Tone::Neutral | Tone::Accent | Tone::Info => Icon::InformationCircle,
        Tone::Success => Icon::CheckCircle,
        Tone::Warning => Icon::ExclamationTriangle,
        Tone::Danger => Icon::ExclamationCircle,
    }
}

/// An inline note in a [`Tone`]: its icon in the tone's fill, centred
/// vertically on a semibold title and a secondary body, on the tone's tint.
/// It spans the available width.
#[must_use = "call `show` to add the callout"]
pub struct Callout<'a> {
    tone: Tone,
    icon: Icon,
    title: &'a str,
    body: Option<&'a str>,
    body_role: TextRole,
    alert: bool,
}

impl<'a> Callout<'a> {
    /// A callout in `tone` titled `title`, with the tone's icon
    /// ([`tone_icon`]).
    pub fn new(tone: Tone, title: &'a str) -> Self {
        Self {
            tone,
            icon: tone_icon(tone),
            title,
            body: None,
            body_role: type_scale::UI,
            alert: false,
        }
    }

    /// The explanation under the title.
    pub fn body(mut self, body: &'a str) -> Self {
        self.body = Some(body);
        self
    }

    /// Set the body in 12 point meta text on the title's 20 point lines, for
    /// dense callouts inside dialogs.
    pub fn compact(mut self) -> Self {
        self.body_role = type_scale::META.with_size(type_scale::META.size(), TITLE.line_height());
        self
    }

    /// Announce the callout as an alert (a failure that just appeared in
    /// place, such as a rejected apply).
    pub fn alert(mut self) -> Self {
        self.alert = true;
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = icon;
        self
    }

    /// Add the callout.
    pub fn show(self, ui: &mut Ui) -> Response {
        self.show_with(ui, |_| {})
    }

    /// Add the callout with `extra` content (chips, a list) below its body.
    pub fn show_with(self, ui: &mut Ui, extra: impl FnOnce(&mut Ui)) -> Response {
        let theme = ui.tokens();
        let colors = theme.tone(self.tone);
        let outer = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 0.0));
        let background = ui.painter().add(egui::Shape::Noop);

        let text_left = outer.left() + PADDING_X + size::ICON + ICON_GAP;
        let text_rect = Rect::from_min_max(
            pos2(text_left, outer.top() + PADDING_Y),
            pos2(outer.right() - PADDING_X, f32::INFINITY),
        );
        let mut text_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(text_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        text_ui.spacing_mut().item_spacing = Vec2::ZERO;
        let title = TITLE.label(&mut text_ui, self.title, theme.text.primary);
        if self.alert {
            crate::a11y::describe_container(
                &text_ui,
                egui::accesskit::Role::Alert,
                crate::a11y::Name::LabelledBy(title.id),
                None,
            );
        }
        if let Some(body) = self.body {
            text_ui.add_space(TITLE_TO_BODY);
            self.body_role
                .label(&mut text_ui, body, theme.text.secondary);
        }
        extra(&mut text_ui);

        let rect = Rect::from_min_max(
            outer.min,
            pos2(
                outer.right(),
                text_ui.min_rect().bottom().max(text_rect.top()) + PADDING_Y,
            ),
        );
        ui.painter().set(
            background,
            egui::epaint::RectShape::filled(rect, radius::CALLOUT, colors.tint),
        );
        let icon_rect = Rect::from_center_size(
            pos2(outer.left() + PADDING_X + size::ICON / 2.0, rect.center().y),
            Vec2::splat(size::ICON),
        );
        self.icon.paint(ui, icon_rect, colors.fill);
        ui.allocate_rect(rect, Sense::hover())
    }
}
