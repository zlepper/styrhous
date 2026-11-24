//! The header of an inspector blade (C19): navigation, the resource's kind
//! badge, title and status line, its primary actions with their keys, and
//! a row of readouts under them.

use egui::{Align, Color32, Layout, Rect, Response, Sense, Ui, UiBuilder, pos2, vec2};

use super::chrome::{BladeNavigation, close_button_at_end, close_width, navigation_buttons};
use super::interaction::HeaderAction;
use crate::design::theme::{ThemeExt, Tone};
use crate::design::type_scale::{self, TextRole, Weight};
use crate::design::{size, space};
use crate::interaction::PointingHand;
use crate::readout::{Readout, ReadoutRow};
use crate::status::StatusLabel;
use crate::tooltip::{TooltipExt, elided_label};

/// The resource's name (`.t-title` at 18 px): Bricolage 18/24.
const TITLE_ROLE: TextRole = type_scale::CARD_TITLE.with_size(18.0, 24.0);
/// The state word of the status line (`.w6.t-meta`).
const STATE_ROLE: TextRole = type_scale::META.with_weight(Weight::SemiBold);
/// Gap between the items of the header row (`.row` gap 10).
const ROW_GAP: f32 = 10.0;
/// Margin on each side of the rule after the navigation (`margin: 0 2px`).
const RULE_MARGIN: f32 = space::S2;
/// Gap between the header row and the readouts (`.col` gap 14).
const READOUTS_GAP: f32 = 14.0;
/// Inset of the readouts from the header's padding (`padding: 0 4px`).
const READOUTS_INSET: f32 = space::S4;

/// What an [`InspectorHeader`] did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InspectorHeaderResponse {
    pub back: bool,
    pub forward: bool,
    pub close: bool,
    /// The link of the status line was clicked.
    pub status_link: bool,
}

/// The status line under an inspector's title: a lamp, the state word in
/// its tone and a one-line reason ("● Degraded · warmup container
/// crash-looping").
#[derive(Clone, Copy, Debug)]
pub struct InspectorStatus<'a> {
    pub tone: Tone,
    pub state: &'a str,
    pub reason: Option<&'a str>,
    /// A reference ending the line in `accent-text` ("· on kind-control-plane").
    pub link: Option<&'a str>,
}

/// An inspector blade's header (C19, about 150 points): row one is Back ·
/// Forward | kind badge · title over its status line · the primary actions
/// (each showing its key, [`crate::Button::shortcut`]) · More · Close; row
/// two the inspector's [`ReadoutRow`]. It replaces the plain blade header
/// for resources with live state.
#[must_use = "call `show` to add the header"]
pub struct InspectorHeader<'a> {
    navigation: BladeNavigation<'a>,
    kind: Option<(&'a str, Option<Color32>)>,
    title: &'a str,
    status: Option<InspectorStatus<'a>>,
    readouts: &'a [Readout<'a>],
}

impl<'a> InspectorHeader<'a> {
    pub fn new(title: &'a str, navigation: BladeNavigation<'a>) -> Self {
        Self {
            navigation,
            kind: None,
            title,
            status: None,
            readouts: &[],
        }
    }

    /// Lead the title with the resource `kind` badge, on `fill` (the
    /// kind's category colour) or the accent fill.
    pub fn kind(mut self, kind: &'a str, fill: Option<Color32>) -> Self {
        self.kind = Some((kind, fill));
        self
    }

    pub fn status(mut self, status: InspectorStatus<'a>) -> Self {
        self.status = Some(status);
        self
    }

    /// The readouts of the second row (none: a one-row header).
    pub fn readouts(mut self, readouts: &'a [Readout<'a>]) -> Self {
        self.readouts = readouts;
        self
    }

    /// Add the header across the available width, `add_actions` laying
    /// the primary actions and More out left to right before Close.
    pub fn show(self, ui: &mut Ui, add_actions: impl FnOnce(&mut Ui)) -> InspectorHeaderResponse {
        let padding = size::INSPECTOR_HEADER_PADDING;
        let readouts_height = if self.readouts.is_empty() {
            0.0
        } else {
            READOUTS_GAP + ReadoutRow::new(self.readouts).height(ui)
        };
        let height = padding.sum().y + size::INSPECTOR_HEADER_ROW + readouts_height;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let content = rect - padding;
        let row = Rect::from_min_size(
            content.min,
            vec2(content.width(), size::INSPECTOR_HEADER_ROW),
        );
        let result = self.show_row(ui, row, &response, add_actions);
        if !self.readouts.is_empty() {
            let top = row.bottom() + READOUTS_GAP;
            let readouts = Rect::from_min_max(
                pos2(content.left() + READOUTS_INSET, top),
                pos2(content.right() - READOUTS_INSET, content.bottom()),
            );
            let mut readouts_ui = ui.new_child(UiBuilder::new().max_rect(readouts));
            ReadoutRow::new(self.readouts).show(&mut readouts_ui);
        }
        result
    }

    fn show_row(
        &self,
        ui: &mut Ui,
        row: Rect,
        response: &Response,
        add_actions: impl FnOnce(&mut Ui),
    ) -> InspectorHeaderResponse {
        let mut result = InspectorHeaderResponse::default();
        let mut row_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::left_to_right(Align::Center)),
        );
        row_ui.spacing_mut().item_spacing = vec2(ROW_GAP, 0.0);
        match navigation_buttons(&mut row_ui, self.navigation, RULE_MARGIN) {
            HeaderAction::Back => result.back = true,
            HeaderAction::Forward => result.forward = true,
            HeaderAction::Close | HeaderAction::None => {}
        }
        if let Some((kind, fill)) = self.kind {
            let fill = fill.unwrap_or(row_ui.tokens().accent.fill);
            crate::badge::kind_badge_on(&mut row_ui, kind, fill);
        }

        // The actions sit against Close at the end; their width is known
        // from the previous pass (egui lays a widget out before its size).
        let close_width = close_width();
        let actions_id = response.id.with("actions-width");
        let actions_width = ui
            .data(|data| data.get_temp::<f32>(actions_id))
            .unwrap_or(0.0);
        let actions_right = row.right() - close_width - ROW_GAP;
        let actions_left = actions_right - actions_width;
        let title_left = row_ui.cursor().left();
        let title_width = (actions_left - ROW_GAP - title_left).max(0.0);
        result.status_link = self.title_block(
            &mut row_ui,
            Rect::from_min_size(pos2(title_left, row.top()), vec2(title_width, row.height())),
        );

        let mut actions_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(actions_left, row.top()),
                    pos2(actions_right, row.bottom()),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        actions_ui.spacing_mut().item_spacing = vec2(ROW_GAP, 0.0);
        add_actions(&mut actions_ui);
        let measured = actions_ui.min_rect().width().max(0.0);
        if (measured - actions_width).abs() > 0.01 {
            ui.data_mut(|data| data.insert_temp(actions_id, measured));
            ui.ctx()
                .request_discard("inspector header actions changed width");
        }

        result.close = close_button_at_end(ui, row, self.navigation.close_label);
        result
    }

    /// The title over the status line in `rect`.
    fn title_block(&self, ui: &mut Ui, rect: Rect) -> bool {
        let theme = ui.tokens();
        let block_height =
            TITLE_ROLE.line_height() + self.status.map_or(0.0, |_| type_scale::META.line_height());
        let top = rect.center().y - block_height / 2.0;
        let title =
            TITLE_ROLE.galley_elided(ui.ctx(), self.title, theme.text.primary, rect.width(), 1);
        let title_rect = Rect::from_min_size(
            pos2(rect.left(), top),
            vec2(rect.width(), TITLE_ROLE.line_height()),
        );
        let elided = title.elided;
        let title_response = type_scale::allocate_text_at(ui, title_rect, self.title);
        type_scale::paint_text(
            ui,
            &title_response,
            title_rect.min,
            title,
            theme.text.primary,
        );
        if elided {
            let _ = title_response.tooltip(self.title);
        }
        let Some(status) = self.status else {
            return false;
        };
        // The status line: the lamp and state word, then "· reason",
        // elided to the room left.
        let line = Rect::from_min_max(
            pos2(rect.left(), title_rect.bottom()),
            pos2(
                rect.right(),
                title_rect.bottom() + type_scale::META.line_height(),
            ),
        );
        let mut line_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(line)
                .layout(Layout::left_to_right(Align::Min)),
        );
        line_ui.spacing_mut().item_spacing = vec2(size::STATUS_DOT_GAP, 0.0);
        StatusLabel::new(status.state, status.tone)
            .role(STATE_ROLE)
            .color(theme.tone(status.tone).text)
            .show(&mut line_ui);
        if let Some(reason) = status.reason {
            if status.link.is_some() {
                type_scale::META.label(&mut line_ui, &format!("· {reason}"), theme.text.muted);
            } else {
                elided_label(
                    &mut line_ui,
                    type_scale::META,
                    &format!("· {reason}"),
                    theme.text.muted,
                );
            }
        }
        let Some(link) = status.link else {
            return false;
        };
        let galley = type_scale::META.galley(
            line_ui.ctx(),
            link,
            theme.accent.text,
            line_ui.available_width(),
        );
        let (link_rect, response) =
            type_scale::allocate_text(&mut line_ui, galley.rect.size(), link);
        let response = response.interact(Sense::click()).with_pointing_hand();
        type_scale::paint_text(
            &line_ui,
            &response,
            link_rect.min,
            galley,
            theme.accent.text,
        );
        response.clicked()
    }
}
