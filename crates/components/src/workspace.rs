//! Shared application-shell primitives for dense desktop workspaces.
//!
//! The components in this module own the common Lens-inspired surface treatment so
//! application screens do not repeat fills, borders, radii, or empty-state layout.

use egui::{Color32, Context, Frame, Margin, RichText, Stroke, Ui};

use crate::design::theme::{self, Theme, ThemeExt, ThemeScope};
use crate::design::{radius, space, stroke, type_scale};

const CARD_MARGIN: i8 = space::S12 as i8;
const STATE_MARGIN: i8 = space::S32 as i8;

/// The content canvas used beside the application navigation.
pub struct WorkspacePage;

impl WorkspacePage {
    /// Frame for the central panel, filled with `surface-canvas`.
    pub fn canvas_frame(theme: &Theme) -> Frame {
        Frame::NONE
            .fill(theme.surface.canvas)
            .inner_margin(Margin::ZERO)
    }

    pub fn show<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        add_contents(ui)
    }
}

/// Show `add_contents` in a default card with the given inner margin.
pub(crate) fn show_card<R>(
    ui: &mut Ui,
    padding: i8,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    WorkspaceCard::new()
        .margin(padding)
        .show_inner(ui, add_contents)
}

/// A card (`.card`) for filters, tables, forms and other workspace content.
///
/// Cards separate content by tone, not borders: the card fill
/// ([`Theme::card_fill`]) on the panel, the card radius and no visible
/// outline. Its contents are shown in [`ThemeScope::Card`], which re-points
/// rows, fields and wells for the card's background.
pub struct WorkspaceCard {
    margin: Margin,
    radius: u8,
    stroke: Option<Stroke>,
    fill: Option<Color32>,
}

impl Default for WorkspaceCard {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceCard {
    pub fn new() -> Self {
        Self {
            margin: Margin::same(CARD_MARGIN),
            radius: radius::CARD,
            stroke: None,
            fill: None,
        }
    }

    /// The frame of a default card in `theme`'s context, without a scope.
    ///
    /// Its hairline stroke is the card fill itself, so it is invisible (the
    /// design's cards have `border: 0`) yet keeps the frame's geometry when a
    /// caller sets a visible stroke for a selected or highlighted card.
    pub fn frame(theme: &Theme) -> Frame {
        let fill = theme.card_fill();
        Frame::new()
            .fill(fill)
            .stroke(stroke::hairline(fill))
            .corner_radius(radius::CARD)
            .inner_margin(Margin::same(CARD_MARGIN))
    }

    /// The inner margin between the card edge and its contents.
    pub fn margin(mut self, margin: impl Into<Margin>) -> Self {
        self.margin = margin.into();
        self
    }

    /// The corner radius, for cards nested in tighter layouts.
    pub fn radius(mut self, radius: u8) -> Self {
        self.radius = radius;
        self
    }

    /// A visible outline, for selected or highlighted cards. Keep it a
    /// hairline so the card keeps its size.
    pub fn stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Another fill, for selected cards.
    pub fn fill(mut self, fill: Color32) -> Self {
        self.fill = Some(fill);
        self
    }

    fn build(&self, theme: &Theme) -> Frame {
        let mut frame = Self::frame(theme)
            .inner_margin(self.margin)
            .corner_radius(self.radius);
        if let Some(fill) = self.fill {
            frame = frame.fill(fill).stroke(stroke::hairline(fill));
        }
        if let Some(stroke) = self.stroke {
            frame = frame.stroke(stroke);
        }
        frame
    }

    pub fn show<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        self.show_inner(ui, add_contents).inner
    }

    /// Like [`Self::show`], also returning the card's response.
    pub fn show_inner<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> egui::InnerResponse<R> {
        self.build(&ui.tokens())
            .show(ui, |ui| theme::scope(ui, ThemeScope::Card, add_contents))
    }
}

/// A centered state for an empty or not-yet-configured workspace.
pub struct WorkspaceEmptyState<'a> {
    title: &'a str,
    message: &'a str,
}

impl<'a> WorkspaceEmptyState<'a> {
    pub fn new(title: &'a str, message: &'a str) -> Self {
        Self { title, message }
    }

    pub fn show(self, ui: &mut Ui) {
        let tokens = ui.tokens();
        Frame::new()
            .inner_margin(Margin::same(STATE_MARGIN))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 44.0),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.label(
                                RichText::new(self.title)
                                    .font(type_scale::TITLE.font_id(ui.ctx()))
                                    .color(tokens.text.primary),
                            );
                            ui.add_space(space::S8);
                            ui.label(
                                RichText::new(self.message)
                                    .font(type_scale::BODY.font_id(ui.ctx()))
                                    .color(tokens.text.muted),
                            );
                        },
                    );
                });
            });
    }
}

/// The code surface used for focused editing drawers.
pub struct WorkspaceDrawer;

impl WorkspaceDrawer {
    pub fn frame(ctx: &Context) -> Frame {
        let tokens = ctx.tokens();
        Frame::new()
            .fill(tokens.surface.sunken)
            .stroke(stroke::divider(&tokens))
            .inner_margin(Margin::same(CARD_MARGIN))
    }

    pub fn text_color(ctx: &Context) -> Color32 {
        ctx.tokens().code.text
    }

    pub fn editor_background(ctx: &Context) -> Color32 {
        ctx.tokens().code.surface
    }
}

/// Render a compact title and optional detail for a workspace section.
pub fn workspace_section_header(ui: &mut Ui, title: &str, detail: Option<&str>) {
    let tokens = ui.tokens();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title)
                .font(type_scale::TITLE.font_id(ui.ctx()))
                .color(tokens.text.primary),
        );
        if let Some(detail) = detail {
            ui.label(
                RichText::new(detail)
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(tokens.text.muted),
            );
        }
    });
}
