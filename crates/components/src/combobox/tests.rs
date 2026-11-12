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
fn fuzzy_filter_orders_strongest_matches_first() {
    let resources = ["my-api", "a-p-i", "api-server", "api"];
    let selected = Rc::new(RefCell::new(None));
    let selected_for_ui = selected.clone();
    let mut harness = Harness::new_ui(move |ui| {
        TailwindCombobox::from_label("Resources")
            .placeholder("Search resources...")
            .width(250.0)
            .filter_by(|resource: &&str| *resource)
            .show_items(ui, &resources, |cb, resource| {
                if cb.item(*resource, false).clicked() {
                    *selected_for_ui.borrow_mut() = Some(*resource);
                }
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Resources")
        .click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("api".into()));
    harness.run();

    let api = harness.get_by_label("api").rect();
    let api_server = harness.get_by_label("api-server").rect();
    let substring = harness.get_by_label("my-api").rect();
    let subsequence = harness.get_by_label("a-p-i").rect();
    assert!(api.top() < api_server.top());
    assert!(api_server.top() < substring.top());
    assert!(substring.top() < subsequence.top());
    harness.ui_harness("comboboxes/fuzzy_filter_orders_strongest_matches_first/ranked_results");

    harness.key_down(Key::Enter);
    harness.run();
    assert_eq!(*selected.borrow(), Some("api"));
}

#[test]
fn fuzzy_filter_preserves_source_order_for_equally_scored_matches() {
    let resources = ["z-api", "a-api"];
    let mut harness = Harness::new_ui(move |ui| {
        TailwindCombobox::from_label("Resources")
            .placeholder("Search resources...")
            .width(250.0)
            .filter_by(|resource: &&str| *resource)
            .show_items(ui, &resources, |cb, resource| {
                cb.item(*resource, false);
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Resources")
        .click();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("api".into()));
    harness.run();

    assert!(
        harness.get_by_label("z-api").rect().top() < harness.get_by_label("a-api").rect().top()
    );
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

const RANKED_RESOURCES: [&str; 8] = [
    "pod-a", "pod-b", "pod-c", "pod-d", "pod-e", "pod-f", "pod-g", "pod",
];

/// Opens a combobox over [`RANKED_RESOURCES`] and records the last selected
/// resource and whether the Select all row was activated.
struct RankedComboboxHarness {
    harness: Harness<'static>,
    items: Rc<RefCell<Vec<&'static str>>>,
    selected: Rc<RefCell<Option<&'static str>>>,
    select_all_activated: Rc<RefCell<bool>>,
}

impl RankedComboboxHarness {
    fn open(with_select_all: bool) -> Self {
        Self::open_with(RANKED_RESOURCES.to_vec(), with_select_all)
    }

    fn open_with(items: Vec<&'static str>, with_select_all: bool) -> Self {
        let items = Rc::new(RefCell::new(items));
        let items_for_ui = items.clone();
        let selected = Rc::new(RefCell::new(None));
        let select_all_activated = Rc::new(RefCell::new(false));
        let selected_for_ui = selected.clone();
        let select_all_for_ui = select_all_activated.clone();
        let mut harness = Harness::new_ui(move |ui| {
            let mut combobox = TailwindCombobox::from_label("Resources").width(250.0);
            if with_select_all {
                combobox = combobox.select_all(false);
            }
            let items = items_for_ui.borrow();
            let response = combobox.filter_by(|resource: &&str| *resource).show_items(
                ui,
                items.iter(),
                |cb, resource| {
                    if cb.item(*resource, false).clicked() {
                        *selected_for_ui.borrow_mut() = Some(*resource);
                    }
                },
            );
            if response.select_all_clicked {
                *select_all_for_ui.borrow_mut() = true;
            }
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Resources")
            .click();
        harness.run();
        Self {
            harness,
            items,
            selected,
            select_all_activated,
        }
    }

    fn press(&mut self, key: Key, times: usize) {
        for _ in 0..times {
            self.harness.key_down(key);
            self.harness.run();
        }
    }

    fn set_items(&mut self, items: Vec<&'static str>) {
        *self.items.borrow_mut() = items;
        self.harness.run();
    }

    fn type_text(&mut self, text: &str) {
        self.harness
            .input_mut()
            .events
            .push(egui::Event::Text(text.into()));
        self.harness.run();
    }
}

#[test]
fn typing_without_arrowing_keeps_the_best_match_highlighted() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.type_text("po");
    combobox.type_text("d");
    combobox.press(Key::Enter, 1);

    assert_eq!(*combobox.selected.borrow(), Some("pod"));
}

#[test]
fn typing_without_arrowing_keeps_the_best_match_highlighted_with_select_all() {
    let mut combobox = RankedComboboxHarness::open(true);
    combobox.type_text("pod");
    combobox.press(Key::Enter, 1);

    assert_eq!(*combobox.selected.borrow(), Some("pod"));
    assert!(!*combobox.select_all_activated.borrow());
}

#[test]
fn arrowed_item_stays_highlighted_when_the_filter_reranks_it() {
    let mut combobox = RankedComboboxHarness::open(false);
    // pod-f is the sixth row unfiltered, but "pod" ranks the exact match first.
    combobox.press(Key::ArrowDown, 5);
    combobox.type_text("pod");
    combobox.press(Key::Enter, 1);

    assert_eq!(*combobox.selected.borrow(), Some("pod-f"));
}

#[test]
fn arrowed_item_stays_highlighted_when_the_filter_is_extended_again() {
    let mut combobox = RankedComboboxHarness::open(true);
    combobox.press(Key::ArrowDown, 6);
    combobox.type_text("po");
    combobox.type_text("d");
    combobox.press(Key::Enter, 1);

    assert_eq!(*combobox.selected.borrow(), Some("pod-f"));
    assert!(!*combobox.select_all_activated.borrow());
}

#[test]
fn arrowed_item_is_followed_when_the_select_all_row_reappears() {
    let mut combobox = RankedComboboxHarness::open(true);
    combobox.type_text("pod");
    combobox.press(Key::ArrowDown, 3);
    combobox.press(Key::Backspace, 3);
    combobox.press(Key::Enter, 1);

    assert!(!*combobox.select_all_activated.borrow());
    assert_eq!(*combobox.selected.borrow(), Some("pod-c"));
}

#[test]
fn highlight_returns_to_the_best_match_when_the_arrowed_item_disappears() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 5);
    // pod-f cannot match "pod-a", so the highlight falls back to the top row.
    combobox.type_text("pod-a");
    // pod-f matches again, but it is no longer explicitly chosen, so the
    // highlight stays on the best match instead of jumping back to it.
    combobox.press(Key::Backspace, 2);
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("pod"));
}

#[test]
fn select_all_row_resets_to_the_best_match_once_typing_hides_it() {
    let mut combobox = RankedComboboxHarness::open(true);
    combobox.press(Key::ArrowDown, 1);
    combobox.press(Key::ArrowUp, 1);
    combobox.type_text("pod");
    combobox.press(Key::Enter, 1);

    assert_eq!(*combobox.selected.borrow(), Some("pod"));
    assert!(!*combobox.select_all_activated.borrow());
}

#[test]
fn clearing_the_filter_without_arrowing_returns_to_select_all() {
    let mut combobox = RankedComboboxHarness::open(true);
    combobox.type_text("pod");
    combobox.press(Key::Backspace, 3);
    combobox.press(Key::Enter, 1);

    assert!(*combobox.select_all_activated.borrow());
    assert_eq!(*combobox.selected.borrow(), None);
}

#[test]
fn filter_and_enter_in_the_same_frame_select_the_best_match() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox
        .harness
        .input_mut()
        .events
        .push(egui::Event::Text("pod".into()));
    combobox.harness.key_down(Key::Enter);
    combobox.harness.run();

    assert_eq!(*combobox.selected.borrow(), Some("pod"));
}

#[test]
fn filter_and_enter_in_the_same_frame_select_the_arrowed_item() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 5);
    combobox
        .harness
        .input_mut()
        .events
        .push(egui::Event::Text("pod".into()));
    combobox.harness.key_down(Key::Enter);
    combobox.harness.run();

    assert_eq!(*combobox.selected.borrow(), Some("pod-f"));
}

#[test]
fn closing_the_popup_forgets_the_arrowed_item() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 4);
    combobox.press(Key::Escape, 1);
    combobox
        .harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Resources")
        .click();
    combobox.harness.run();
    combobox.type_text("pod");
    combobox.press(Key::Enter, 1);

    assert_eq!(*combobox.selected.borrow(), Some("pod"));
}

#[test]
fn reopening_the_popup_starts_from_the_first_row() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 4);
    combobox.press(Key::Escape, 1);
    combobox
        .harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Resources")
        .click();
    combobox.harness.run();
    combobox.press(Key::Enter, 1);

    assert_eq!(*combobox.selected.borrow(), Some("pod-a"));
}

#[test]
fn arrowing_through_duplicate_filter_texts_visits_every_row() {
    let items = vec!["a", "dup", "dup", "c"];
    let mut combobox = RankedComboboxHarness::open_with(items.clone(), false);
    combobox.press(Key::ArrowDown, 2);
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("dup"));

    let mut combobox = RankedComboboxHarness::open_with(items.clone(), false);
    combobox.press(Key::ArrowDown, 3);
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("c"));

    // The second duplicate is still the highlighted one after re-filtering.
    let mut combobox = RankedComboboxHarness::open_with(items, false);
    combobox.press(Key::ArrowDown, 2);
    combobox.type_text("d");
    combobox.press(Key::ArrowDown, 1);
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("dup"));
}

#[test]
fn removing_the_arrowed_item_from_the_caller_resets_to_the_top_row() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 3);
    combobox.set_items(vec!["pod-a", "pod-b", "pod-c", "pod-e"]);
    combobox.press(Key::Enter, 1);

    assert_eq!(*combobox.selected.borrow(), Some("pod-a"));
}

#[test]
fn inserting_before_the_arrowed_item_in_the_caller_keeps_it_highlighted() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 2);
    combobox.set_items(vec!["new", "pod-a", "pod-b", "pod-c", "pod-d"]);
    combobox.press(Key::Enter, 1);

    assert_eq!(*combobox.selected.borrow(), Some("pod-c"));
}

#[test]
fn shrinking_the_list_below_the_highlighted_index_clamps_it() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 5);
    combobox.set_items(vec!["only-a", "only-b"]);
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("only-a"));

    // A focus that was never explicitly chosen is clamped to the last row.
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.set_items(vec![]);
    combobox.press(Key::ArrowDown, 1);
    combobox.set_items(vec!["only-a", "only-b"]);
    combobox.press(Key::Enter, 1);
    assert!(combobox.selected.borrow().is_some());
}

#[test]
fn hovering_a_row_does_not_change_the_highlight_when_typing() {
    let mut combobox = RankedComboboxHarness::open(false);
    let hovered = combobox.harness.get_by_label("pod-d").rect().center();
    combobox.harness.event(egui::Event::PointerMoved(hovered));
    combobox.harness.run();
    combobox.type_text("pod");
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("pod"));

    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 2);
    let hovered = combobox.harness.get_by_label("pod-d").rect().center();
    combobox.harness.event(egui::Event::PointerMoved(hovered));
    combobox.harness.run();
    combobox.type_text("pod");
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("pod-c"));
}

#[test]
fn ctrl_click_keeps_the_popup_open_and_the_arrowed_item_highlighted() {
    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 2);
    let target = combobox.harness.get_by_label("pod-a").rect().center();
    let modifiers = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    combobox
        .harness
        .event(egui::Event::ModifiersChanged(modifiers));
    combobox.harness.event(egui::Event::PointerMoved(target));
    for pressed in [true, false] {
        combobox.harness.event(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
    combobox.harness.run();
    combobox
        .harness
        .event(egui::Event::ModifiersChanged(egui::Modifiers::default()));
    combobox.harness.run();
    assert_eq!(*combobox.selected.borrow(), Some("pod-a"));

    combobox.type_text("pod");
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("pod-c"));
}

#[test]
fn arrowing_past_either_end_then_refiltering_keeps_the_end_row() {
    let mut combobox = RankedComboboxHarness::open_with(vec!["x-1", "x-2", "x-3"], false);
    combobox.press(Key::ArrowDown, 20);
    combobox.type_text("x");
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("x-3"));

    let mut combobox = RankedComboboxHarness::open(false);
    combobox.press(Key::ArrowDown, 2);
    combobox.press(Key::ArrowUp, 20);
    combobox.type_text("pod");
    combobox.press(Key::Enter, 1);
    assert_eq!(*combobox.selected.borrow(), Some("pod-a"));
}
