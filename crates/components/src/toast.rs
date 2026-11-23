//! Toasts (`.toast`): the outcome of something the user just did, stacked
//! in the window's bottom-right corner.

use egui::{
    Align, Align2, Area, Context, Id, Layout, Order, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2,
};

use crate::Button;
use crate::a11y::{Name, describe_container};
use crate::callout::TITLE;
use crate::design::theme::{ThemeExt, Tone};
use crate::design::{radius, size, space, type_scale};
use crate::icons::Icon;
use crate::mini_button::{MINI_BUTTON, MiniButton};

/// Width of a toast.
pub const TOAST_WIDTH: f32 = 360.0;
/// Gap between stacked toasts.
pub const TOAST_GAP: f32 = space::S8;
/// Distance of the toast stack from the window's right and bottom edges.
pub const TOAST_INSET: f32 = space::S16;
/// Padding of a toast: 12 points but 16 on the left.
const PADDING: egui::Margin = egui::Margin {
    left: 16,
    right: 12,
    top: 12,
    bottom: 12,
};
/// Gap between the icon, the text and the dismiss button.
const GAP: f32 = space::S12;
/// Gap between the title and the detail line.
const TITLE_TO_DETAIL: f32 = space::S2;
/// Gap above the action link.
const ACTION_TOP: f32 = space::S8;

/// The icon of a toast in `tone`: the check circle for successes, the X
/// circle for failures, the triangle for warnings, otherwise the
/// information circle.
pub fn toast_icon(tone: Tone) -> Icon {
    match tone {
        Tone::Success => Icon::CheckCircle,
        Tone::Danger => Icon::XCircle,
        Tone::Warning => Icon::ExclamationTriangle,
        Tone::Neutral | Tone::Accent | Tone::Info => Icon::InformationCircle,
    }
}

/// What the user did with a [`Toast`] this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToastResponse {
    /// The action link was clicked.
    pub action: bool,
    /// The dismiss button was clicked.
    pub dismissed: bool,
}

/// A toast (`.toast`): the tone's icon centred on a semibold title, an
/// optional detail line and action link, and a dismiss button, on a raised
/// surface [`TOAST_WIDTH`] wide.
#[must_use = "call `show` to add the toast"]
pub struct Toast<'a> {
    tone: Tone,
    title: &'a str,
    detail: Option<&'a str>,
    action: Option<&'a str>,
    dismissible: bool,
    id: Option<Id>,
}

impl<'a> Toast<'a> {
    /// A dismissible toast in `tone` titled `title`.
    pub fn new(tone: Tone, title: &'a str) -> Self {
        Self {
            tone,
            title,
            detail: None,
            action: None,
            dismissible: true,
            id: None,
        }
    }

    /// The secondary line under the title (the target, or the reason).
    pub fn detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail);
        self
    }

    /// A link action under the text ("View job").
    pub fn action(mut self, label: &'a str) -> Self {
        self.action = Some(label);
        self
    }

    /// Identify the toast by `salt` (an operation's sequence number) instead
    /// of its words, so toasts that read the same keep their own state.
    pub fn id_salt(mut self, salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = Some(Id::new(salt));
        self
    }

    /// Whether the toast has a dismiss button; true by default.
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    /// Add the toast at the cursor.
    pub fn show(self, ui: &mut Ui) -> ToastResponse {
        let outer = Rect::from_min_size(ui.cursor().min, vec2(TOAST_WIDTH, 0.0));
        let mut toast_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(
                    self.id
                        .unwrap_or_else(|| Id::new(("toast", self.title, self.detail))),
                )
                .max_rect(outer)
                .layout(Layout::top_down(Align::Min)),
        );
        let (rect, response, title) = self.paint(&mut toast_ui, outer);
        describe_container(
            &toast_ui,
            egui::accesskit::Role::Status,
            Name::LabelledBy(title),
            None,
        );
        ui.advance_cursor_after_rect(rect);
        response
    }

    fn paint(self, ui: &mut Ui, outer: Rect) -> (Rect, ToastResponse, egui::Id) {
        let theme = ui.tokens();
        let surface = ui.painter().add(egui::Shape::Noop);
        let content_top = outer.top() + f32::from(PADDING.top);
        let text_left = outer.left() + f32::from(PADDING.left) + size::ICON_LG + GAP;
        let mini_left = outer.right() - f32::from(PADDING.right) - MINI_BUTTON;
        let text_right = if self.dismissible {
            mini_left - GAP
        } else {
            outer.right() - f32::from(PADDING.right)
        };

        let mut text_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(text_left, content_top),
                    pos2(text_right, f32::INFINITY),
                ))
                .layout(Layout::top_down(Align::Min)),
        );
        text_ui.spacing_mut().item_spacing = Vec2::ZERO;
        let title = TITLE.label(&mut text_ui, self.title, theme.text.primary);
        if let Some(detail) = self.detail {
            text_ui.add_space(TITLE_TO_DETAIL);
            type_scale::META.label(&mut text_ui, detail, theme.text.secondary);
        }
        let mut response = ToastResponse::default();
        if let Some(action) = self.action {
            text_ui.add_space(ACTION_TOP);
            response.action = Button::link(action).show(&mut text_ui).clicked();
        }

        let mut content_bottom = text_ui.min_rect().bottom();
        if self.dismissible {
            let mini = Rect::from_min_size(pos2(mini_left, content_top), Vec2::splat(MINI_BUTTON));
            response.dismissed = MiniButton::dismiss()
                .show(ui, mini, ui.id().with("dismiss"))
                .clicked();
            content_bottom = content_bottom.max(mini.bottom());
        }
        let rect = Rect::from_min_max(
            outer.min,
            pos2(outer.right(), content_bottom + f32::from(PADDING.bottom)),
        );
        let icon_rect = Rect::from_center_size(
            pos2(
                outer.left() + f32::from(PADDING.left) + size::ICON_LG / 2.0,
                (content_top + content_bottom) / 2.0,
            ),
            Vec2::splat(size::ICON_LG),
        );
        toast_icon(self.tone).paint(ui, icon_rect, theme.tone(self.tone).fill);

        ui.painter().set(
            surface,
            egui::Shape::Vec(vec![
                theme
                    .shadow
                    .raised
                    .shape(ui.ctx(), ui.pixels_per_point(), rect, radius::TOAST),
                egui::epaint::RectShape::filled(rect, radius::TOAST, theme.surface.raised).into(),
            ]),
        );
        ui.allocate_rect(rect, Sense::hover());
        (rect, response, title.id)
    }
}

/// Show `add_toasts` as the window's toast stack: [`TOAST_WIDTH`] wide,
/// [`TOAST_INSET`] from the bottom-right corner, [`TOAST_GAP`] apart, newest
/// at the bottom.
pub fn show_toast_stack<R>(ctx: &Context, id: Id, add_toasts: impl FnOnce(&mut Ui) -> R) -> R {
    Area::new(id)
        .order(Order::Foreground)
        .anchor(Align2::RIGHT_BOTTOM, vec2(-TOAST_INSET, -TOAST_INSET))
        .show(ctx, |ui| {
            ui.set_width(TOAST_WIDTH);
            ui.spacing_mut().item_spacing = vec2(0.0, TOAST_GAP);
            add_toasts(ui)
        })
        .inner
}
