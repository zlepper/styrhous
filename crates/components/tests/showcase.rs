//! Visual catalogue snapshots for the public component API.
//!
//! These pages are intentionally broader than the component-level regression
//! tests: each one provides a quick reference for the component's supported
//! visual states while also protecting the composed presentation from regressions.

use std::collections::HashSet;

use components::design::theme::ThemeExt;
use components::design::type_scale;
use components::icons::Icon;
use components::test_support::UiHarnessSnapshot;
use components::test_support::board::{BoardHeader, board, cell, column, row};
use components::{
    Button, ButtonSize, ButtonVariant, CheckState, Combobox, SortDirection, SortState, Table,
    TableRowBuilder, Tabs, WideSidebar,
};
use egui::{Align, Layout, Ui, Vec2};
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
    ui.painter()
        .rect_filled(ui.max_rect(), 0.0, ui.tokens().surface.canvas);
    ui.heading(title);
    ui.add_space(12.0);
}

fn apply_reference_theme(harness: &mut Harness<'_>) {
    components::test_support::setup_egui(harness);
}

fn show_people_table(ui: &mut egui::Ui, people: &[Person]) {
    Table::new(ui.id().with("people-table"))
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
    let mut harness = Harness::new_ui(show_buttons_showcase);

    apply_reference_theme(&mut harness);
    harness.run();
    harness.ui_harness("showcase/showcase_buttons/buttons");
}

const BUTTON_VARIANTS: [(&str, ButtonVariant); 7] = [
    ("Primary", ButtonVariant::Primary),
    ("Secondary", ButtonVariant::Secondary),
    ("Soft", ButtonVariant::Soft),
    ("Ghost", ButtonVariant::Ghost),
    ("Danger", ButtonVariant::Danger),
    ("Danger quiet", ButtonVariant::DangerQuiet),
    ("Link", ButtonVariant::Link),
];
const BUTTON_SIZES: [(&str, ButtonSize); 3] = [
    ("sm · 28", ButtonSize::Sm),
    ("md · 32", ButtonSize::Md),
    ("lg · 36", ButtonSize::Lg),
];
const BUTTON_LABEL_COLUMN: f32 = 132.0;
const BUTTON_COLUMN: f32 = 200.0;
const BUTTON_ROW: f32 = 48.0;

/// Every variant at every size, then the icon, loading and disabled forms.
fn show_buttons_showcase(ui: &mut Ui) {
    let header = BoardHeader {
        eyebrow: "Showcase · components::Button",
        title: "Buttons",
        description: "Every variant at every size, with icons, loading and disabled states.",
    };
    board(ui, header, |ui| {
        let theme = ui.tokens();
        let layout = Layout::left_to_right(Align::Center);
        column(ui, 0.0, |ui| {
            row(ui, BUTTON_ROW, 0.0, Align::Center, |ui| {
                cell(
                    ui,
                    Vec2::new(BUTTON_LABEL_COLUMN, BUTTON_ROW),
                    layout,
                    0.0,
                    |_| {},
                );
                for (label, _) in BUTTON_SIZES {
                    cell(
                        ui,
                        Vec2::new(BUTTON_COLUMN, BUTTON_ROW),
                        layout,
                        0.0,
                        |ui| {
                            type_scale::META.label(ui, label, theme.text.muted);
                        },
                    );
                }
                for label in ["Leading icon", "Icon only", "Loading", "Disabled"] {
                    cell(
                        ui,
                        Vec2::new(BUTTON_COLUMN, BUTTON_ROW),
                        layout,
                        0.0,
                        |ui| {
                            type_scale::META.label(ui, label, theme.text.muted);
                        },
                    );
                }
            });
            for (name, variant) in BUTTON_VARIANTS {
                row(ui, BUTTON_ROW, 0.0, Align::Center, |ui| {
                    let demo = |ui: &mut Ui, button: Button| {
                        cell(
                            ui,
                            Vec2::new(BUTTON_COLUMN, BUTTON_ROW),
                            layout,
                            0.0,
                            |ui| {
                                button.show(ui);
                            },
                        );
                    };
                    cell(
                        ui,
                        Vec2::new(BUTTON_LABEL_COLUMN, BUTTON_ROW),
                        layout,
                        0.0,
                        |ui| {
                            type_scale::UI_CONTROL.label(ui, name, theme.text.primary);
                        },
                    );
                    for (size_label, size) in BUTTON_SIZES {
                        demo(
                            ui,
                            Button::new("Button text")
                                .variant(variant)
                                .size(size)
                                .accessibility_label(format!("{name} {size_label}")),
                        );
                    }
                    demo(
                        ui,
                        Button::new("Refresh")
                            .variant(variant)
                            .leading_icon(Icon::ArrowPath)
                            .accessibility_label(format!("{name} with icon")),
                    );
                    demo(
                        ui,
                        Button::icon(Icon::Pencil, format!("{name} icon")).variant(variant),
                    );
                    demo(
                        ui,
                        Button::new("Saving")
                            .variant(variant)
                            .loading(true)
                            .accessibility_label(format!("{name} loading")),
                    );
                    demo(
                        ui,
                        Button::new("Unavailable")
                            .variant(variant)
                            .enabled(false)
                            .accessibility_label(format!("{name} disabled")),
                    );
                });
            }
        });
    });
}

#[test]
fn showcase_comboboxes() {
    let mut harness = Harness::new_ui(|ui| {
        showcase_title(ui, "Comboboxes");

        Combobox::from_label("Empty selection")
            .placeholder("Search people...")
            .width(280.0)
            .filter_by(|person: &Person| person.name)
            .show_items(ui, &PEOPLE, |combobox, person| {
                combobox.item(person.name, false);
            });

        ui.add_space(12.0);

        Combobox::from_label("Selected value")
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

        Combobox::from_label("Multi-select")
            .selected_text("2 people selected")
            .width(280.0)
            .select_all("Select all", CheckState::Off)
            .filter_by(|person: &Person| person.name)
            .show_items(ui, &PEOPLE, |combobox, person| {
                combobox.item_with_status(
                    person.name,
                    matches!(person.name, "Michael Foster" | "Emily Selman"),
                    Some(person.active),
                );
            });

        ui.add_space(12.0);

        Combobox::from_label("Filter results")
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
    harness.ui_harness("showcase/showcase_comboboxes/comboboxes");
}

#[test]
fn showcase_sidebars() {
    let mut harness = Harness::new_ui(|ui| {
        showcase_title(ui, "Sidebars");
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Narrow").strong());
                components::NarrowSidebar::new().show(ui, |sidebar| {
                    sidebar.item("Dashboard", Icon::Home.image(), true);
                    sidebar.item("People", Icon::Users.image(), false);
                    sidebar.separator();
                    sidebar.avatar_item("Tailwind Labs", "T", false);
                    sidebar.avatar_item("Acme", "A", false);
                });
            });

            ui.add_space(24.0);

            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Wide").strong());
                WideSidebar::new().width(280.0).show(ui, |sidebar| {
                    sidebar.item("Dashboard", Icon::Home.image(), true);
                    sidebar.item("Calendar", Icon::Calendar.image(), false);
                    sidebar.section_header("Workspace");
                    sidebar.expandable("Projects", Icon::Folder.image(), true, |sidebar| {
                        sidebar.child_item("Platform", false);
                        sidebar.child_item("Kubernetes UI", true);
                    });
                    sidebar.item("Documents", Icon::Document.image(), false);
                });
            });
        });
    });

    apply_reference_theme(&mut harness);
    harness.run();
    harness.ui_harness("showcase/showcase_sidebars/sidebars");
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
            let mut selected = HashSet::from(["Michael Foster"]);
            Table::new("showcase-selectable")
                .column("name", "Name", |column| column.initial_width(150.0))
                .column("role", "Role", |column| column.initial_width(100.0))
                .selectable()
                .show_selectable(
                    &mut columns[0],
                    &PEOPLE[..2],
                    &mut selected,
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
            Table::new("showcase-sorted")
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
            Table::new("showcase-column-visibility")
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
    harness.ui_harness("showcase/showcase_tables/tables");
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
            tabs.tab("Dashboard", Some(Icon::Home.image()), |ui| {
                ui.label("Dashboard content");
            });
            tabs.tab("People", Some(Icon::Users.image()), |ui| {
                ui.label("People content");
            });
            tabs.tab("Documents", Some(Icon::Document.image()), |ui| {
                ui.label("Documents content");
            });
        });
    });

    apply_reference_theme(&mut harness);
    harness.run();
    harness.ui_harness("showcase/showcase_tabs/tabs");
}
