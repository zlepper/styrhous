use super::*;
use crate::test_support::{UiHarnessSnapshot, primary_click, primary_press, primary_release};
use crate::{BulkSelectionBar, CellAlign, HeaderState, column_visibility_menu};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Clone)]
struct User {
    id: u32,
    name: String,
    title: String,
    email: String,
    role: String,
}

fn test_users() -> Vec<User> {
    vec![
        User {
            id: 1,
            name: "Lindsay Walton".into(),
            title: "Front-end Developer".into(),
            email: "lindsay.walton@example.com".into(),
            role: "Member".into(),
        },
        User {
            id: 2,
            name: "Courtney Henry".into(),
            title: "Designer".into(),
            email: "courtney.henry@example.com".into(),
            role: "Admin".into(),
        },
        User {
            id: 3,
            name: "Tom Cook".into(),
            title: "Director of Product".into(),
            email: "tom.cook@example.com".into(),
            role: "Member".into(),
        },
        User {
            id: 4,
            name: "Whitney Francis".into(),
            title: "Copywriter".into(),
            email: "whitney.francis@example.com".into(),
            role: "Admin".into(),
        },
        User {
            id: 5,
            name: "Leonard Krasner".into(),
            title: "Senior Designer".into(),
            email: "leonard.krasner@example.com".into(),
            role: "Owner".into(),
        },
        User {
            id: 6,
            name: "Floyd Miles".into(),
            title: "Principal Designer".into(),
            email: "floyd.miles@example.com".into(),
            role: "Member".into(),
        },
    ]
}

#[test]
fn test_table_basic() {
    let users = test_users();

    let mut harness = Harness::new_ui(|ui| {
        Table::new("users-table")
            .column("name", "Name", |col| col.initial_width(150.0))
            .column("title", "Title", |col| col.initial_width(150.0))
            .column("email", "Email", |col| col.initial_width(200.0))
            .column("role", "Role", |col| col.initial_width(100.0))
            .show(ui, &users, |ui, user, col_index| {
                let text = match col_index {
                    0 => &user.name,
                    1 => &user.title,
                    2 => &user.email,
                    3 => &user.role,
                    _ => return,
                };
                TableRowBuilder::text(ui, text, col_index == 0);
            });
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/test_table_basic/basic");
}

#[test]
fn test_table_alternating_rows() {
    let users = test_users();

    let mut harness = Harness::new_ui(|ui| {
        Table::new("users-alternating")
            .column("name", "Name", |col| col.initial_width(200.0))
            .column("email", "Email", |col| col.initial_width(250.0))
            .show(ui, &users, |ui, user, col_index| {
                let text = match col_index {
                    0 => &user.name,
                    1 => &user.email,
                    _ => return,
                };
                TableRowBuilder::text(ui, text, col_index == 0);
            });
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/test_table_alternating_rows/alternating_rows");
}

#[test]
fn test_table_with_selection() {
    let users = test_users();
    let mut selection: HashSet<u32> = HashSet::new();

    let mut harness = Harness::new_ui(|ui| {
        Table::new("users-selection")
            .column("name", "Name", |col| col.initial_width(150.0))
            .column("title", "Title", |col| col.initial_width(150.0))
            .selectable()
            .show_selectable(
                ui,
                &users,
                &mut selection,
                |user| user.id,
                |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                },
            );
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Select all rows");
    harness.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Select row 1");
    harness.ui_harness("tables/test_table_with_selection/with_selection");
}

#[test]
fn selectable_table_toggles_rows_and_only_visible_rows_from_its_header() {
    let users = test_users();
    let selected = Rc::new(RefCell::new(HashSet::from([3])));
    let selected_for_ui = selected.clone();
    let visible_users = &users[..2];
    let mut harness = Harness::new_ui(move |ui| {
        Table::new("users-selection-interactions")
            .column("name", "Name", |col| col.initial_width(150.0))
            .selectable()
            .show_selectable(
                ui,
                visible_users,
                &mut selected_for_ui.borrow_mut(),
                |user| user.id,
                |ui, user, _| TableRowBuilder::text(ui, &user.name, true),
            );
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.get_by_label("Select row 1").click();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([1, 3]));

    harness.get_by_label("Select all rows").click();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([1, 2, 3]));

    harness.get_by_label("Select all rows").click();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([3]));
}

#[test]
fn test_table_select_all() {
    let users = test_users();
    // All users selected
    let mut selection: HashSet<u32> = users.iter().map(|u| u.id).collect();

    let mut harness = Harness::new_ui(|ui| {
        Table::new("users-select-all")
            .column("name", "Name", |col| col.initial_width(150.0))
            .column("title", "Title", |col| col.initial_width(150.0))
            .selectable()
            .show_selectable(
                ui,
                &users,
                &mut selection,
                |user| user.id,
                |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                },
            );
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/test_table_select_all/select_all");
}

#[test]
fn test_table_select_all_indeterminate() {
    let users = test_users();
    // Only some users selected (partial selection)
    let mut selection: HashSet<u32> = HashSet::new();
    selection.insert(1);
    selection.insert(3);

    let mut harness = Harness::new_ui(|ui| {
        Table::new("users-indeterminate")
            .column("name", "Name", |col| col.initial_width(150.0))
            .column("title", "Title", |col| col.initial_width(150.0))
            .selectable()
            .show_selectable(
                ui,
                &users,
                &mut selection,
                |user| user.id,
                |ui, user, col_index| {
                    let text = match col_index {
                        0 => &user.name,
                        1 => &user.title,
                        _ => return,
                    };
                    TableRowBuilder::text(ui, text, col_index == 0);
                },
            );
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/test_table_select_all_indeterminate/select_all_indeterminate");
}

/// Helper function to sort users based on sort state
fn sort_users(users: &mut [User], sort_state: &Option<SortState>) {
    if let Some(state) = sort_state {
        users.sort_by(|a, b| {
            let cmp = match state.column_id.as_str() {
                "name" => a.name.cmp(&b.name),
                "title" => a.title.cmp(&b.title),
                "email" => a.email.cmp(&b.email),
                _ => std::cmp::Ordering::Equal,
            };
            match state.direction {
                SortDirection::Ascending => cmp,
                SortDirection::Descending => cmp.reverse(),
            }
        });
    }
}

#[test]
fn test_table_sorting() {
    // Comprehensive sorting test that demonstrates the full sorting workflow:
    // 1. No sort state (unsorted indicator shown)
    // 2. Sort by name ascending (data is sorted, up arrow shown)
    // 3. Sort by title descending (data is sorted, down arrow shown)

    // --- Snapshot 1: No sort state (unsorted) ---
    let users = test_users();
    let mut harness = Harness::new_ui(|ui| {
        let mut sort_state: Option<SortState> = None;
        Table::new("users-sortable")
            .column("name", "Name", |col| col.sortable().initial_width(150.0))
            .column("title", "Title", |col| col.sortable().initial_width(150.0))
            .column("email", "Email", |col| col.initial_width(200.0)) // Not sortable
            .show_sortable(ui, &users, &mut sort_state, |ui, user, col_index| {
                let text = match col_index {
                    0 => &user.name,
                    1 => &user.title,
                    2 => &user.email,
                    _ => return,
                };
                TableRowBuilder::text(ui, text, col_index == 0);
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/test_table_sorting/sorting_unsorted");

    // --- Snapshot 2: Sort by name ascending ---
    // Sort state is set before rendering, and data is sorted accordingly
    let mut users = test_users();
    let mut sort_state = Some(SortState::new("name", SortDirection::Ascending));
    sort_users(&mut users, &sort_state);

    let mut harness = Harness::new_ui(|ui| {
        Table::new("users-sort-name-asc")
            .column("name", "Name", |col| col.sortable().initial_width(150.0))
            .column("title", "Title", |col| col.sortable().initial_width(150.0))
            .column("email", "Email", |col| col.initial_width(200.0))
            .show_sortable(ui, &users, &mut sort_state, |ui, user, col_index| {
                let text = match col_index {
                    0 => &user.name,
                    1 => &user.title,
                    2 => &user.email,
                    _ => return,
                };
                TableRowBuilder::text(ui, text, col_index == 0);
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/test_table_sorting/sorting_name_asc");

    // --- Snapshot 3: Sort by title descending ---
    let mut users = test_users();
    let mut sort_state = Some(SortState::new("title", SortDirection::Descending));
    sort_users(&mut users, &sort_state);

    let mut harness = Harness::new_ui(|ui| {
        Table::new("users-sort-title-desc")
            .column("name", "Name", |col| col.sortable().initial_width(150.0))
            .column("title", "Title", |col| col.sortable().initial_width(150.0))
            .column("email", "Email", |col| col.initial_width(200.0))
            .show_sortable(ui, &users, &mut sort_state, |ui, user, col_index| {
                let text = match col_index {
                    0 => &user.name,
                    1 => &user.title,
                    2 => &user.email,
                    _ => return,
                };
                TableRowBuilder::text(ui, text, col_index == 0);
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/test_table_sorting/sorting_title_desc");
}

#[test]
fn test_table_column_toggle_menu() {
    let users = test_users();
    let hidden_columns: HashSet<String> = HashSet::new();

    let mut harness = Harness::new_ui(|ui| {
        Table::new("users-column-toggle")
            .column("name", "Name", |col| col.initial_width(150.0))
            .column("title", "Title", |col| col.initial_width(150.0))
            .column("email", "Email", |col| col.initial_width(200.0))
            .show_with_column_toggle(ui, &users, &hidden_columns, |ui, user, col_index| {
                let text = match col_index {
                    0 => &user.name,
                    1 => &user.title,
                    2 => &user.email,
                    _ => return,
                };
                TableRowBuilder::text(ui, text, col_index == 0);
            });
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/test_table_column_toggle_menu/column_toggle_menu");
}

#[test]
fn test_table_hidden_column() {
    let users = test_users();
    let mut hidden_columns: HashSet<String> = HashSet::new();
    hidden_columns.insert("title".to_string()); // Hide the Title column

    let mut harness = Harness::new_ui(|ui| {
        Table::new("users-hidden-column")
            .column("name", "Name", |col| col.initial_width(150.0))
            .column("title", "Title", |col| col.initial_width(150.0))
            .column("email", "Email", |col| col.initial_width(200.0))
            .show_with_column_toggle(ui, &users, &hidden_columns, |ui, user, col_index| {
                let text = match col_index {
                    0 => &user.name,
                    1 => &user.title,
                    2 => &user.email,
                    _ => return,
                };
                TableRowBuilder::text(ui, text, col_index == 0);
            });
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/test_table_hidden_column/hidden_column");
}

#[test]
fn configurable_table_preserves_header_and_row_accessibility_targets() {
    let users = test_users();
    let clicked_headers = Rc::new(RefCell::new(Vec::new()));
    let clicked_headers_for_ui = clicked_headers.clone();
    let mut harness = Harness::new_ui(move |ui| {
        Table::new("configurable-accessibility")
            .column("name", "Name", |column| {
                column.sortable().initial_width(150.0)
            })
            .show_configurable_with_row_response(
                ui,
                &users,
                Some(&SortState::new("name", SortDirection::Ascending)),
                |response, id, _, _| {
                    if response.clicked() {
                        clicked_headers_for_ui.borrow_mut().push(id.to_owned());
                    }
                },
                |_, _| {},
                |ui, user, _| TableRowBuilder::text(ui, &user.name, true),
                |_, _, _| {},
            );
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.get_by_label("Name column");
    harness.get_by_label("Resize Name column");
    harness.get_by_label("Name, row 1");

    harness.get_by_label("Name column").click();
    harness.run();
    assert_eq!(*clicked_headers.borrow(), vec!["name"]);
}

#[test]
fn configurable_table_invokes_each_row_callback_for_its_cell() {
    let users = test_users();
    let callback_cells = Rc::new(RefCell::new(Vec::new()));
    let callback_cells_for_ui = callback_cells.clone();
    let mut harness = Harness::new_ui(move |ui| {
        Table::new("configurable-row-callbacks")
            .column("name", "Name", |column| column.initial_width(150.0))
            .column("title", "Title", |column| column.initial_width(150.0))
            .show_configurable_with_row_response(
                ui,
                &users[..2],
                None,
                |_, _, _, _| {},
                |_, _| {},
                |ui, user, column_index| {
                    let text = if column_index == 0 {
                        &user.name
                    } else {
                        &user.title
                    };
                    TableRowBuilder::text(ui, text, column_index == 0);
                },
                |_, user, column_index| {
                    callback_cells_for_ui
                        .borrow_mut()
                        .push((user.id, column_index));
                },
            );
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    let callback_cells = callback_cells.borrow();
    let expected_cells = [(1, 0), (1, 1), (2, 0), (2, 1)];
    assert!(!callback_cells.is_empty());
    assert_eq!(callback_cells.len() % expected_cells.len(), 0);
    assert!(
        callback_cells
            .chunks_exact(expected_cells.len())
            .all(|cells| cells == expected_cells)
    );
}

#[test]
fn selectable_configurable_table_only_selects_rows_with_keys() {
    let users = test_users();
    let selected = Rc::new(RefCell::new(HashSet::from([1, 999])));
    let selected_for_ui = selected.clone();
    let clicked_headers = Rc::new(RefCell::new(Vec::new()));
    let clicked_headers_for_ui = clicked_headers.clone();
    let visible_users = &users[..3];
    let mut harness = Harness::new_ui(move |ui| {
        Table::new("selectable-configurable-keys")
            .column("name", "Name", |column| column.initial_width(150.0))
            .selectable()
            .show_selectable_configurable_with_row_response(
                ui,
                visible_users,
                &mut selected_for_ui.borrow_mut(),
                |user| (user.id <= 2).then_some(&user.id),
                None,
                |response, id, _, _| {
                    if response.clicked() {
                        clicked_headers_for_ui.borrow_mut().push(id.to_owned());
                    }
                },
                |_, _| {},
                |ui, user, _| TableRowBuilder::text(ui, &user.name, true),
                |_, _, _| {},
            );
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.get_by_label("Select all rows");
    harness.get_by_label("Select row 1");

    harness.get_by_label("Select all rows").click();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([1, 2, 999]));
    assert!(clicked_headers.borrow().is_empty());

    let selection_header = harness.get_by_label("Selection column").rect();
    let header_edge = egui::pos2(selection_header.left() + 4.0, selection_header.center().y);
    primary_click(&mut harness, header_edge);
    harness.run();
    assert_eq!(*clicked_headers.borrow(), vec!["selection"]);

    harness.get_by_label("Select all rows").click();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([999]));
}

#[test]
fn selectable_configurable_table_ignores_a_press_that_started_outside_its_checkbox() {
    let users = test_users();
    let selected = Rc::new(RefCell::new(HashSet::new()));
    let selected_for_ui = selected.clone();
    let mut harness = Harness::new_ui(move |ui| {
        Table::new("selectable-configurable-press-origin")
            .column("name", "Name", |column| column.initial_width(150.0))
            .selectable()
            .show_selectable_configurable_with_row_response(
                ui,
                &users[..1],
                &mut selected_for_ui.borrow_mut(),
                |user| Some(&user.id),
                None,
                |_, _, _, _| {},
                |_, _| {},
                |ui, user, _| TableRowBuilder::text(ui, &user.name, true),
                |_, _, _| {},
            );
    });

    crate::test_support::setup_egui(&mut harness);
    harness.run();
    let selection_header = harness.get_by_label("Selection column").rect();
    let checkbox = harness.get_by_label("Select all rows").rect();

    primary_press(
        &mut harness,
        egui::pos2(selection_header.left() + 4.0, selection_header.center().y),
    );
    primary_release(&mut harness, checkbox.center());
    harness.run();
    assert!(selected.borrow().is_empty());

    primary_press(&mut harness, checkbox.center());
    primary_release(
        &mut harness,
        egui::pos2(selection_header.left() + 4.0, selection_header.center().y),
    );
    primary_press(
        &mut harness,
        egui::pos2(selection_header.left() + 4.0, selection_header.center().y),
    );
    primary_release(&mut harness, checkbox.center());
    harness.run();
    assert!(selected.borrow().is_empty());

    harness.get_by_label("Select all rows").click();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([1]));
}

/// A table of `users` with a selectable column and a name column, sharing
/// `selected` with the test.
fn selectable_users_harness(selected: Rc<RefCell<HashSet<u32>>>) -> Harness<'static> {
    let users = test_users();
    let mut harness = Harness::new_ui(move |ui| {
        Table::new("users-range")
            .column("name", "Name", |col| col.initial_width(200.0))
            .selectable()
            .show_selectable(
                ui,
                &users,
                &mut selected.borrow_mut(),
                |user| user.id,
                |ui, user, _| TableRowBuilder::text(ui, &user.name, true),
            );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness
}

/// Click at `position` with Shift held, then release Shift.
fn shift_click(harness: &mut Harness<'_>, position: egui::Pos2) {
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    primary_click(harness, position);
    harness.run();
    harness.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    harness.run();
}

#[test]
fn shift_clicking_a_checkbox_selects_the_range_from_the_last_clicked_row() {
    let selected = Rc::new(RefCell::new(HashSet::new()));
    let mut harness = selectable_users_harness(selected.clone());
    harness.get_by_label("Select row 2").click();
    harness.run();

    let fifth = harness.get_by_label("Select row 5").rect().center();
    shift_click(&mut harness, fifth);
    assert_eq!(*selected.borrow(), HashSet::from([2, 3, 4, 5]));

    let third = harness.get_by_label("Select row 3").rect().center();
    shift_click(&mut harness, third);
    assert_eq!(
        *selected.borrow(),
        HashSet::from([2]),
        "shift-clicking a selected row deselects the range"
    );
}

#[test]
fn clicking_a_sortable_header_cycles_ascending_descending_and_default() {
    assert_eq!(
        SortState::after_click(None, "name").map(|state| state.direction),
        Some(SortDirection::Ascending)
    );
    let ascending = SortState::new("name", SortDirection::Ascending);
    assert_eq!(
        SortState::after_click(Some(&ascending), "name").map(|state| state.direction),
        Some(SortDirection::Descending)
    );
    let descending = SortState::new("name", SortDirection::Descending);
    assert!(SortState::after_click(Some(&descending), "name").is_none());
    assert_eq!(
        SortState::after_click(Some(&descending), "title").map(|state| state.column_id),
        Some("title".to_owned())
    );

    let users = test_users();
    let sort = Rc::new(RefCell::new(None::<SortState>));
    let sort_in_ui = sort.clone();
    let mut harness = Harness::new_ui(move |ui| {
        Table::new("users-cycle")
            .column("name", "Name", |col| col.sortable().initial_width(150.0))
            .show_sortable(ui, &users, &mut sort_in_ui.borrow_mut(), |ui, user, _| {
                TableRowBuilder::text(ui, &user.name, true);
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.get_by_label("Sort by Name").click();
    harness.run();
    harness
        .get_by_label("Sort by Name; currently ascending")
        .click();
    harness.run();
    harness
        .get_by_label("Sort by Name; currently descending")
        .click();
    harness.run();
    assert!(sort.borrow().is_none());
}

#[test]
fn rows_show_hover_selection_and_inspection_as_tile_fills() {
    let users = test_users();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(520.0, 300.0))
        .build_ui(move |ui| {
            ui.set_max_width(520.0);
            let mut selected = HashSet::from([2, 3]);
            Table::new("users-row-states")
                .column("name", "Name", |col| col.sortable().fill_remaining())
                .column("restarts", "Restarts", |col| {
                    col.numeric().initial_width(88.0)
                })
                .column("cpu", "CPU", |col| col.numeric().initial_width(80.0))
                .column("actions", "", |col| {
                    col.align(CellAlign::Center).initial_width(48.0)
                })
                .selectable()
                .inspected_row(Some(4))
                .force_row_hovered(1)
                .force_header_state("restarts", HeaderState::Hovered)
                .show_selectable(
                    ui,
                    &users,
                    &mut selected,
                    |user| user.id,
                    |ui, user, column| match column {
                        0 => TableRowBuilder::text(ui, &user.name, true),
                        1 => TableRowBuilder::number(ui, &user.id.to_string()),
                        2 if user.id % 2 == 0 => TableRowBuilder::number(ui, "12m"),
                        2 => TableRowBuilder::missing(ui, "Metrics server not available"),
                        _ => {
                            crate::MoreButton::new(format!("More actions for {}", user.name))
                                .show(ui, |_| {});
                        }
                    },
                );
        });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    let name = harness.get_by_label("Lindsay Walton").rect();
    let number = harness.get_by_label("1").rect();
    let actions = harness
        .get_by_label("More actions for Lindsay Walton")
        .rect();
    assert_eq!(name.center().y, number.center().y);
    assert_eq!(name.center().y, actions.center().y);
    harness.ui_harness("tables/rows_show_hover_selection_and_inspection_as_tile_fills");

    let missing = |harness: &Harness<'_>| {
        harness
            .get_all_by_label("Metrics server not available")
            .count()
    };
    let cells = missing(&harness);
    harness
        .get_all_by_label("Metrics server not available")
        .next()
        .expect("a missing value")
        .hover();
    harness.run();
    assert_eq!(
        missing(&harness),
        cells + 1,
        "the em dash explains itself in a tooltip"
    );
}

#[test]
fn bulk_selection_bar_counts_the_selection_and_clears_it() {
    let cleared = Rc::new(RefCell::new(0));
    let cleared_in_ui = cleared.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(720.0, 60.0))
        .build_ui(move |ui| {
            ui.add_space(8.0);
            ui.set_max_width(720.0);
            let response = BulkSelectionBar::new(2, 6).show(ui, |ui| {
                crate::Button::danger_quiet("Delete")
                    .leading_icon(crate::icons::Icon::Trash)
                    .size(crate::ButtonSize::Sm)
                    .show(ui);
            });
            if response.clear {
                *cleared_in_ui.borrow_mut() += 1;
            }
        });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/bulk_selection_bar_counts_the_selection_and_clears_it");

    harness.get_by_label("Clear selection").click();
    harness.run();
    harness.get_by_label("Selected rows").click();
    harness.run();
    assert_eq!(*cleared.borrow(), 2);
}

#[test]
fn column_visibility_menu_toggles_columns_and_resets() {
    let hidden = Rc::new(RefCell::new(HashSet::from(["owner".to_owned()])));
    let reset = Rc::new(RefCell::new(false));
    let (hidden_in_ui, reset_in_ui) = (hidden.clone(), reset.clone());
    let mut harness = Harness::new_ui(move |ui| {
        crate::MoreButton::new("Columns").show(ui, |menu| {
            let response = column_visibility_menu(
                menu,
                &[("name", "Name"), ("owner", "Owner"), ("age", "Age")],
                &mut hidden_in_ui.borrow_mut(),
            );
            *reset_in_ui.borrow_mut() |= response.reset;
        });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.get_by_label("Columns").click();
    harness.run();
    harness.get_by_label("Owner").click();
    harness.run();
    harness.get_by_label("Age").click();
    harness.run();
    assert_eq!(*hidden.borrow(), HashSet::from(["age".to_owned()]));

    harness.get_by_label("Reset to default").click();
    harness.run();
    assert!(*reset.borrow());
}

#[test]
fn loading_tables_show_skeleton_rows_under_their_header() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(420.0, 140.0))
        .build_ui(|ui| {
            ui.set_max_width(420.0);
            Table::new("loading")
                .column("name", "Name", |col| {
                    col.initial_width(200.0).skeleton_width(150.0)
                })
                .column("status", "Status", |col| col.initial_width(112.0))
                .column("age", "Age", |col| col.numeric().initial_width(80.0))
                .show_loading(ui, 3, "Loading pods");
        });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("tables/loading_tables_show_skeleton_rows_under_their_header");
}

#[test]
fn resizing_a_column_stops_at_the_minimum_width() {
    let users = test_users();
    let width = Rc::new(RefCell::new(150.0_f32));
    let width_in_ui = width.clone();
    let mut harness = Harness::new_ui(move |ui| {
        let current = *width_in_ui.borrow();
        Table::new("users-resize")
            .column("name", "Name", |col| col.initial_width(current))
            .column("title", "Title", |col| col.initial_width(150.0))
            .show_configurable_with_row_response(
                ui,
                &users,
                None,
                |_, _, _, _| {},
                |_, new_width| *width_in_ui.borrow_mut() = new_width,
                |ui, user, _| TableRowBuilder::text(ui, &user.name, true),
                |_, _, _| {},
            );
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    let handle = harness.get_by_label("Resize Name column").rect().center();
    harness.drag_at(handle);
    harness.run();
    harness.hover_at(handle + egui::vec2(-40.0, 0.0));
    harness.run();
    assert_eq!(*width.borrow(), 110.0);
    harness.hover_at(handle + egui::vec2(-400.0, 0.0));
    harness.run();
    harness.drop_at(handle + egui::vec2(-400.0, 0.0));
    harness.run();
    assert_eq!(*width.borrow(), 48.0);
}

#[test]
fn numbers_use_tabular_figures_only_in_tabular_columns() {
    let mut harness = Harness::new_ui(|ui| {
        Table::new("figures")
            .column("proportional", "Proportional", |col| {
                col.numeric().initial_width(120.0)
            })
            .column("tabular", "Tabular", |col| {
                col.numeric().tabular().initial_width(120.0)
            })
            .show(ui, &["111"], |ui, value, _| {
                TableRowBuilder::number(ui, value)
            });
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    let widths: Vec<f32> = harness
        .get_all_by_label("111")
        .map(|node| node.rect().width())
        .collect();
    assert_eq!(widths.len(), 2);
    assert!(
        widths[1] > widths[0],
        "tabular ones are wider than proportional ones: {widths:?}"
    );
}

#[test]
fn configurable_tables_report_a_clicked_sortable_header() {
    let users = test_users();
    let sorted_by = Rc::new(RefCell::new(Vec::new()));
    let sorted_by_ui = sorted_by.clone();
    let mut harness = Harness::new_ui(move |ui| {
        let clicked = Table::new("configurable-sort")
            .column("name", "Name", |column| {
                column.sortable().initial_width(150.0)
            })
            .column("title", "Title", |column| column.initial_width(150.0))
            .show_configurable_with_row_response(
                ui,
                &users,
                None,
                |_, _, _, _| {},
                |_, _| {},
                |ui, user, column| match column {
                    0 => TableRowBuilder::text(ui, &user.name, true),
                    _ => TableRowBuilder::text(ui, &user.title, false),
                },
                |_, _, _| {},
            );
        sorted_by_ui.borrow_mut().extend(clicked);
    });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    harness.get_by_label("Name column").click();
    harness.run();
    harness.get_by_label("Title column").click();
    harness.run();
    assert_eq!(
        *sorted_by.borrow(),
        ["name"],
        "only the sortable column asks to sort"
    );
}

#[test]
fn selectable_loading_rows_keep_the_checkbox_column_and_skip_unlabelled_columns() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(420.0, 140.0))
        .build_ui(|ui| {
            ui.set_max_width(420.0);
            Table::new("loading-selectable")
                .column("name", "Name", |col| col.fill_remaining())
                .column("actions", "", |col| {
                    col.initial_width(crate::design::size::TABLE_ACTIONS_COLUMN)
                        .align(CellAlign::Center)
                })
                .selectable()
                .show_loading(ui, 3, "Loading pods");
        });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    let loading = harness.get_by_label("Loading pods").rect();
    let name = harness
        .get_by_role_and_label(egui::accesskit::Role::ColumnHeader, "Name")
        .rect();
    assert_eq!(
        name.left() - loading.left(),
        crate::design::size::TABLE_CHECKBOX_COLUMN,
        "the Name column starts after the empty checkbox column"
    );
    harness.ui_harness(
        "tables/selectable_loading_rows_keep_the_checkbox_column_and_skip_unlabelled_columns",
    );
}

#[test]
fn container_squares_describe_each_container_in_a_key_value_tooltip() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(360.0, 160.0))
        .build_ui(|ui| {
            ui.add_space(100.0);
            ui.horizontal(|ui| {
                ui.add_space(150.0);
                TableRowBuilder::containers(
                    ui,
                    [
                        (
                            crate::design::theme::Tone::Success,
                            "Container: api",
                            crate::tooltip::TooltipContent::KeyValues(&[
                                ("Container", "api"),
                                ("State", "Running"),
                            ]),
                        ),
                        (
                            crate::design::theme::Tone::Warning,
                            "Container: sidecar",
                            crate::tooltip::TooltipContent::KeyValues(&[
                                ("Container", "sidecar"),
                                ("State", "Waiting · ContainerCreating"),
                            ]),
                        ),
                    ],
                );
            });
        });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    assert!(
        harness
            .query_by_label(" Waiting · ContainerCreating")
            .is_none()
    );

    harness.get_by_label("Container: sidecar").hover();
    harness.run();
    harness.get_by_label(" Waiting · ContainerCreating");
    assert!(
        harness.query_by_label(" Running").is_none(),
        "only the hovered container is described"
    );
}

// C17
const GROUP_A: [&str; 3] = ["a-0", "a-1", "a-2"];
const GROUP_B: [&str; 2] = ["b-0", "b-1"];

#[test]
fn a_grouped_table_selects_the_rows_it_shows_and_ranges_within_a_group() {
    let selected = Rc::new(RefCell::new(HashSet::<&str>::new()));
    let expanded = Rc::new(std::cell::Cell::new(false));
    let (set, open) = (selected.clone(), expanded.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 400.0))
        .build_ui(move |ui| {
            let _ = Table::new("groups")
                .column("name", "Name", |col| col.initial_width(200.0))
                .selectable()
                .show_grouped(
                    ui,
                    vec![
                        crate::TableGroup::new(crate::GroupHeader::new("a", 3), &GROUP_A[..]),
                        crate::TableGroup::new(crate::GroupHeader::new("b", 2), &GROUP_B[..])
                            .expanded(open.get()),
                    ],
                    Some(&mut set.borrow_mut()),
                    |name: &&str| Some(*name),
                    None,
                    |_| None,
                    |ui, name, _| TableRowBuilder::text(ui, name, true),
                );
        });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    harness.get_by_label("Select all rows").click();
    harness.run();
    assert_eq!(
        *selected.borrow(),
        HashSet::from(GROUP_A),
        "the rows of a collapsed group are not selected"
    );

    selected.borrow_mut().clear();
    expanded.set(true);
    harness.run();
    harness.get_by_label("Select row 3").click();
    harness.run();
    let b_1 = harness.get_by_label("Select row 5").rect().center();
    shift_click(&mut harness, b_1);
    assert_eq!(
        *selected.borrow(),
        HashSet::from(["a-2", "b-1"]),
        "a range does not reach from one group into another"
    );
    let b_0 = harness.get_by_label("Select row 4").rect().center();
    shift_click(&mut harness, b_0);
    assert_eq!(*selected.borrow(), HashSet::from(["a-2", "b-0", "b-1"]));
}

// WP3b
mod grouped_configurable {
    use super::*;
    use crate::design::size;
    use crate::design::theme::Tone;
    use crate::{GroupHeader, GroupedTableResponse, TableGroup};

    type Row = (&'static str, Option<Tone>);

    const ROWS: [Row; 4] = [
        ("healthy", None),
        ("pending", Some(Tone::Warning)),
        ("steady", None),
        ("failed", Some(Tone::Danger)),
    ];

    fn table() -> Table {
        Table::new("grouped-configurable")
            .column("name", "Name", |col| col.sortable().initial_width(200.0))
            .column("status", "Status", |col| col.initial_width(100.0))
            .selectable()
            .fill_available_height()
    }

    /// The grouped table of `groups` of rows, reporting each frame's response.
    /// It is not run: a table told to scroll to a row keeps asking for
    /// another frame.
    fn harness(
        groups: impl Fn() -> Vec<Vec<Row>> + 'static,
        grouped: bool,
        height: f32,
        scroll_to: Option<usize>,
        responses: Rc<RefCell<Vec<GroupedTableResponse>>>,
    ) -> Harness<'static> {
        let groups = Rc::new(groups());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(400.0, height))
            .build_ui(move |ui| {
                ui.set_max_height(height);
                let table_groups = groups
                    .iter()
                    .enumerate()
                    .map(|(index, rows)| {
                        if grouped {
                            let name: &'static str = ["first", "second"][index];
                            TableGroup::new(GroupHeader::new(name, rows.len()), &rows[..])
                        } else {
                            TableGroup::ungrouped(&rows[..])
                        }
                    })
                    .collect();
                let mut selected = HashSet::<&str>::new();
                let response = table().scroll_to_row(scroll_to).show_grouped_configurable(
                    ui,
                    table_groups,
                    Some(&mut selected),
                    |row: &Row| Some(row.0),
                    None,
                    |row| row.1,
                    |_, _, _, _| {},
                    |_, _| {},
                    |ui, row, column| match column {
                        0 => TableRowBuilder::text(ui, row.0, true),
                        _ => TableRowBuilder::text(ui, "status", false),
                    },
                    |_, _, _| {},
                );
                responses.borrow_mut().push(response);
            });
        crate::test_support::setup_egui(&mut harness);
        harness
    }

    #[test]
    fn it_reports_a_clicked_sort_and_ticks_against_the_viewport_when_the_list_is_short() {
        let responses = Rc::new(RefCell::new(Vec::new()));
        let mut harness = harness(|| vec![ROWS.to_vec()], true, 400.0, None, responses.clone());
        harness.run();
        harness.get_by_label("Name column").click();
        harness.run();

        let responses = responses.borrow();
        assert!(
            responses
                .iter()
                .any(|response| response.sort_clicked.as_deref() == Some("name"))
        );
        let last = responses.last().expect("a frame");
        assert_eq!(last.ticks.len(), 2, "one tick per problem row");
        assert_eq!(last.ticks[0].tone, Tone::Warning);
        assert_eq!(last.ticks[1].tone, Tone::Danger);
        // The viewport is taller than the 34 + 4 × 40 point list, so a tick
        // sits at its row's height over the viewport, not over the list.
        let (first, second) = (
            size::GROUP_HEADER + size::ROW_PITCH,
            size::GROUP_HEADER + 3.0 * size::ROW_PITCH,
        );
        assert!(last.ticks[0].position < first / last.content_height);
        assert!((last.ticks[1].position / last.ticks[0].position - second / first).abs() < 0.01);
        assert_eq!(last.visible.0, 0.0);
        assert!(
            (last.visible.1 - 1.0).abs() < f32::EPSILON,
            "the thumb fills the track"
        );
    }

    #[test]
    fn ungrouped_rows_have_no_header_row_and_still_tick() {
        let responses = Rc::new(RefCell::new(Vec::new()));
        let mut harness = harness(
            || vec![ROWS.to_vec()],
            false,
            400.0,
            None,
            responses.clone(),
        );
        harness.run();
        assert!(harness.query_by_label_contains("Collapse").is_none());
        assert!(harness.query_by_label_contains("Expand").is_none());
        let responses = responses.borrow();
        let last = responses.last().expect("a frame");
        assert_eq!(last.ticks.len(), 2);
        assert!(
            (last.content_height - 4.0 * size::ROW_PITCH).abs() < f32::EPSILON,
            "no header rows in the height"
        );
        assert!(
            (last.ticks[1].position / last.ticks[0].position - 3.0).abs() < 0.01,
            "ticks sit at the rows 1 and 3 of the list"
        );
    }

    #[test]
    fn it_scrolls_the_requested_row_into_view_across_groups() {
        let many = |name: &'static str| vec![(name, None); 20];
        let responses = Rc::new(RefCell::new(Vec::new()));
        let mut harness = harness(
            move || vec![many("first-row"), many("second-row")],
            true,
            240.0,
            Some(30),
            responses,
        );
        harness.run_steps(4);
        // Row 30 counts across groups: the eleventh row of the second group.
        let row = harness.get_by_label("Select row 31").rect();
        assert!(
            row.top() >= 0.0 && row.bottom() <= 240.0,
            "the row is within the window: {row:?}"
        );
        assert!(
            harness.query_by_label("Select row 1").is_none(),
            "the top of the list scrolled away"
        );
    }
}

#[test]
fn usage_cells_show_their_figures_and_a_bar_that_warns_as_it_fills() {
    let usage = [
        ("0.82 / 4", Some(0.21)),
        ("3.1 / 4", Some(0.78)),
        ("3.9 / 4", Some(0.96)),
        ("15m", None),
    ];
    let mut harness = Harness::builder()
        .with_size(egui::vec2(420.0, 200.0))
        .build_ui(|ui| {
            ui.set_max_width(420.0);
            Table::new("usage")
                .column("name", "Name", |col| col.initial_width(120.0))
                .column("cpu", "CPU", |col| col.numeric().initial_width(140.0))
                .show(ui, &usage, |ui, (figures, fraction), column| match column {
                    0 => TableRowBuilder::text(ui, "node", true),
                    _ => TableRowBuilder::usage(ui, figures, *fraction),
                });
        });
    crate::test_support::setup_egui(&mut harness);
    harness.run();
    harness.get_by_label("21% in use");
    harness.get_by_label("78% in use");
    harness.get_by_label("96% in use");
    assert_eq!(
        harness.query_all_by_label_contains("% in use").count(),
        3,
        "a cell without a fraction has no bar"
    );
    harness.ui_harness("tables/usage_cells_show_their_figures_and_a_bar_that_warns_as_it_fills");
}

#[test]
fn a_shift_click_range_follows_the_anchor_row_when_the_groups_reorder() {
    let selected = Rc::new(RefCell::new(HashSet::<&str>::new()));
    let swapped = Rc::new(std::cell::Cell::new(false));
    let (set, swap) = (selected.clone(), swapped.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 400.0))
        .build_ui(move |ui| {
            let a = crate::TableGroup::new(crate::GroupHeader::new("a", 3), &GROUP_A[..]);
            let b = crate::TableGroup::new(crate::GroupHeader::new("b", 2), &GROUP_B[..]);
            let groups = if swap.get() { vec![b, a] } else { vec![a, b] };
            let _ = Table::new("anchors")
                .column("name", "Name", |col| col.initial_width(200.0))
                .selectable()
                .show_grouped(
                    ui,
                    groups,
                    Some(&mut set.borrow_mut()),
                    |name: &&str| Some(*name),
                    None,
                    |_| None,
                    |ui, name, _| TableRowBuilder::text(ui, name, true),
                );
        });
    crate::test_support::setup_egui(&mut harness);
    harness.run();

    harness.get_by_label("Select row 2").click();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from(["a-1"]));

    // The groups trade places: "a-1" is now the fourth row.
    selected.borrow_mut().clear();
    swapped.set(true);
    harness.run();
    let a_2 = harness.get_by_label("Select row 5").rect().center();
    shift_click(&mut harness, a_2);
    assert_eq!(
        *selected.borrow(),
        HashSet::from(["a-1", "a-2"]),
        "the range runs from the anchor row, wherever its group went"
    );
}

#[test]
fn cells_share_one_style_that_follows_their_table_and_theme_scope() {
    use crate::design::theme::{self, ThemeExt, ThemeScope};
    use std::sync::Arc;

    /// What a cell saw: its style, and the scope's field background.
    type Seen = Vec<(Arc<egui::Style>, egui::Color32)>;
    let root_cells = Rc::new(RefCell::new(Seen::new()));
    let scoped_cells = Rc::new(RefCell::new(Seen::new()));
    let users = test_users();
    let (root, scoped) = (Rc::clone(&root_cells), Rc::clone(&scoped_cells));
    let mut harness = Harness::new_ui(move |ui| {
        root.borrow_mut().clear();
        scoped.borrow_mut().clear();
        let table = |ui: &mut egui::Ui, id: &str, seen: &Rc<RefCell<Seen>>| {
            Table::new(id)
                .column("name", "Name", |col| col.initial_width(150.0))
                .column("role", "Role", |col| col.initial_width(100.0))
                .show(ui, &users, |ui, user, col_index| {
                    seen.borrow_mut()
                        .push((Arc::clone(ui.style()), ui.tokens().components.field_bg));
                    TableRowBuilder::text(
                        ui,
                        if col_index == 0 {
                            &user.name
                        } else {
                            &user.role
                        },
                        false,
                    );
                });
        };
        ui.allocate_ui(egui::vec2(400.0, 160.0), |ui| {
            table(ui, "root-table", &root)
        });
        theme::scope(ui, ThemeScope::Popover, |ui| {
            ui.allocate_ui(egui::vec2(400.0, 160.0), |ui| {
                table(ui, "popover-table", &scoped)
            });
        });
    });
    crate::test_support::install_distinct_role_theme(&harness.ctx);
    harness.run();

    let theme = harness.ctx.tokens();
    let popover = theme.scoped(ThemeScope::Popover);
    assert_ne!(theme.components.field_bg, popover.components.field_bg);
    for (cells, field_bg) in [
        (root_cells.borrow(), theme.components.field_bg),
        (scoped_cells.borrow(), popover.components.field_bg),
    ] {
        assert!(cells.len() >= 2, "every cell of the table was laid out");
        let (first, _) = &cells[0];
        for (style, cell_field_bg) in cells.iter() {
            assert!(
                Arc::ptr_eq(style, first),
                "the cells of a table share one style"
            );
            assert_eq!(
                style.spacing.item_spacing,
                egui::vec2(layout::CELL_GAP, 0.0)
            );
            assert_eq!(*cell_field_bg, field_bg);
            // Stock text edits in a cell use the scope's field background.
            assert_eq!(style.visuals.extreme_bg_color, field_bg);
        }
    }
}
