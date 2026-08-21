use super::*;
use crate::test_support::UiHarnessSnapshot;
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
        TailwindTable::new("users-table")
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
        TailwindTable::new("users-alternating")
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
        TailwindTable::new("users-selection")
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
        TailwindTable::new("users-selection-interactions")
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
    harness.get_by_label("Select row 1").click_accesskit();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([1, 3]));

    harness.get_by_label("Select all rows").click_accesskit();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([1, 2, 3]));

    harness.get_by_label("Select all rows").click_accesskit();
    harness.run();
    assert_eq!(*selected.borrow(), HashSet::from([3]));
}

#[test]
fn test_table_select_all() {
    let users = test_users();
    // All users selected
    let mut selection: HashSet<u32> = users.iter().map(|u| u.id).collect();

    let mut harness = Harness::new_ui(|ui| {
        TailwindTable::new("users-select-all")
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
        TailwindTable::new("users-indeterminate")
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
        TailwindTable::new("users-sortable")
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
        TailwindTable::new("users-sort-name-asc")
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
        TailwindTable::new("users-sort-title-desc")
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
        TailwindTable::new("users-column-toggle")
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
        TailwindTable::new("users-hidden-column")
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
