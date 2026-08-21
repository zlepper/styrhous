use super::*;
use crate::test_support::UiHarnessSnapshot;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

struct Person {
    name: String,
    id: u32,
    active: bool,
}

fn test_people() -> Vec<Person> {
    vec![
        Person {
            name: "Michael Foster".into(),
            id: 1,
            active: true,
        },
        Person {
            name: "Floyd Miles".into(),
            id: 2,
            active: false,
        },
        Person {
            name: "Emily Selman".into(),
            id: 3,
            active: false,
        },
        Person {
            name: "Benjamin Russel".into(),
            id: 4,
            active: true,
        },
    ]
}

#[test]
fn test_combobox_flow() {
    let people = test_people();
    let selected = Rc::new(RefCell::new(HashSet::new()));
    let selected_for_ui = selected.clone();
    let mut harness = Harness::new_ui(move |ui| {
        let mut selected = selected_for_ui.borrow_mut();
        TailwindCombobox::from_label("Assigned to")
            .placeholder("Search...")
            .width(250.0)
            .select_all(selected.len() == people.len())
            .filter_by(|person: &Person| &person.name)
            .show_items(ui, &people, |cb, person| {
                let is_selected = selected.contains(&person.id);
                if let Some(action) = cb
                    .item_with_status(&person.name, is_selected, Some(person.active))
                    .selection_action()
                {
                    match action {
                        SelectionAction::Replace => {
                            selected.clear();
                            selected.insert(person.id);
                        }
                        SelectionAction::Toggle => {
                            if !selected.insert(person.id) {
                                selected.remove(&person.id);
                            }
                        }
                    }
                }
            });
    });
    crate::test_support::setup_egui(&mut harness);

    harness.run();
    harness.ui_harness("comboboxes/test_combobox_flow/closed");

    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Assigned to")
        .click();
    harness.run();
    harness.ui_harness("comboboxes/test_combobox_flow/open");

    harness
        .input_mut()
        .events
        .push(egui::Event::Text("mi".into()));
    harness.run();
    harness.ui_harness("comboboxes/test_combobox_flow/filtered");

    harness.get_by_label("Michael Foster").click();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([1]));

    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Assigned to")
        .click();
    harness.run();
    harness.ui_harness("comboboxes/test_combobox_flow/selected");
}

#[test]
fn modifier_click_toggles_without_closing() {
    let people = test_people();
    let selected = Rc::new(RefCell::new(HashSet::from([1])));
    let selected_for_ui = selected.clone();
    let select_all_activated = Rc::new(RefCell::new(false));
    let select_all_for_ui = select_all_activated.clone();
    let mut harness = Harness::new_ui(move |ui| {
        let mut selected = selected_for_ui.borrow_mut();
        let response = TailwindCombobox::from_label("People")
            .placeholder("Search...")
            .width(250.0)
            .select_all(selected.len() == people.len())
            .filter_by(|person: &Person| &person.name)
            .show_items(ui, &people, |cb, person| {
                let is_selected = selected.contains(&person.id);
                if let Some(action) = cb.item(&person.name, is_selected).selection_action() {
                    match action {
                        SelectionAction::Replace => {
                            selected.clear();
                            selected.insert(person.id);
                        }
                        SelectionAction::Toggle => {
                            if !selected.insert(person.id) {
                                selected.remove(&person.id);
                            }
                        }
                    }
                }
            });
        if response.select_all_clicked {
            *select_all_for_ui.borrow_mut() = true;
        }
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "People")
        .click();
    harness.run();
    harness.get_by_label("Select all");

    let floyd_position = harness.get_by_label("Floyd Miles").rect().center();
    let modifiers = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    harness.event(egui::Event::ModifiersChanged(modifiers));
    harness.event(egui::Event::PointerMoved(floyd_position));
    harness.event(egui::Event::PointerButton {
        pos: floyd_position,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    harness.event(egui::Event::PointerButton {
        pos: floyd_position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.run();
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::default()));
    assert_eq!(*selected.borrow(), HashSet::from([1, 2]));
    harness.get_by_label("Select all");
    assert!(!*select_all_activated.borrow());
}

#[test]
fn select_all_is_hidden_while_searching() {
    let people = test_people();
    let mut harness = Harness::new_ui(move |ui| {
        TailwindCombobox::from_label("People")
            .placeholder("Search...")
            .width(250.0)
            .select_all(false)
            .filter_by(|person: &Person| &person.name)
            .show_items(ui, &people, |cb, person| {
                cb.item(&person.name, false);
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "People")
        .click();
    harness.run();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("flo".into()));
    harness.run();

    assert!(harness.query_by_label("Select all").is_none());
    harness.get_by_label("Floyd Miles");
}

#[test]
fn keyboard_navigation_moves_focus_and_selects_the_focused_item() {
    let people = test_people();
    let selected = Rc::new(RefCell::new(HashSet::new()));
    let selected_for_ui = selected.clone();
    let mut harness = Harness::new_ui(move |ui| {
        let mut selected = selected_for_ui.borrow_mut();
        TailwindCombobox::from_label("People")
            .placeholder("Search...")
            .width(250.0)
            .filter_by(|person: &Person| &person.name)
            .show_items(ui, &people, |cb, person| {
                if let Some(action) = cb
                    .item(&person.name, selected.contains(&person.id))
                    .selection_action()
                {
                    match action {
                        SelectionAction::Replace => {
                            selected.clear();
                            selected.insert(person.id);
                        }
                        SelectionAction::Toggle => unreachable!("keyboard activation replaces"),
                    }
                }
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "People")
        .click();
    harness.run();

    harness.key_down(Key::ArrowDown);
    harness.run();
    harness.key_down(Key::Enter);
    harness.run();

    assert_eq!(*selected.borrow(), HashSet::from([2]));
    assert!(harness.query_by_label("Michael Foster").is_none());
}

#[test]
fn long_namespace_names_are_truncated_without_hiding_selection_affordances() {
    let namespaces = [
        "namespace-with-a-very-long-name-that-must-not-overlap-the-status-or-checkmark",
        "default",
    ];
    let selected_namespace = namespaces[0];
    let mut harness = Harness::new_ui(move |ui| {
        TailwindCombobox::from_label("Namespaces")
            .selected_text(selected_namespace)
            .selected_status(Some(true))
            .width(230.0)
            .filter_by(|namespace: &&str| *namespace)
            .show_items(ui, &namespaces, |cb, namespace| {
                cb.item_with_status(*namespace, *namespace == selected_namespace, Some(true));
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("comboboxes/long_namespace_names_are_truncated_without_hiding_selection_affordances/long_namespace_closed");
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
        .hover();
    harness.run();
    harness.ui_harness("comboboxes/long_namespace_names_are_truncated_without_hiding_selection_affordances/long_namespace_closed_tooltip");

    let namespaces = [
        "namespace-with-a-very-long-name-that-must-not-overlap-the-status-or-checkmark",
        "default",
    ];
    let mut open_harness = Harness::new_ui(move |ui| {
        TailwindCombobox::from_label("Namespaces")
            .selected_text(selected_namespace)
            .selected_status(Some(true))
            .width(230.0)
            .filter_by(|namespace: &&str| *namespace)
            .show_items(ui, &namespaces, |cb, namespace| {
                cb.item_with_status(*namespace, *namespace == selected_namespace, Some(true));
            });
    });
    crate::test_support::setup_egui(&mut open_harness);
    open_harness.run();
    open_harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
        .click();
    open_harness.run();
    open_harness.ui_harness("comboboxes/long_namespace_names_are_truncated_without_hiding_selection_affordances/long_namespace_open");
    open_harness.get_by_label(selected_namespace).hover();
    open_harness.run_ok();
    open_harness.ui_harness("comboboxes/long_namespace_names_are_truncated_without_hiding_selection_affordances/long_namespace_open_tooltip");
}

#[test]
fn keyboard_navigation_does_not_scroll_a_three_item_result_list() {
    let namespaces = ["system", "staging", "sandbox", "default"];
    let selected = Rc::new(RefCell::new(None));
    let selected_for_ui = selected.clone();
    let mut harness = Harness::new_ui(move |ui| {
        TailwindCombobox::from_label("Namespaces")
            .placeholder("Search namespaces...")
            .width(250.0)
            .filter_by(|namespace: &&str| *namespace)
            .show_items(ui, &namespaces, |cb, namespace| {
                if cb.item(*namespace, false).clicked() {
                    *selected_for_ui.borrow_mut() = Some(*namespace);
                }
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
        .click();
    harness.run();
    assert!(
        harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Search Namespaces")
            .is_focused(),
        "the opened combobox must expose its focused filter as a labelled text input"
    );
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("s".into()));
    harness.run();

    let system_rect = harness.get_by_label("system").rect();
    let staging_rect = harness.get_by_label("staging").rect();
    let sandbox_rect = harness.get_by_label("sandbox").rect();

    harness.key_down(Key::ArrowDown);
    harness.run();
    harness.key_down(Key::ArrowDown);
    harness.run();

    assert_eq!(harness.get_by_label("system").rect(), system_rect);
    assert_eq!(harness.get_by_label("staging").rect(), staging_rect);
    assert_eq!(harness.get_by_label("sandbox").rect(), sandbox_rect);
    harness.ui_harness("comboboxes/keyboard_navigation_does_not_scroll_a_three_item_result_list/three_filtered_results");

    harness.key_down(Key::Enter);
    harness.run();
    assert_eq!(*selected.borrow(), Some("sandbox"));
}

#[test]
fn keyboard_navigation_scrolls_a_focused_item_into_view() {
    let namespaces = (0..20)
        .map(|index| format!("namespace-{index:03}"))
        .collect::<Vec<_>>();
    let selected = Rc::new(RefCell::new(None));
    let selected_for_ui = selected.clone();
    let mut harness = Harness::new_ui(move |ui| {
        TailwindCombobox::from_label("Namespaces")
            .placeholder("Search namespaces...")
            .width(250.0)
            .filter_by(|namespace: &String| namespace)
            .show_items(ui, &namespaces, |cb, namespace| {
                if cb.item(namespace, false).clicked() {
                    *selected_for_ui.borrow_mut() = Some(namespace.clone());
                }
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    let combobox_rect = harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
        .rect();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
        .click();
    harness.run();

    for _ in 0..10 {
        harness.key_down(Key::ArrowDown);
        harness.run();
    }

    let focused_rect = harness.get_by_label("namespace-010").rect();
    assert!(focused_rect.top() >= combobox_rect.bottom());
    assert!(focused_rect.bottom() <= combobox_rect.bottom() + DROPDOWN_MAX_HEIGHT);
    harness.ui_harness(
        "comboboxes/keyboard_navigation_scrolls_a_focused_item_into_view/keyboard_scroll_into_view",
    );

    harness.key_down(Key::Enter);
    harness.run();
    assert_eq!(selected.borrow().as_deref(), Some("namespace-010"));
}

#[test]
fn dropdown_expands_after_backspacing_to_more_search_results() {
    let namespaces = ["system", "staging", "sandbox", "services"];
    let mut harness = Harness::new_ui(move |ui| {
        TailwindCombobox::from_label("Namespaces")
            .placeholder("Search namespaces...")
            .width(250.0)
            .filter_by(|namespace: &&str| *namespace)
            .show_items(ui, &namespaces, |cb, namespace| {
                cb.item(*namespace, false);
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
        .click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("sy".into()));
    harness.run();
    harness.get_by_label("system");

    harness.key_down(Key::Backspace);
    harness.run();
    harness.get_by_label("staging");
    harness.get_by_label("sandbox");
    harness.ui_harness(
        "comboboxes/dropdown_expands_after_backspacing_to_more_search_results/filter_expands",
    );
}

#[test]
fn dropdown_with_two_hundred_items_is_capped_to_a_scrollable_height() {
    let namespaces = (0..200)
        .map(|index| format!("namespace-{index:03}"))
        .collect::<Vec<_>>();
    let mut harness = Harness::new_ui(move |ui| {
        TailwindCombobox::from_label("Namespaces")
            .placeholder("Search namespaces...")
            .width(250.0)
            .filter_by(|namespace: &String| namespace)
            .show_items(ui, &namespaces, |cb, namespace| {
                cb.item(namespace, false);
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Namespaces")
        .click();
    harness.run();
    harness.ui_harness("comboboxes/dropdown_with_two_hundred_items_is_capped_to_a_scrollable_height/two_hundred_items");
}
