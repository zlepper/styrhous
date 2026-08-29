use super::*;
use crate::icons::{arrow_left_icon, trash_icon};
use crate::test_support::UiHarnessSnapshot;
use egui_kittest::{Harness, kittest::Queryable};

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
            section_label(ui, "Primary buttons");
            button_row(ui, ButtonVariant::Primary, ButtonRounding::Default);
            section_label(ui, "Secondary buttons");
            button_row(ui, ButtonVariant::Secondary, ButtonRounding::Default);
            section_label(ui, "Soft buttons");
            button_row(ui, ButtonVariant::Soft, ButtonRounding::Default);
            section_label(ui, "Danger buttons");
            button_row(ui, ButtonVariant::Danger, ButtonRounding::Default);
            section_label(ui, "Rounded primary buttons");
            button_row(ui, ButtonVariant::Primary, ButtonRounding::Rounded);
            section_label(ui, "Pill primary buttons");
            button_row(ui, ButtonVariant::Primary, ButtonRounding::Pill);
            section_label(ui, "Pill secondary buttons");
            button_row(ui, ButtonVariant::Secondary, ButtonRounding::Pill);
            section_label(ui, "Pill soft buttons");
            button_row(ui, ButtonVariant::Soft, ButtonRounding::Pill);
            section_label(ui, "Icon buttons");
            ui.horizontal(|ui| {
                for (size, label) in SIZES {
                    TailwindButton::icon(
                        if matches!(size, ButtonSize::Xl) {
                            trash_icon()
                        } else {
                            arrow_left_icon()
                        }
                        .fit_to_exact_size(Vec2::splat(16.0)),
                    )
                    .variant(ButtonVariant::Secondary)
                    .size(size)
                    .accessibility_label(format!("{label} icon button"))
                    .show(ui);
                }
            });
        });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("buttons/test_buttons/variants");
}

#[test]
fn test_button_interaction_states() {
    for (variant, label) in [
        (ButtonVariant::Primary, "Primary"),
        (ButtonVariant::Secondary, "Secondary"),
        (ButtonVariant::Soft, "Soft"),
        (ButtonVariant::Danger, "Danger"),
    ] {
        let mut harness = Harness::new_ui(move |ui| {
            ui.vertical(|ui| {
                ui.label(format!("{label}: Default vs Hovered"));
                ui.horizontal(|ui| {
                    TailwindButton::new("Default").variant(variant).show(ui);
                    TailwindButton::new("Hovered").variant(variant).show(ui);
                });
            });
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.get_by_label("Hovered").hover();
        harness.run_ok();
        harness.ui_harness(format!(
            "buttons/test_button_interaction_states/{}_hovered",
            label.to_ascii_lowercase()
        ));
    }

    for (variant, label) in [
        (ButtonVariant::Primary, "Primary"),
        (ButtonVariant::Secondary, "Secondary"),
        (ButtonVariant::Soft, "Soft"),
        (ButtonVariant::Danger, "Danger"),
    ] {
        let mut harness = Harness::new_ui(move |ui| {
            ui.vertical(|ui| {
                ui.label(format!("{label}: Default vs Pressed"));
                ui.horizontal(|ui| {
                    TailwindButton::new("Default").variant(variant).show(ui);
                    TailwindButton::new("Pressed").variant(variant).show(ui);
                });
            });
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
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
        harness.ui_harness(format!(
            "buttons/test_button_interaction_states/{}_pressed",
            label.to_ascii_lowercase()
        ));
    }
}

#[test]
fn test_secondary_icon_interaction_states() {
    let icon_button = |label: &str| {
        TailwindButton::icon(arrow_left_icon().fit_to_exact_size(Vec2::splat(16.0)))
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Md)
            .accessibility_label(label)
    };

    let mut hover_harness = Harness::new_ui(|ui| {
        ui.horizontal(|ui| {
            icon_button("Default icon").show(ui);
            icon_button("Hovered icon").show(ui);
        });
    });
    crate::test_support::setup_egui(&mut hover_harness);
    hover_harness.run();
    hover_harness.get_by_label("Hovered icon").hover();
    hover_harness.run_ok();
    hover_harness.ui_harness("buttons/test_secondary_icon_interaction_states/hovered");

    let mut pressed_harness = Harness::new_ui(|ui| {
        ui.horizontal(|ui| {
            icon_button("Default icon").show(ui);
            icon_button("Pressed icon").show(ui);
        });
    });
    crate::test_support::setup_egui(&mut pressed_harness);
    pressed_harness.run();
    let button = pressed_harness.get_by_label("Pressed icon");
    let center = button.rect().center();
    pressed_harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(center));
    pressed_harness
        .input_mut()
        .events
        .push(egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    pressed_harness.step();
    pressed_harness.ui_harness("buttons/test_secondary_icon_interaction_states/pressed");
}
