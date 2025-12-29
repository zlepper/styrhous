//! Tailwind-styled button component for egui

use egui::{Button, Color32, CornerRadius, Response, Stroke, Ui, Vec2, WidgetText};

use crate::colors::{gray, indigo, WHITE};

/// Button color variant
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Solid indigo background with white text
    #[default]
    Primary,
    /// White background with gray border and dark text
    Secondary,
    /// Light indigo background with indigo text
    Soft,
}

/// Button size preset
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonSize {
    /// Extra small: compact padding
    Xs,
    /// Small
    Sm,
    /// Medium (default)
    #[default]
    Md,
    /// Large
    Lg,
    /// Extra large
    Xl,
}

/// Button corner rounding style
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonRounding {
    /// Default rounding (6px)
    #[default]
    Default,
    /// More rounded (8px)
    Rounded,
    /// Fully rounded pill shape
    Pill,
}

/// A Tailwind-styled button builder for egui
///
/// # Example
/// ```ignore
/// TailwindButton::new("Click me").show(ui);
///
/// TailwindButton::primary("Save")
///     .size(ButtonSize::Lg)
///     .rounded(ButtonRounding::Pill)
///     .show(ui);
/// ```
pub struct TailwindButton<'a> {
    text: WidgetText,
    variant: ButtonVariant,
    size: ButtonSize,
    rounding: ButtonRounding,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> TailwindButton<'a> {
    /// Create a new button with the given text
    ///
    /// Defaults to Primary variant, Md size, and Default rounding
    pub fn new(text: impl Into<WidgetText>) -> Self {
        Self {
            text: text.into(),
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            rounding: ButtonRounding::Default,
            _marker: std::marker::PhantomData,
        }
    }

    /// Create a primary button (solid indigo background)
    pub fn primary(text: impl Into<WidgetText>) -> Self {
        Self::new(text).variant(ButtonVariant::Primary)
    }

    /// Create a secondary button (white background with border)
    pub fn secondary(text: impl Into<WidgetText>) -> Self {
        Self::new(text).variant(ButtonVariant::Secondary)
    }

    /// Create a soft button (light indigo background)
    pub fn soft(text: impl Into<WidgetText>) -> Self {
        Self::new(text).variant(ButtonVariant::Soft)
    }

    /// Set the button variant (color scheme)
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set the button size
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Set the button rounding style
    pub fn rounded(mut self, rounding: ButtonRounding) -> Self {
        self.rounding = rounding;
        self
    }

    /// Show the button and return the response
    pub fn show(self, ui: &mut Ui) -> Response {
        let (fill, fill_hovered, fill_active, text_color, stroke) = self.variant_colors();
        let (padding, min_height) = self.size_metrics();
        let corner_radius = self.corner_radius();

        // Save current widget visuals
        let saved_widgets = ui.visuals().widgets.clone();
        let saved_button_padding = ui.spacing().button_padding;

        // Apply custom visuals for all button states
        let visuals = &mut ui.visuals_mut().widgets;

        // Inactive state (not hovered)
        visuals.inactive.weak_bg_fill = fill;
        visuals.inactive.bg_fill = fill;
        visuals.inactive.bg_stroke = stroke;
        visuals.inactive.fg_stroke = Stroke::new(1.0, text_color);
        visuals.inactive.corner_radius = corner_radius;

        // Hovered state
        visuals.hovered.weak_bg_fill = fill_hovered;
        visuals.hovered.bg_fill = fill_hovered;
        visuals.hovered.bg_stroke = stroke;
        visuals.hovered.fg_stroke = Stroke::new(1.0, text_color);
        visuals.hovered.corner_radius = corner_radius;

        // Active/pressed state
        visuals.active.weak_bg_fill = fill_active;
        visuals.active.bg_fill = fill_active;
        visuals.active.bg_stroke = stroke;
        visuals.active.fg_stroke = Stroke::new(1.0, text_color);
        visuals.active.corner_radius = corner_radius;

        // Apply custom padding
        ui.spacing_mut().button_padding = padding;

        // Create and render the button
        let response = ui.add(Button::new(self.text).min_size(Vec2::new(0.0, min_height)));

        // Restore original visuals
        ui.visuals_mut().widgets = saved_widgets;
        ui.spacing_mut().button_padding = saved_button_padding;

        response
    }

    /// Get colors for the current variant
    fn variant_colors(&self) -> (Color32, Color32, Color32, Color32, Stroke) {
        match self.variant {
            ButtonVariant::Primary => (
                indigo::_600,         // fill
                indigo::_700,         // fill_hovered
                indigo::_800,         // fill_active
                WHITE,                // text_color
                Stroke::NONE,         // stroke
            ),
            ButtonVariant::Secondary => (
                WHITE,                // fill
                gray::_50,            // fill_hovered
                gray::_100,           // fill_active
                gray::_700,           // text_color
                Stroke::new(1.0, gray::_300), // stroke
            ),
            ButtonVariant::Soft => (
                indigo::_50,          // fill
                indigo::_100,         // fill_hovered
                indigo::_200,         // fill_active
                indigo::_600,         // text_color
                Stroke::NONE,         // stroke
            ),
        }
    }

    /// Get size metrics for the current size
    fn size_metrics(&self) -> (Vec2, f32) {
        match self.size {
            ButtonSize::Xs => (Vec2::new(8.0, 4.0), 24.0),
            ButtonSize::Sm => (Vec2::new(12.0, 6.0), 28.0),
            ButtonSize::Md => (Vec2::new(16.0, 8.0), 36.0),
            ButtonSize::Lg => (Vec2::new(20.0, 10.0), 44.0),
            ButtonSize::Xl => (Vec2::new(24.0, 12.0), 52.0),
        }
    }

    /// Get corner radius for the current rounding style
    fn corner_radius(&self) -> CornerRadius {
        match self.rounding {
            ButtonRounding::Default => CornerRadius::same(6),
            ButtonRounding::Rounded => CornerRadius::same(8),
            ButtonRounding::Pill => CornerRadius::same(255), // max u8 for pill shape
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;

    const SIZES: [(ButtonSize, &str); 5] = [
        (ButtonSize::Xs, "Xs"),
        (ButtonSize::Sm, "Sm"),
        (ButtonSize::Md, "Md"),
        (ButtonSize::Lg, "Lg"),
        (ButtonSize::Xl, "Xl"),
    ];

    fn section_label(ui: &mut Ui, text: &str) {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(text).strong());
        ui.add_space(4.0);
    }

    fn button_row(
        ui: &mut Ui,
        variant: ButtonVariant,
        rounding: ButtonRounding,
    ) {
        ui.horizontal(|ui| {
            for (size, _) in SIZES {
                TailwindButton::new("Button text")
                    .variant(variant)
                    .size(size)
                    .rounded(rounding)
                    .show(ui);
            }
        });
    }

    #[test]
    fn test_buttons() {
        let mut harness = Harness::new_ui(|ui| {
            ui.vertical(|ui| {
                // Primary buttons
                section_label(ui, "Primary buttons");
                button_row(ui, ButtonVariant::Primary, ButtonRounding::Default);

                // Secondary buttons
                section_label(ui, "Secondary buttons");
                button_row(ui, ButtonVariant::Secondary, ButtonRounding::Default);

                // Soft buttons
                section_label(ui, "Soft buttons");
                button_row(ui, ButtonVariant::Soft, ButtonRounding::Default);

                // Rounded primary buttons (pill)
                section_label(ui, "Rounded primary buttons");
                button_row(ui, ButtonVariant::Primary, ButtonRounding::Pill);

                // Rounded secondary buttons (pill)
                section_label(ui, "Rounded secondary buttons");
                button_row(ui, ButtonVariant::Secondary, ButtonRounding::Pill);

                // Rounded soft buttons (pill)
                section_label(ui, "Rounded soft buttons");
                button_row(ui, ButtonVariant::Soft, ButtonRounding::Pill);
            });
        });
        harness.snapshot("buttons");
    }
}
