//! Tailwind-styled button component for egui

use egui::{Button, Color32, CornerRadius, Response, Shadow, Stroke, Ui, Vec2, WidgetText};

use crate::colors::{BLACK, WHITE, gray, indigo};

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
        let shadow = self.shadow();

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
        let button = Button::new(self.text).min_size(Vec2::new(0.0, min_height));
        let response = ui.add(button);

        // Draw shadow behind the button (painted on layer below)
        if shadow != Shadow::NONE {
            let shadow_shape = shadow.as_shape(response.rect, corner_radius);
            ui.painter().add(shadow_shape);
        }

        // Restore original visuals
        ui.visuals_mut().widgets = saved_widgets;
        ui.spacing_mut().button_padding = saved_button_padding;

        response
    }

    /// Get colors for the current variant
    fn variant_colors(&self) -> (Color32, Color32, Color32, Color32, Stroke) {
        match self.variant {
            ButtonVariant::Primary => (
                indigo::_600, // fill
                indigo::_700, // fill_hovered
                indigo::_800, // fill_active
                WHITE,        // text_color
                Stroke::NONE, // stroke
            ),
            ButtonVariant::Secondary => (
                WHITE,                        // fill
                gray::_50,                    // fill_hovered
                gray::_100,                   // fill_active
                gray::_700,                   // text_color
                Stroke::new(1.0, gray::_300), // stroke
            ),
            ButtonVariant::Soft => (
                indigo::_50,  // fill
                indigo::_100, // fill_hovered
                indigo::_200, // fill_active
                indigo::_600, // text_color
                Stroke::NONE, // stroke
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

    /// Get shadow for the current variant
    /// Tailwind shadow-xs: box-shadow: 0 1px 2px 0 rgb(0 0 0 / 0.05)
    fn shadow(&self) -> Shadow {
        match self.variant {
            ButtonVariant::Secondary => Shadow {
                offset: [0, 1], // 0 horizontal, 1px down
                blur: 2,
                spread: 0,
                color: BLACK.gamma_multiply(0.05),
            },
            _ => Shadow::NONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, SnapshotResults, kittest::Queryable};

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

    fn button_row(ui: &mut Ui, variant: ButtonVariant, rounding: ButtonRounding) {
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
        harness.snapshot("buttons/variants");
    }

    /// Test button hover and active (pressed) states
    /// Each snapshot shows a reference button (default) next to the interactive button
    #[test]
    fn test_button_interaction_states() {
        let mut results = SnapshotResults::new();

        // Test Primary button - hovered
        {
            let mut harness = Harness::new_ui(|ui| {
                ui.vertical(|ui| {
                    ui.label("Primary: Default vs Hovered");
                    ui.horizontal(|ui| {
                        TailwindButton::primary("Default").show(ui);
                        TailwindButton::primary("Hovered").show(ui);
                    });
                });
            });
            harness.get_by_label("Hovered").hover();
            harness.run_ok();
            results.add(harness.try_snapshot("buttons/primary_hovered"));
        }

        // Test Primary button - pressed
        {
            let mut harness = Harness::new_ui(|ui| {
                ui.vertical(|ui| {
                    ui.label("Primary: Default vs Pressed");
                    ui.horizontal(|ui| {
                        TailwindButton::primary("Default").show(ui);
                        TailwindButton::primary("Pressed").show(ui);
                    });
                });
            });
            let button = harness.get_by_label("Pressed");
            let center = button.rect().center();
            harness
                .input_mut()
                .events
                .push(egui::Event::PointerMoved(center));
            harness.input_mut().events.push(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            });
            harness.step();
            results.add(harness.try_snapshot("buttons/primary_pressed"));
        }

        // Test Secondary button - hovered
        {
            let mut harness = Harness::new_ui(|ui| {
                ui.vertical(|ui| {
                    ui.label("Secondary: Default vs Hovered");
                    ui.horizontal(|ui| {
                        TailwindButton::secondary("Default").show(ui);
                        TailwindButton::secondary("Hovered").show(ui);
                    });
                });
            });
            harness.get_by_label("Hovered").hover();
            harness.run_ok();
            results.add(harness.try_snapshot("buttons/secondary_hovered"));
        }

        // Test Secondary button - pressed
        {
            let mut harness = Harness::new_ui(|ui| {
                ui.vertical(|ui| {
                    ui.label("Secondary: Default vs Pressed");
                    ui.horizontal(|ui| {
                        TailwindButton::secondary("Default").show(ui);
                        TailwindButton::secondary("Pressed").show(ui);
                    });
                });
            });
            let button = harness.get_by_label("Pressed");
            let center = button.rect().center();
            harness
                .input_mut()
                .events
                .push(egui::Event::PointerMoved(center));
            harness.input_mut().events.push(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            });
            harness.step();
            results.add(harness.try_snapshot("buttons/secondary_pressed"));
        }

        // Test Soft button - hovered
        {
            let mut harness = Harness::new_ui(|ui| {
                ui.vertical(|ui| {
                    ui.label("Soft: Default vs Hovered");
                    ui.horizontal(|ui| {
                        TailwindButton::soft("Default").show(ui);
                        TailwindButton::soft("Hovered").show(ui);
                    });
                });
            });
            harness.get_by_label("Hovered").hover();
            harness.run_ok();
            results.add(harness.try_snapshot("buttons/soft_hovered"));
        }

        // Test Soft button - pressed
        {
            let mut harness = Harness::new_ui(|ui| {
                ui.vertical(|ui| {
                    ui.label("Soft: Default vs Pressed");
                    ui.horizontal(|ui| {
                        TailwindButton::soft("Default").show(ui);
                        TailwindButton::soft("Pressed").show(ui);
                    });
                });
            });
            let button = harness.get_by_label("Pressed");
            let center = button.rect().center();
            harness
                .input_mut()
                .events
                .push(egui::Event::PointerMoved(center));
            harness.input_mut().events.push(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            });
            harness.step();
            results.add(harness.try_snapshot("buttons/soft_pressed"));
        }
    }
}
