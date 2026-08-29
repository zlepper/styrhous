//! Tailwind-styled button component for egui

use egui::{
    Button, Color32, CornerRadius, Image, Response, Stroke, TextStyle, Ui, Vec2, WidgetText,
};

use crate::PointingHand;
use crate::colors::{WHITE, gray, indigo};
use crate::design::{button, radius, typography};

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
    /// Solid danger background with white text
    Danger,
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
    content: ButtonContent<'a>,
    variant: ButtonVariant,
    size: ButtonSize,
    rounding: ButtonRounding,
    accessibility_label: Option<String>,
}

enum ButtonContent<'a> {
    Text(WidgetText),
    Icon(Image<'a>),
}

impl<'a> TailwindButton<'a> {
    /// Create a new button with the given text
    ///
    /// Defaults to Primary variant, Md size, and Default rounding
    pub fn new(text: impl Into<WidgetText>) -> Self {
        Self {
            content: ButtonContent::Text(text.into()),
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            rounding: ButtonRounding::Default,
            accessibility_label: None,
        }
    }

    /// Create an icon-only button.
    ///
    /// Call [`Self::accessibility_label`] to provide a label for assistive
    /// technologies and UI tests.
    pub fn icon(icon: Image<'a>) -> Self {
        Self {
            content: ButtonContent::Icon(icon),
            variant: ButtonVariant::Primary,
            size: ButtonSize::Md,
            rounding: ButtonRounding::Default,
            accessibility_label: None,
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

    /// Create a destructive action button.
    pub fn danger(text: impl Into<WidgetText>) -> Self {
        Self::new(text).variant(ButtonVariant::Danger)
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

    /// Set an accessible label for an icon-only button.
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Show the button and return the response
    pub fn show(self, ui: &mut Ui) -> Response {
        let (fill, fill_hovered, fill_active, text_color, stroke) = self.variant_colors();
        let (text_padding, control_height) = self.size_metrics();
        let (padding, min_size) = if matches!(&self.content, ButtonContent::Icon(_)) {
            // Icon-only buttons share the text-button height ladder. They only
            // differ by using that height as a square hit target around their
            // centered 16×16px glyph.
            (
                Vec2::splat((control_height - 16.0) / 2.0),
                Vec2::splat(control_height),
            )
        } else {
            (text_padding, Vec2::new(0.0, control_height))
        };
        let corner_radius = self.corner_radius();

        // Save current widget visuals
        let saved_style = ui.style().clone();

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
        let button_font = typography::semibold_or_proportional(ui.ctx(), typography::BODY_SIZE);
        ui.style_mut()
            .text_styles
            .insert(TextStyle::Button, button_font);

        // Create and render the button
        let button = match self.content {
            ButtonContent::Text(text) => Button::new(text),
            ButtonContent::Icon(icon) => Button::image(icon),
        }
        .min_size(min_size);
        let response = ui.add(button).with_pointing_hand();

        // Restore original visuals
        ui.set_style(saved_style);

        if let Some(label) = self.accessibility_label {
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label.clone())
            });
        }

        response
    }

    /// Get colors for the current variant
    fn variant_colors(&self) -> (Color32, Color32, Color32, Color32, Stroke) {
        match self.variant {
            ButtonVariant::Primary => (
                button::PRIMARY, // fill
                indigo::_700,    // fill_hovered
                indigo::_800,    // fill_active
                WHITE,           // text_color
                Stroke::NONE,    // stroke
            ),
            ButtonVariant::Secondary => (
                button::SECONDARY,            // fill
                gray::_50,                    // fill_hovered
                gray::_100,                   // fill_active
                gray::_700,                   // text_color
                Stroke::new(1.0, gray::_300), // stroke
            ),
            ButtonVariant::Soft => (
                button::SOFT, // fill
                indigo::_100, // fill_hovered
                indigo::_200, // fill_active
                indigo::_600, // text_color
                Stroke::NONE, // stroke
            ),
            ButtonVariant::Danger => (
                button::DANGER,
                button::DANGER.gamma_multiply(0.9),
                button::DANGER.gamma_multiply(0.8),
                WHITE,
                Stroke::NONE,
            ),
        }
    }

    /// Get size metrics for the current size
    fn size_metrics(&self) -> (Vec2, f32) {
        match self.size {
            ButtonSize::Xs => (Vec2::new(8.0, 7.0), 32.0),
            ButtonSize::Sm => (Vec2::new(17.5, 8.0), 34.0),
            ButtonSize::Md => (Vec2::new(26.0, 10.0), 38.0),
            ButtonSize::Lg => (Vec2::new(30.5, 11.0), 40.0),
            ButtonSize::Xl => (Vec2::new(43.0, 13.0), 44.0),
        }
    }

    /// Get corner radius for the current rounding style
    fn corner_radius(&self) -> CornerRadius {
        match self.rounding {
            ButtonRounding::Default => radius::control(),
            ButtonRounding::Rounded => radius::surface(),
            ButtonRounding::Pill => CornerRadius::same(255), // max u8 for pill shape
        }
    }
}

#[cfg(test)]
mod tests;
