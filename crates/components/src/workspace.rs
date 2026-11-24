//! Shared application-shell primitives for dense desktop workspaces (cards
//! are [`crate::Card`]).
//!
//! The components in this module own the common Lens-inspired surface treatment so
//! application screens do not repeat fills, borders or radii (empty states are
//! [`crate::EmptyState`]).

use egui::{Frame, Margin, Ui};

use crate::design::size;
use crate::design::theme::Theme;

/// The content canvas used beside the application navigation.
pub struct WorkspacePage;

impl WorkspacePage {
    /// Frame for the central panel beside the sidebar, filled with
    /// `surface-canvas`; its end keeps [`size::WORKSPACE_END_GUTTER`] for a
    /// list's scroll track (A02 `.main`).
    pub fn canvas_frame(theme: &Theme) -> Frame {
        Frame::NONE.fill(theme.surface.canvas).inner_margin(Margin {
            right: size::WORKSPACE_END_GUTTER as i8,
            ..Margin::ZERO
        })
    }

    pub fn show<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        add_contents(ui)
    }
}
