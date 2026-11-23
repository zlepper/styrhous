use std::cell::{Cell, RefCell};
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::*;
use crate::MoreButton;
use crate::test_support::{UiHarnessSnapshot, setup_egui};

/// The C07 resource actions menu: icons, shortcut hints, a submenu, groups
/// and a destructive item last.
fn resource_actions(menu: &mut Menu<'_>) {
    menu.item(
        MenuItem::new("Edit YAML")
            .icon(Icon::PencilSquare)
            .shortcut("E"),
    );
    menu.item(
        MenuItem::new("View logs")
            .icon(Icon::DocumentText)
            .shortcut("L"),
    );
    menu.submenu(MenuItem::new("Shell").icon(Icon::CommandLine), |menu| {
        menu.heading("Debug image");
        menu.item(MenuItem::new("Busybox").detail("General profile"));
        menu.item(MenuItem::new("Netshoot").detail("Network admin profile"));
    });
    menu.item(
        MenuItem::new("Copy name")
            .icon(Icon::ClipboardDocument)
            .shortcut("⌘C"),
    );
    menu.separator();
    menu.item(
        MenuItem::new("Restart rollout")
            .icon(Icon::ArrowPath)
            .enabled(false)
            .disabled_reason("Deployment is paused"),
    );
    menu.separator();
    menu.item(
        MenuItem::new("Delete")
            .icon(Icon::Trash)
            .shortcut("⌫")
            .danger(),
    );
}

fn open_menu(harness: &mut Harness<'_>) {
    setup_egui(harness);
    harness.run();
    harness.get_by_label("More actions for coredns").click();
    harness.run();
}

#[test]
fn menu_items_align_icons_labels_and_shortcut_hints() {
    let mut harness = Harness::builder().build_ui(|ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            preview(ui, 220.0, "Resource actions", |menu| {
                resource_actions(menu);
                menu.separator();
                menu.heading("Columns");
                menu.checkbox("Owner", CheckState::Off);
                menu.checkbox("Ready", CheckState::On);
            });
        });
    });
    setup_egui(&mut harness);
    harness.run();

    let edit = harness.get_by_label("Edit YAML").rect();
    let copy = harness.get_by_label("Copy name").rect();
    assert_eq!(edit.height(), size::OPTION);
    assert_eq!(edit.left(), copy.left(), "labels share the icon column");
    harness.ui_harness("menu/menu_items_align_icons_labels_and_shortcut_hints");
}

#[test]
fn more_button_opens_its_menu_below_with_the_trigger_pressed() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(480.0, 380.0))
        .build_ui(|ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(240.0);
                MoreButton::new("More actions for coredns").show(ui, resource_actions);
            });
        });
    open_menu(&mut harness);

    let trigger = harness.get_by_label("More actions for coredns").rect();
    let menu = harness.get_by_label("More actions for coredns menu").rect();
    assert_eq!(menu.right(), trigger.right(), "right edges align");
    assert_eq!(menu.top(), trigger.bottom() + size::POPOVER_GAP);
    assert!(menu.width() >= size::MENU_MIN_WIDTH);
    harness.ui_harness("menu/more_button_opens_its_menu_below_with_the_trigger_pressed");
}

#[test]
fn hovering_a_submenu_item_opens_it_beside_the_menu() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(720.0, 380.0))
        .build_ui(|ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(240.0);
                MoreButton::new("More actions for coredns").show(ui, resource_actions);
            });
        });
    open_menu(&mut harness);
    harness.get_by_label("Shell").hover();
    harness.run();

    let shell = harness.get_by_label("Shell").rect();
    let busybox = harness.get_by_label("Busybox (General profile)").rect();
    assert!(
        busybox.left() > shell.right(),
        "the submenu opens to the right"
    );
    harness.ui_harness("menu/hovering_a_submenu_item_opens_it_beside_the_menu");
}

#[test]
fn disabled_items_stay_readable_but_do_not_activate() {
    let activated = Rc::new(Cell::new(false));
    let activated_in_ui = activated.clone();
    let mut harness = Harness::new_ui(move |ui| {
        MoreButton::new("More actions for coredns").show(ui, |menu| {
            let response = menu.item(MenuItem::new("Restart rollout").enabled(false));
            activated_in_ui.set(activated_in_ui.get() || response.clicked());
        });
    });
    open_menu(&mut harness);
    harness.get_by_label("Restart rollout").click();
    harness.run();

    assert!(!activated.get());
    assert!(
        harness.query_by_label("Restart rollout").is_some(),
        "the menu stays open"
    );
}

#[test]
fn checkbox_items_toggle_without_closing_the_menu() {
    let visible = Rc::new(RefCell::new(false));
    let visible_in_ui = visible.clone();
    let mut harness = Harness::new_ui(move |ui| {
        MoreButton::new("More actions for coredns").show(ui, |menu| {
            let state = CheckState::from(*visible_in_ui.borrow());
            if menu.checkbox("Owner", state).clicked() {
                let mut visible = visible_in_ui.borrow_mut();
                *visible = !*visible;
            }
        });
    });
    open_menu(&mut harness);
    harness.get_by_label("Owner").click();
    harness.run();

    assert!(*visible.borrow());
    assert!(
        harness.query_by_label("Owner").is_some(),
        "the menu stays open"
    );
}

#[test]
fn a_submenu_of_a_left_opened_submenu_keeps_cascading_left() {
    let mut harness = Harness::builder().build_ui(|ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(740.0);
            MoreButton::new("More actions for coredns").show(ui, |menu| {
                menu.item(MenuItem::new("Edit YAML"));
                menu.submenu(MenuItem::new("Debug shell"), |menu| {
                    menu.submenu(MenuItem::new("coredns"), |menu| {
                        menu.item(MenuItem::new("Busybox"));
                    });
                });
            });
        });
    });
    setup_egui(&mut harness);
    // Narrow enough that the first submenu reaches the screen's edge.
    harness.set_size(egui::vec2(900.0, 380.0));
    harness.run();
    harness.get_by_label("More actions for coredns").click();
    harness.run();
    harness.get_by_label("Debug shell").hover();
    harness.run();
    harness.get_by_label("coredns").hover();
    harness.run();

    let main = harness.get_by_label("More actions for coredns menu").rect();
    let container = harness.get_by_label("Debug shell menu").rect();
    let images = harness.get_by_label("coredns menu").rect();
    assert!(
        container.right() <= main.left(),
        "the submenu flips left at the screen edge"
    );
    assert!(
        images.right() <= container.left(),
        "its own submenu keeps cascading left rather than covering it"
    );
    assert!(images.left() >= 0.0, "it stays on screen");
}

#[test]
fn an_elided_item_label_shows_its_full_text_in_a_tooltip() {
    let label = "registry.example.com/platform/debugging-tools:2026.11-with-a-long-tag";
    let mut harness = Harness::builder()
        .with_size(egui::vec2(480.0, 240.0))
        .build_ui(|ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                preview(ui, 220.0, "Images", |menu| {
                    menu.item(MenuItem::new(label));
                    menu.item(MenuItem::new("Busybox"));
                });
            });
        });
    setup_egui(&mut harness);
    harness.run();
    assert!(harness.query_all_by_label(label).count() == 1);

    harness.get_by_label(label).hover();
    harness.run();
    assert_eq!(
        harness.query_all_by_label(label).count(),
        2,
        "the tooltip repeats the full label"
    );

    harness.get_by_label("Busybox").hover();
    harness.run();
    assert_eq!(
        harness.query_all_by_label("Busybox").count(),
        1,
        "a label that fits has no tooltip"
    );
}

#[test]
fn submenus_with_room_on_the_right_keep_cascading_right() {
    let mut harness = Harness::builder().build_ui(|ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(220.0);
            MoreButton::new("More actions for coredns").show(ui, |menu| {
                menu.item(MenuItem::new("Edit YAML"));
                menu.submenu(MenuItem::new("Debug shell"), |menu| {
                    menu.submenu(MenuItem::new("coredns"), |menu| {
                        menu.item(MenuItem::new("Busybox"));
                    });
                });
            });
        });
    });
    open_menu(&mut harness);
    harness.get_by_label("Debug shell").hover();
    harness.run();
    harness.get_by_label("coredns").hover();
    harness.run();

    let main = harness.get_by_label("More actions for coredns menu").rect();
    let container = harness.get_by_label("Debug shell menu").rect();
    let images = harness.get_by_label("coredns menu").rect();
    assert!(container.left() >= main.right(), "the submenu opens right");
    assert!(
        images.left() >= container.right(),
        "its own submenu opens right too"
    );
}
