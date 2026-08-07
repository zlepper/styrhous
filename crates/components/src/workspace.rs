//! Shared application-shell primitives for dense desktop workspaces.
//!
//! The components in this module own the common Lens-inspired surface treatment so
//! application screens do not repeat fills, borders, radii, or empty-state layout.

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Stroke, Ui};

use crate::colors::{CONTENT_BACKGROUND, WHITE, gray};

const CARD_MARGIN: i8 = 12;
const STATE_MARGIN: i8 = 32;
const SURFACE_RADIUS: u8 = 8;

/// The light content canvas used beside the dark application navigation.
pub struct WorkspacePage;

impl WorkspacePage {
    /// Frame for the central panel, ensuring the workspace background fills all available space.
    pub fn frame() -> Frame {
        Frame::NONE
            .fill(CONTENT_BACKGROUND)
            .inner_margin(Margin::ZERO)
    }

    pub fn show<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        add_contents(ui)
    }
}

/// A bordered white surface for filters, tables, and other workspace content.
pub struct WorkspaceCard {
    padding: i8,
}

impl Default for WorkspaceCard {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceCard {
    pub fn new() -> Self {
        Self {
            padding: CARD_MARGIN,
        }
    }

    pub fn padding(mut self, padding: i8) -> Self {
        self.padding = padding;
        self
    }

    pub fn show<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        Frame::new()
            .fill(WHITE)
            .stroke(Stroke::new(1.0, gray::_200))
            .corner_radius(CornerRadius::same(SURFACE_RADIUS))
            .inner_margin(Margin::same(self.padding))
            .show(ui, add_contents)
            .inner
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
                                    .size(18.0)
                                    .strong()
                                    .color(gray::_800),
                            );
                            ui.add_space(6.0);
                            ui.label(RichText::new(self.message).size(13.0).color(gray::_500));
                        },
                    );
                });
            });
    }
}

/// The dark surface used for focused editing drawers.
pub struct WorkspaceDrawer;

impl WorkspaceDrawer {
    pub fn frame() -> Frame {
        Frame::new()
            .fill(gray::_900)
            .stroke(Stroke::new(1.0, gray::_700))
            .inner_margin(Margin::same(CARD_MARGIN))
    }

    pub fn text_color() -> Color32 {
        gray::_100
    }

    pub fn editor_background() -> Color32 {
        gray::_950
    }
}

/// Render a compact title and optional detail for a workspace section.
pub fn workspace_section_header(ui: &mut Ui, title: &str, detail: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(16.0).strong().color(gray::_900));
        if let Some(detail) = detail {
            ui.label(RichText::new(detail).size(12.0).color(gray::_500));
        }
    });
}
