use super::*;
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
            section_label(ui, "Rounded primary buttons");
            button_row(ui, ButtonVariant::Primary, ButtonRounding::Pill);
            section_label(ui, "Rounded secondary buttons");
            button_row(ui, ButtonVariant::Secondary, ButtonRounding::Pill);
            section_label(ui, "Rounded soft buttons");
            button_row(ui, ButtonVariant::Soft, ButtonRounding::Pill);
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
