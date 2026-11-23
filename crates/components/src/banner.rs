//! Banners (`.banner`): one app-wide condition (licence states, an update,
//! a lost cluster connection) across the top of the window. Only one shows
//! at a time, by priority danger > warning > info; it pushes the workspace
//! down rather than covering it.

use egui::{Align, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::PointingHand;
use crate::a11y::{Name, describe_container};
use crate::callout::tone_icon;
use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale;
use crate::design::{size, space};
use crate::mini_button::MiniButton;
use crate::tooltip::TooltipExt;

/// Horizontal padding of a banner.
const PADDING_X: f32 = space::S16;
/// Gap between the icon, the message, the action and the dismiss button.
const GAP: f32 = space::ICON_TEXT;
/// Distance from the action's baseline to the top of its underline (CSS
/// `text-decoration: underline` in Figtree at 13 px).
const UNDERLINE_OFFSET: f32 = 2.5;
/// Thickness of the action's underline.
const UNDERLINE_THICKNESS: f32 = 0.65;
/// The action's words (semibold, in the message colour).
const ACTION_ROLE: type_scale::TextRole = type_scale::UI_STRONG;

/// What the user did with a [`Banner`] this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BannerResponse {
    /// The action link was clicked.
    pub action: bool,
    /// The dismiss button was clicked.
    pub dismissed: bool,
}

/// A banner [`size::BANNER`] tall spanning the available width: the tone's
/// icon, a message in `*-text-on-tint`, an optional underlined action and an
/// optional dismiss button at the far right, on the tone's tint above a
/// hairline in its outline colour.
#[must_use = "call `show` to add the banner"]
pub struct Banner<'a> {
    tone: Tone,
    message: &'a str,
    action: Option<&'a str>,
    dismissible: bool,
}

impl<'a> Banner<'a> {
    /// A banner in `tone` (danger, warning or info) saying `message`.
    pub fn new(tone: Tone, message: &'a str) -> Self {
        Self {
            tone,
            message,
            action: None,
            dismissible: false,
        }
    }

    /// An underlined action after the message ("Manage license", "Retry").
    pub fn action(mut self, label: &'a str) -> Self {
        self.action = Some(label);
        self
    }

    /// Show a dismiss button, which hides the banner for the session.
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    /// Add the banner at the cursor.
    pub fn show(self, ui: &mut Ui) -> BannerResponse {
        let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), size::BANNER));
        let mut banner_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(("banner", self.message))
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let (response, message) = self.paint(&mut banner_ui, rect);
        describe_container(
            &banner_ui,
            egui::accesskit::Role::Status,
            Name::LabelledBy(message),
            None,
        );
        ui.allocate_rect(rect, Sense::hover());
        response
    }

    fn paint(self, ui: &mut Ui, rect: Rect) -> (BannerResponse, egui::Id) {
        let theme = ui.tokens();
        let colors = theme.tone(self.tone);
        let painter = ui.painter();
        painter.rect_filled(rect, 0, colors.tint);
        // The border is inside the banner's height; its content centres above it.
        let border =
            Rect::from_min_max(pos2(rect.left(), rect.bottom() - size::HAIRLINE), rect.max);
        painter.rect_filled(border, 0, colors.outline);
        let row = Rect::from_min_max(
            pos2(rect.left() + PADDING_X, rect.top()),
            pos2(rect.right() - PADDING_X, border.top()),
        );
        let centre = row.center().y;

        let icon_rect = Rect::from_center_size(
            pos2(row.left() + size::ICON / 2.0, centre),
            Vec2::splat(size::ICON),
        );
        tone_icon(self.tone).paint(ui, icon_rect, colors.fill);

        let role = type_scale::UI;
        let baseline = role.centred_baseline(row.height(), ui.pixels_per_point());
        // The message gives way: it is cut short of the action and the dismiss
        // button, with its full text in a tooltip.
        let dismiss_width = if self.dismissible {
            crate::MINI_BUTTON + GAP
        } else {
            0.0
        };
        let action_width = self.action.map_or(0.0, |action| {
            ACTION_ROLE
                .galley(ui.ctx(), action, colors.text_on_tint, f32::INFINITY)
                .rect
                .width()
                + GAP
        });
        let text_rect = Rect::from_min_max(
            pos2(icon_rect.right() + GAP, row.top()),
            pos2(row.right() - dismiss_width, row.bottom()),
        );
        let mut text_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(text_rect)
                .layout(Layout::left_to_right(Align::Min)),
        );
        text_ui.spacing_mut().item_spacing = vec2(GAP, 0.0);
        let message_width = (text_rect.width() - action_width).max(0.0);
        let galley = role.galley_elided(
            ui.ctx(),
            self.message,
            colors.text_on_tint,
            message_width,
            1,
        );
        let elided = galley.elided;
        let top = row.top() + baseline - role.css_baseline(ui.pixels_per_point());
        let message_rect = Rect::from_min_size(pos2(text_rect.left(), top), galley.rect.size());
        let message = type_scale::allocate_text_at(&mut text_ui, message_rect, self.message);
        type_scale::paint_text(
            &text_ui,
            &message,
            message_rect.min,
            galley,
            colors.text_on_tint,
        );
        let message = if elided {
            message.tooltip(self.message)
        } else {
            message
        };
        let mut response = BannerResponse::default();
        if let Some(action) = self.action {
            response.action =
                underlined_action(&mut text_ui, action, colors.text_on_tint, baseline).clicked();
        }
        if self.dismissible {
            let mini = Rect::from_center_size(
                pos2(row.right() - crate::MINI_BUTTON / 2.0, centre),
                Vec2::splat(crate::MINI_BUTTON),
            );
            response.dismissed = MiniButton::dismiss()
                .color(colors.text_on_tint)
                .show(ui, mini, ui.id().with("dismiss"))
                .clicked();
        }
        (response, message.id)
    }
}

/// The banner's action: semibold message-coloured text, always underlined,
/// on the row's `baseline`.
fn underlined_action(ui: &mut Ui, label: &str, color: egui::Color32, baseline: f32) -> Response {
    let role = ACTION_ROLE;
    let galley = role.galley(ui.ctx(), label, color, f32::INFINITY);
    let css_baseline = role.css_baseline(ui.pixels_per_point());
    let row = ui.cursor();
    let rect = Rect::from_min_size(
        pos2(row.left(), row.top() + baseline - css_baseline),
        galley.rect.size(),
    );
    let response = ui.allocate_rect(rect, Sense::click()).with_pointing_hand();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, ui.is_enabled(), label));
    type_scale::expose_text_runs_as(
        ui,
        &response,
        rect.min,
        &galley,
        egui::accesskit::Role::Link,
    );
    type_scale::paint_galley(ui.painter(), rect.min, galley, color);
    ui.painter().rect_filled(
        Rect::from_min_size(
            pos2(rect.left(), rect.top() + css_baseline + UNDERLINE_OFFSET),
            vec2(rect.width(), UNDERLINE_THICKNESS),
        ),
        0,
        color,
    );
    response
}
