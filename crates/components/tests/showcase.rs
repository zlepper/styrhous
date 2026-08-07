//! Visual catalogue snapshots for the public component API.
//!
//! These pages are intentionally broader than the component-level regression
//! tests: each one provides a quick reference for the component's supported
//! visual states while also protecting the composed presentation from regressions.

use std::collections::HashSet;

use components::colors::WHITE;
use components::icons::{calendar_icon, document_icon, folder_icon, home_icon, users_icon};
use components::{
    ButtonRounding, ButtonSize, ButtonVariant, SortDirection, SortState, TableRowBuilder, Tabs,
    TailwindButton, TailwindCombobox, TailwindTable, WideSidebar,
};
use egui_kittest::{Harness, kittest::Queryable};

struct Person {
    name: &'static str,
    role: &'static str,
    active: bool,
}

const PEOPLE: [Person; 4] = [
    Person {
        name: "Michael Foster",
        role: "Designer",
        active: true,
    },
    Person {
        name: "Emily Selman",
        role: "Engineer",
        active: false,
    },
    Person {
        name: "Floyd Miles",
        role: "Product",
        active: false,
    },
    Person {
        name: "Courtney Henry",
        role: "Admin",
        active: true,
    },
];

fn showcase_title(ui: &mut egui::Ui, title: &str) {
    ui.painter().rect_filled(ui.max_rect(), 0.0, WHITE);
    ui.heading(title);
    ui.add_space(12.0);
}

fn apply_reference_theme(harness: &mut Harness<'_>) {
    components::test_support::setup_egui(&harness.ctx);
}

fn show_people_table(ui: &mut egui::Ui, people: &[Person]) {
    TailwindTable::new(ui.id().with("people-table"))
        .column("name", "Name", |column| column.initial_width(150.0))
        .column("role", "Role", |column| column.initial_width(100.0))
        .show(ui, people, |ui, person, column_index| {
            let text = match column_index {
                0 => person.name,
                1 => person.role,
                _ => return,
            };
            TableRowBuilder::text(ui, text, column_index == 0);
        });
}

#[test]
fn showcase_buttons() {
    let mut harness = Harness::new_ui(|ui| {
        showcase_title(ui, "Buttons");

        for (label, variant) in [
            ("Primary", ButtonVariant::Primary),
            ("Secondary", ButtonVariant::Secondary),
            ("Soft", ButtonVariant::Soft),
        ] {
            ui.label(egui::RichText::new(label).strong());
            ui.horizontal(|ui| {
                for size in [
                    ButtonSize::Xs,
                    ButtonSize::Sm,
                    ButtonSize::Md,
                    ButtonSize::Lg,
                    ButtonSize::Xl,
                ] {
                    TailwindButton::new("Button text")
                        .variant(variant)
                        .size(size)
                        .show(ui);
                }
            });
            ui.add_space(8.0);
        }

        ui.label(egui::RichText::new("Rounding").strong());
        ui.horizontal(|ui| {
            TailwindButton::primary("Default")
                .rounded(ButtonRounding::Default)
                .show(ui);
            TailwindButton::primary("Rounded")
                .rounded(ButtonRounding::Rounded)
                .show(ui);
            TailwindButton::primary("Pill")
                .rounded(ButtonRounding::Pill)
                .show(ui);
        });
    });

    apply_reference_theme(&mut harness);
    harness.run();
    harness.snapshot("showcase/buttons");
}

#[test]
fn showcase_comboboxes() {
    let mut harness = Harness::new_ui(|ui| {
        showcase_title(ui, "Comboboxes");

        TailwindCombobox::from_label("Empty selection")
            .placeholder("Search people...")
            .width(280.0)
            .filter_by(|person: &Person| person.name)
            .show_items(ui, &PEOPLE, |combobox, person| {
                combobox.item(person.name, false);
            });

        ui.add_space(12.0);

        TailwindCombobox::from_label("Selected value")
            .selected_text("Michael Foster")
            .selected_status(Some(true))
            .width(280.0)
            .filter_by(|person: &Person| person.name)
            .show_items(ui, &PEOPLE, |combobox, person| {
                combobox.item_with_status(
                    person.name,
                    person.name == "Michael Foster",
                    Some(person.active),
                );
            });

        ui.add_space(12.0);

        TailwindCombobox::from_label("Multi-select")
            .selected_text("2 people selected")
            .width(280.0)
            .select_all(false)
            .filter_by(|person: &Person| person.name)
            .show_items(ui, &PEOPLE, |combobox, person| {
                combobox.item_with_status(
                    person.name,
                    matches!(person.name, "Michael Foster" | "Emily Selman"),
                    Some(person.active),
                );
            });

        ui.add_space(12.0);

        TailwindCombobox::from_label("Filter results")
            .placeholder("Type to filter...")
            .width(280.0)
            .filter_by(|person: &Person| person.name)
            .show_items(ui, &PEOPLE, |combobox, person| {
                combobox.item(person.name, person.name == "Emily Selman");
            });
    });

    apply_reference_theme(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::ComboBox, "Multi-select")
        .click();
    harness.run();
    harness.snapshot("showcase/comboboxes");
}

#[test]
fn showcase_sidebars() {
    let mut harness = Harness::new_ui(|ui| {
        showcase_title(ui, "Sidebars");
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Narrow").strong());
                components::NarrowSidebar::new().show(ui, |sidebar| {
                    sidebar.item("Dashboard", home_icon(), true);
                    sidebar.item("People", users_icon(), false);
                    sidebar.separator();
                    sidebar.avatar_item("Tailwind Labs", "T", false);
                    sidebar.avatar_item("Acme", "A", false);
                });
            });

            ui.add_space(24.0);

            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Wide").strong());
                WideSidebar::new().width(280.0).show(ui, |sidebar| {
                    sidebar.item("Dashboard", home_icon(), true);
                    sidebar.item("Calendar", calendar_icon(), false);
                    sidebar.section_header("Workspace");
                    sidebar.expandable("Projects", folder_icon(), true, |sidebar| {
                        sidebar.child_item("Platform", false);
                        sidebar.child_item("Kubernetes UI", true);
                    });
                    sidebar.item("Documents", document_icon(), false);
                });
            });
        });
    });

    apply_reference_theme(&mut harness);
    harness.run();
    harness.snapshot("showcase/sidebars");
}

#[test]
fn showcase_tables() {
    let mut harness = Harness::new_ui(|ui| {
        showcase_title(ui, "Tables");
        ui.columns(2, |columns| {
            columns[0].label(egui::RichText::new("Standard").strong());
            show_people_table(&mut columns[0], &PEOPLE[..2]);
            columns[0].add_space(20.0);
            columns[0].label(egui::RichText::new("Selectable").strong());
            let selected = HashSet::from(["Michael Foster"]);
            TailwindTable::new("showcase-selectable")
                .column("name", "Name", |column| column.initial_width(150.0))
                .column("role", "Role", |column| column.initial_width(100.0))
                .selectable()
                .show_selectable(
                    &mut columns[0],
                    &PEOPLE[..2],
                    &selected,
                    |person| person.name,
                    |ui, person, column_index| {
                        let text = match column_index {
                            0 => person.name,
                            1 => person.role,
                            _ => return,
                        };
                        TableRowBuilder::text(ui, text, column_index == 0);
                    },
                );

            columns[1].label(egui::RichText::new("Sorted").strong());
            let mut sort = Some(SortState::new("name", SortDirection::Ascending));
            TailwindTable::new("showcase-sorted")
                .column("name", "Name", |column| {
                    column.sortable().initial_width(150.0)
                })
                .column("role", "Role", |column| column.initial_width(100.0))
                .show_sortable(
                    &mut columns[1],
                    &PEOPLE[..2],
                    &mut sort,
                    |ui, person, column_index| {
                        let text = match column_index {
                            0 => person.name,
                            1 => person.role,
                            _ => return,
                        };
                        TableRowBuilder::text(ui, text, column_index == 0);
                    },
                );
            columns[1].add_space(20.0);
            columns[1].label(egui::RichText::new("Column visibility").strong());
            let hidden_columns = HashSet::from(["role".to_owned()]);
            TailwindTable::new("showcase-column-visibility")
                .column("name", "Name", |column| {
                    column.initial_width(150.0).not_hideable()
                })
                .column("role", "Role", |column| column.initial_width(100.0))
                .show_with_column_toggle(
                    &mut columns[1],
                    &PEOPLE[..2],
                    &hidden_columns,
                    |ui, person, column_index| {
                        let text = match column_index {
                            0 => person.name,
                            1 => person.role,
                            _ => return,
                        };
                        TableRowBuilder::text(ui, text, column_index == 0);
                    },
                );
        });
    });

    apply_reference_theme(&mut harness);
    harness.run();
    harness.snapshot("showcase/tables");
}

#[test]
fn showcase_tabs() {
    let mut harness = Harness::new_ui(|ui| {
        showcase_title(ui, "Tabs");
        ui.label(egui::RichText::new("Text tabs").strong());
        Tabs::new("showcase-text-tabs").show(ui, |tabs| {
            tabs.tab("Overview", None, |ui| {
                ui.label("Overview content");
            });
            tabs.tab("Activity", None, |ui| {
                ui.label("Activity content");
            });
            tabs.tab("Settings", None, |ui| {
                ui.label("Settings content");
            });
        });

        ui.add_space(32.0);
        ui.label(egui::RichText::new("Tabs with icons").strong());
        Tabs::new("showcase-icon-tabs").show(ui, |tabs| {
            tabs.tab("Dashboard", Some(home_icon()), |ui| {
                ui.label("Dashboard content");
            });
            tabs.tab("People", Some(users_icon()), |ui| {
                ui.label("People content");
            });
            tabs.tab("Documents", Some(document_icon()), |ui| {
                ui.label("Documents content");
            });
        });
    });

    apply_reference_theme(&mut harness);
    harness.run();
    harness.snapshot("showcase/tabs");
}
