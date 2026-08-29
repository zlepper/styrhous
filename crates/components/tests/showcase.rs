//! Visual catalogue snapshots for the public component API.
//!
//! These pages are intentionally broader than the component-level regression
//! tests: each one provides a quick reference for the component's supported
//! visual states while also protecting the composed presentation from regressions.

use std::collections::HashSet;

use components::colors::{WHITE, gray};
use components::design::{radius, surface, typography};
use components::icons::{
    arrow_left_icon, arrow_right_icon, calendar_icon, document_icon, ellipsis_horizontal_icon,
    folder_icon, home_icon, pencil_icon, trash_icon, users_icon,
};
use components::test_support::UiHarnessSnapshot;
use components::{
    ButtonRounding, ButtonSize, ButtonVariant, SortDirection, SortState, TableRowBuilder, Tabs,
    TailwindButton, TailwindCombobox, TailwindTable, WideSidebar,
};
use egui::{Align, Color32, Label, Layout, Pos2, Rect, RichText, Ui, UiBuilder, Vec2};
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
    components::test_support::setup_egui(harness);
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
    let mut harness = Harness::new_ui(show_buttons_showcase);

    apply_reference_theme(&mut harness);
    harness.run();
    harness.ui_harness("showcase/showcase_buttons/buttons");
}

const BUTTON_SHOWCASE_CARD: Rect =
    Rect::from_min_max(Pos2::new(96.0, 188.0), Pos2::new(1438.0, 926.0));
/// The generated oracle's matrix rasterizes one pixel right and one pixel up
/// from egui's natural control placement. Keep that adjustment at the shared
/// matrix boundary instead of compensating for every label and button.
const BUTTON_SHOWCASE_CONTENT_OFFSET: Vec2 = Vec2::new(1.0, -1.0);
const BUTTON_SHOWCASE_COLUMNS: [(ButtonSize, &str, f32); 5] = [
    (ButtonSize::Xs, "XS", 380.0),
    (ButtonSize::Sm, "SM", 574.0),
    (ButtonSize::Md, "MD", 786.0),
    (ButtonSize::Lg, "LG", 1011.0),
    (ButtonSize::Xl, "XL", 1250.0),
];

fn show_buttons_showcase(ui: &mut Ui) {
    ui.ctx().layer_painter(ui.layer_id()).rect_filled(
        Rect::from_min_size(Pos2::ZERO, Vec2::new(1536.0, 1024.0)),
        0.0,
        Color32::from_rgb(245, 245, 247),
    );

    showcase_text(
        ui,
        Rect::from_min_size(Pos2::new(696.0, 62.0), Vec2::new(160.0, 52.0)),
        RichText::new("Buttons")
            .font(typography::semibold_or_proportional(ui.ctx(), 40.0))
            .color(gray::_900),
    );
    showcase_text(
        ui,
        Rect::from_min_size(Pos2::new(620.0, 120.0), Vec2::new(320.0, 30.0)),
        RichText::new("Application button styles and sizes")
            .font(egui::FontId::proportional(18.0))
            .color(gray::_600),
    );

    ui.painter().rect(
        BUTTON_SHOWCASE_CARD,
        radius::surface(),
        Color32::from_rgb(253, 253, 253),
        surface::control_border(),
        egui::StrokeKind::Inside,
    );

    for (_, label, center_x) in BUTTON_SHOWCASE_COLUMNS {
        let label_width = match label {
            "XS" => 21.0,
            "SM" => 24.7,
            "MD" => 26.0,
            "LG" => 20.5,
            "XL" => 20.0,
            _ => unreachable!("all showcase column labels are covered"),
        };
        showcase_matrix_text(
            ui,
            Rect::from_min_size(
                Pos2::new(center_x - label_width / 2.0, 231.5),
                Vec2::new(40.0, 24.0),
            ),
            RichText::new(label)
                .font(typography::semibold_or_proportional(ui.ctx(), 16.0))
                .color(gray::_950),
        );
    }

    for (label, variant, center_y) in [
        ("Primary", ButtonVariant::Primary, 322.0),
        ("Secondary", ButtonVariant::Secondary, 424.0),
        ("Soft", ButtonVariant::Soft, 526.0),
        ("Danger", ButtonVariant::Danger, 622.0),
    ] {
        showcase_matrix_text(
            ui,
            Rect::from_center_size(Pos2::new(188.0, center_y), Vec2::new(96.0, 30.0)),
            RichText::new(label)
                .font(typography::semibold_or_proportional(ui.ctx(), 18.0))
                .color(gray::_950),
        );
        for (size, _, center_x) in BUTTON_SHOWCASE_COLUMNS {
            showcase_matrix_button(
                ui,
                button_showcase_rect(Pos2::new(center_x, center_y), size),
                TailwindButton::new("Button text")
                    .variant(variant)
                    .size(size),
            );
        }
    }

    ui.painter()
        .hline(140.0..=1396.0, 698.0, egui::Stroke::new(1.0, gray::_200));
    ui.painter()
        .vline(764.0, 735.0..=885.0, egui::Stroke::new(1.0, gray::_200));

    showcase_text(
        ui,
        Rect::from_min_size(Pos2::new(140.0, 746.0), Vec2::new(160.0, 28.0)),
        RichText::new("Icon buttons")
            .font(typography::semibold_or_proportional(ui.ctx(), 18.0))
            .color(gray::_950),
    );
    for (label, icon, center_x) in [
        ("Back", arrow_left_icon(), 178.0),
        ("Forward", arrow_right_icon(), 296.0),
        ("Edit", pencil_icon(), 411.0),
        ("Delete", trash_icon(), 525.0),
        ("More actions", ellipsis_horizontal_icon(), 630.0),
    ] {
        showcase_button(
            ui,
            Rect::from_center_size(Pos2::new(center_x, 835.0), Vec2::splat(44.0)),
            TailwindButton::icon(icon.fit_to_exact_size(Vec2::splat(16.0)).tint(gray::_800))
                .variant(ButtonVariant::Secondary)
                .size(ButtonSize::Xl)
                .accessibility_label(label),
        );
    }

    showcase_text(
        ui,
        Rect::from_min_size(Pos2::new(818.0, 746.0), Vec2::new(180.0, 28.0)),
        RichText::new("Corner radius")
            .font(typography::semibold_or_proportional(ui.ctx(), 18.0))
            .color(gray::_950),
    );
    for (label, rounding, center_x) in [
        ("Default", ButtonRounding::Default, 886.0),
        ("Rounded", ButtonRounding::Rounded, 1076.0),
        ("Pill", ButtonRounding::Pill, 1261.0),
    ] {
        showcase_button(
            ui,
            button_showcase_rounding_rect(Pos2::new(center_x, 838.0), label),
            TailwindButton::primary(label)
                .size(ButtonSize::Xl)
                .rounded(rounding),
        );
    }
}

fn showcase_text(ui: &mut Ui, rect: Rect, rich_text: RichText) {
    showcase_text_at(ui, rect, rich_text, Vec2::ZERO);
}

fn showcase_matrix_text(ui: &mut Ui, rect: Rect, rich_text: RichText) {
    showcase_text_at(ui, rect, rich_text, BUTTON_SHOWCASE_CONTENT_OFFSET);
}

fn showcase_text_at(ui: &mut Ui, rect: Rect, rich_text: RichText, offset: Vec2) {
    let rect = rect.translate(offset);
    let mut text_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    text_ui.add(Label::new(rich_text).extend().selectable(false));
}

fn showcase_button(ui: &mut Ui, rect: Rect, button: TailwindButton<'_>) {
    showcase_button_at(ui, rect, button, Vec2::ZERO);
}

fn showcase_matrix_button(ui: &mut Ui, rect: Rect, button: TailwindButton<'_>) {
    showcase_button_at(ui, rect, button, BUTTON_SHOWCASE_CONTENT_OFFSET);
}

fn showcase_button_at(ui: &mut Ui, rect: Rect, button: TailwindButton<'_>, offset: Vec2) {
    let rect = rect.translate(offset);
    let mut button_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center).with_main_align(Align::Center)),
    );
    button.show(&mut button_ui);
}

fn button_showcase_rect(center: Pos2, size: ButtonSize) -> Rect {
    let dimensions = match size {
        ButtonSize::Xs => Vec2::new(87.8, 32.0),
        ButtonSize::Sm => Vec2::new(106.8, 34.0),
        ButtonSize::Md => Vec2::new(123.8, 38.0),
        ButtonSize::Lg => Vec2::new(132.8, 40.0),
        ButtonSize::Xl => Vec2::new(157.8, 44.0),
    };
    // Egui snaps the odd-pixel SM and LG controls half a pixel to the right.
    // Offset their slots so the rendered accessibility bounds remain centered
    // under the corresponding column heading.
    let center = if matches!(size, ButtonSize::Sm | ButtonSize::Lg) {
        Pos2::new(center.x - 0.5, center.y)
    } else {
        center
    };
    Rect::from_center_size(center, dimensions)
}

fn button_showcase_rounding_rect(center: Pos2, label: &str) -> Rect {
    let width = match label {
        "Default" => 133.5,
        "Rounded" => 145.1,
        "Pill" => 105.1,
        _ => unreachable!("all showcase rounding labels are covered"),
    };
    Rect::from_center_size(center, Vec2::new(width, 44.0))
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
            TailwindTable::new("showcase-selectable")
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
    harness.ui_harness("showcase/showcase_tabs/tabs");
}
