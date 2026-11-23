use super::*;
use crate::test_support::{UiHarnessSnapshot, primary_click, setup_egui};
use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Server {
    Cloud,
    SelfHosted,
}

/// Every control with its label, as the C06 "With labels" column shows them.
#[derive(Clone)]
struct Settings {
    system_namespaces: bool,
    terminated_pods: bool,
    server: Server,
    updates: bool,
}

fn labelled_controls(settings: Rc<RefCell<Settings>>) -> Harness<'static> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(420.0, 220.0))
        .build_ui(move |ui| {
            let mut settings = settings.borrow_mut();
            ui.spacing_mut().item_spacing.y = space::S12;
            Checkbox::new(&mut settings.system_namespaces, "Show system namespaces").show(ui);
            Checkbox::new(&mut settings.terminated_pods, "Include terminated pods")
                .description("Completed and failed pods stay visible for 1 hour.")
                .show(ui);
            Radio::new(&mut settings.server, Server::Cloud, "Styrhous cloud").show(ui);
            Radio::new(
                &mut settings.server,
                Server::SelfHosted,
                "Self-hosted server",
            )
            .show(ui);
            Switch::new(&mut settings.updates, "Check for updates automatically").show(ui);
        });
    setup_egui(&mut harness);
    harness.run();
    harness
}

#[test]
fn labels_are_part_of_the_hit_target() {
    let settings = Rc::new(RefCell::new(Settings {
        system_namespaces: true,
        terminated_pods: false,
        server: Server::SelfHosted,
        updates: true,
    }));
    let mut harness = labelled_controls(settings.clone());
    harness.ui_harness("selection/labelled_controls");

    // Click the words, not the controls.
    for label in [
        "Show system namespaces",
        "Include terminated pods",
        "Styrhous cloud",
        "Check for updates automatically",
    ] {
        let node = harness.get_by_label(label);
        let rect = node.rect();
        primary_click(
            &mut harness,
            egui::pos2(rect.right() - 4.0, rect.center().y),
        );
        harness.run();
    }
    let settings = settings.borrow().clone();
    assert!(!settings.system_namespaces);
    assert!(settings.terminated_pods);
    assert_eq!(settings.server, Server::Cloud);
    assert!(!settings.updates);
}

#[test]
fn controls_expose_their_role_and_checked_state() {
    let settings = Rc::new(RefCell::new(Settings {
        system_namespaces: true,
        terminated_pods: false,
        server: Server::SelfHosted,
        updates: false,
    }));
    let harness = labelled_controls(settings);
    let toggled = |label: &str| harness.get_by_label(label).accesskit_node().toggled();
    assert_eq!(
        harness
            .get_by_label("Show system namespaces")
            .accesskit_node()
            .role(),
        Role::CheckBox
    );
    assert_eq!(toggled("Show system namespaces"), Some(Toggled::True));
    assert_eq!(toggled("Include terminated pods"), Some(Toggled::False));
    assert_eq!(
        harness
            .get_by_label("Self-hosted server")
            .accesskit_node()
            .role(),
        Role::RadioButton
    );
    assert_eq!(toggled("Self-hosted server"), Some(Toggled::True));
    assert_eq!(toggled("Styrhous cloud"), Some(Toggled::False));
    assert_eq!(
        toggled("Check for updates automatically"),
        Some(Toggled::False)
    );
}

#[test]
fn a_mixed_checkbox_reports_mixed_and_leaves_the_update_to_its_caller() {
    let clicks = Rc::new(Cell::new(0));
    let counter = clicks.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(240.0, 40.0))
        .build_ui(move |ui| {
            if Checkbox::with_state(CheckState::Mixed, "All namespaces")
                .show(ui)
                .clicked()
            {
                counter.set(counter.get() + 1);
            }
        });
    setup_egui(&mut harness);
    harness.run();
    assert_eq!(
        harness
            .get_by_label("All namespaces")
            .accesskit_node()
            .toggled(),
        Some(Toggled::Mixed)
    );
    harness.get_by_label("All namespaces").click();
    harness.run();
    assert_eq!(clicks.get(), 1);
}

#[test]
fn space_toggles_a_focused_switch() {
    let on = Rc::new(Cell::new(false));
    let state = on.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 40.0))
        .build_ui(move |ui| {
            let mut value = state.get();
            Switch::new(&mut value, "Check for updates automatically").show(ui);
            state.set(value);
        });
    setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_label("Check for updates automatically")
        .focus();
    harness.run();
    harness.key_press(egui::Key::Space);
    harness.run();
    assert!(on.get());
}

#[test]
fn disabled_controls_ignore_clicks() {
    let on = Rc::new(Cell::new(false));
    let state = on.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 40.0))
        .build_ui(move |ui| {
            let mut value = state.get();
            Checkbox::new(&mut value, "Include terminated pods")
                .enabled(false)
                .show(ui);
            state.set(value);
        });
    setup_egui(&mut harness);
    harness.run();
    harness.get_by_label("Include terminated pods").click();
    harness.run();
    assert!(!on.get());
}

#[test]
fn radio_cards_select_on_a_click_anywhere_on_the_card_and_show_their_body() {
    let selected = Rc::new(Cell::new(0));
    let state = selected.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(520.0, 240.0))
        .build_ui(move |ui| {
            ui.spacing_mut().item_spacing.y = space::S12;
            for (index, (title, description)) in [
                ("Automatic", "Use your system’s preferred terminal."),
                (
                    "Custom launcher",
                    "Use a command template for your preferred terminal.",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let body = (index == 1).then_some(|ui: &mut Ui| {
                    type_scale::META.label(
                        ui,
                        "alacritty -e {command}",
                        ui.tokens().text.secondary,
                    );
                });
                if RadioCard::new(state.get() == index, title, description)
                    .show_with_body(ui, body)
                    .response
                    .clicked()
                {
                    state.set(index);
                }
            }
        });
    setup_egui(&mut harness);
    harness.run();
    assert!(harness.query_by_label("alacritty -e {command}").is_none());

    let card = harness.get_by_label("Custom launcher").rect();
    primary_click(&mut harness, card.right_bottom() - egui::vec2(8.0, 8.0));
    harness.run();
    harness.run();
    assert_eq!(selected.get(), 1);
    harness.get_by_label("alacritty -e {command}");
    harness.ui_harness("selection/radio_card_with_body");
}
