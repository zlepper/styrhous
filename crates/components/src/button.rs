//! Tailwind-styled button component for egui

use egui::{
    Button, Color32, CornerRadius, Image, Response, Stroke, TextStyle, Ui, Vec2, WidgetText,
};

use crate::PointingHand;
use crate::design::theme::{Theme, ThemeExt};
use crate::design::{radius, type_scale};

/// Theme colours of one button variant.
struct ButtonColors {
    fill: Color32,
    fill_hovered: Color32,
    fill_active: Color32,
    text: Color32,
    /// Width of the outline, which is painted in the current fill.
    stroke_width: f32,
}

/// Button color variant
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Solid accent fill with on-fill text
    #[default]
    Primary,
    /// Neutral secondary fill with primary text
    Secondary,
    /// Accent tint with accent text
    Soft,
    /// Solid danger fill with on-fill text
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

impl ButtonSize {
    /// The fixed height of a button of this size, for rows that must line up with one.
    pub const fn height(self) -> f32 {
        match self {
            Self::Xs => 32.0,
            Self::Sm => 34.0,
            Self::Md => 38.0,
            Self::Lg => 40.0,
            Self::Xl => 44.0,
        }
    }
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
    ImageAndText(Image<'a>, WidgetText),
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

    /// Create a button with a leading icon and a text label.
    pub fn image_and_text(image: Image<'a>, text: impl Into<WidgetText>) -> Self {
        let mut button = Self::new("");
        button.content = ButtonContent::ImageAndText(image, text.into());
        button
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

    /// Create a primary button (solid accent fill)
    pub fn primary(text: impl Into<WidgetText>) -> Self {
        Self::new(text).variant(ButtonVariant::Primary)
    }

    /// Create a secondary button (neutral fill)
    pub fn secondary(text: impl Into<WidgetText>) -> Self {
        Self::new(text).variant(ButtonVariant::Secondary)
    }

    /// Create a soft button (accent tint)
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
        let colors = self.variant_colors(&ui.tokens());
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

        // The theme sets `override_text_color`, which egui prefers over a
        // widget's foreground stroke; point it at the variant's text colour.
        ui.visuals_mut().override_text_color = Some(colors.text);

        // Apply custom visuals for all button states
        let widgets = &mut ui.visuals_mut().widgets;
        for (visuals, fill) in [
            (&mut widgets.inactive, colors.fill),
            (&mut widgets.hovered, colors.fill_hovered),
            (&mut widgets.active, colors.fill_active),
        ] {
            visuals.weak_bg_fill = fill;
            visuals.bg_fill = fill;
            // Harbour separates controls by tone: the stroke keeps the
            // control's footprint but always matches its fill.
            visuals.bg_stroke = Stroke::new(colors.stroke_width, fill);
            visuals.fg_stroke = Stroke::new(1.0, colors.text);
            visuals.corner_radius = corner_radius;
        }

        // Apply custom padding
        ui.spacing_mut().button_padding = padding;
        let button_font = type_scale::BODY_STRONG.font_id(ui.ctx());
        ui.style_mut()
            .text_styles
            .insert(TextStyle::Button, button_font);

        // Create and render the button
        let button = match self.content {
            ButtonContent::Text(text) => Button::new(text),
            ButtonContent::Icon(icon) => Button::image(icon),
            ButtonContent::ImageAndText(icon, text) => {
                Button::image_and_text(icon.fit_to_exact_size(egui::Vec2::splat(16.0)), text)
            }
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
    fn variant_colors(&self, theme: &Theme) -> ButtonColors {
        match self.variant {
            ButtonVariant::Primary => ButtonColors {
                fill: theme.accent.fill,
                fill_hovered: theme.accent.fill_hover,
                fill_active: theme.accent.fill_active,
                text: theme.text.on_fill,
                stroke_width: 0.0,
            },
            ButtonVariant::Secondary => ButtonColors {
                fill: theme.components.btn_secondary_bg,
                fill_hovered: theme.components.btn_secondary_hover,
                fill_active: theme.components.btn_secondary_press,
                text: theme.text.primary,
                stroke_width: 1.0,
            },
            ButtonVariant::Soft => ButtonColors {
                fill: theme.accent.tint,
                fill_hovered: theme.accent.tint_hover,
                fill_active: theme.accent.tint_active,
                text: theme.accent.text,
                stroke_width: 0.0,
            },
            ButtonVariant::Danger => ButtonColors {
                fill: theme.danger.fill,
                fill_hovered: theme.danger.fill_hover,
                fill_active: theme.danger.fill_active,
                text: theme.text.on_fill,
                stroke_width: 0.0,
            },
        }
    }

    /// Get size metrics for the current size
    fn size_metrics(&self) -> (Vec2, f32) {
        let padding = match self.size {
            ButtonSize::Xs => Vec2::new(8.0, 7.0),
            ButtonSize::Sm => Vec2::new(17.5, 8.0),
            ButtonSize::Md => Vec2::new(26.0, 10.0),
            ButtonSize::Lg => Vec2::new(30.5, 11.0),
            ButtonSize::Xl => Vec2::new(43.0, 13.0),
        };
        (padding, self.size.height())
    }

    /// Get corner radius for the current rounding style
    fn corner_radius(&self) -> CornerRadius {
        match self.rounding {
            ButtonRounding::Default => egui::CornerRadius::same(radius::KEY),
            ButtonRounding::Rounded => egui::CornerRadius::same(radius::MENU_ITEM),
            ButtonRounding::Pill => CornerRadius::same(255), // max u8 for pill shape
        }
    }
}

#[cfg(test)]
mod tests;
