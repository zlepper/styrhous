//! Shared application-shell primitives for dense desktop workspaces (cards
//! are [`crate::Card`]).
//!
//! The components in this module own the common Lens-inspired surface treatment so
//! application screens do not repeat fills, borders or radii (empty states are
//! [`crate::EmptyState`]).

use egui::{Color32, Context, Frame, Margin, RichText, Ui};

use crate::design::theme::{Theme, ThemeExt};
use crate::design::{space, stroke, type_scale};

const DRAWER_MARGIN: i8 = space::S12 as i8;

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

/// The code surface used for focused editing drawers.
pub struct WorkspaceDrawer;

impl WorkspaceDrawer {
    pub fn frame(ctx: &Context) -> Frame {
        let tokens = ctx.tokens();
        Frame::new()
            .fill(tokens.surface.sunken)
            .stroke(stroke::divider(&tokens))
            .inner_margin(Margin::same(DRAWER_MARGIN))
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
