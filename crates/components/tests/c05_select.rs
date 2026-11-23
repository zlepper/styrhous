//! The C05 "Select & combobox" component board
//! (`docs/design-oracles/harbour/src/C05.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C05`.

use components::design::theme::ThemeExt;
use components::design::type_scale::{self, Weight};
use components::design::{radius, size, space};
use components::icons::Icon;
use components::test_support::board::{
    BoardHeader, DEMO_CAPTION_GAP, board, captioned, cell, column, row, section, snapshot_board,
};
use components::{CheckState, Combobox, ControlState, FieldSize};
use egui::{Align, Layout, Ui, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C05 · components::{Select, Combobox}",
    title: "Select & combobox",
    description: "One trigger, two popover modes: a plain list for short fixed choices and a \
                  searchable list (with optional multi-select) for namespaces and anything over \
                  eight items. The popover is always at least as wide as its trigger.",
};

const COLUMN_GAP: f32 = 48.0;
const CLOSED_COLUMN: f32 = 300.0;
/// Gap between the closed-state demos.
const CLOSED_GAP: f32 = 16.0;
/// Gap between a section's trigger and popover (`.col` gap 12).
const POPOVER_GAP: f32 = 12.0;
/// Gap between the sections of a column.
const SECTION_GAP: f32 = 28.0;
const TRIGGER_WIDTH: f32 = 260.0;
const SMALL_TRIGGER_WIDTH: f32 = 200.0;
/// The tooltip under the truncated trigger (`.tip`, `margin-top: 2px`).
const TOOLTIP_OFFSET: f32 = 2.0;
const TOOLTIP_MAX_WIDTH: f32 = 280.0;
/// The note under the single-select popover.
const NOTE_WIDTH: f32 = 220.0;
const NOTE_OFFSET: f32 = 12.0;

const NAMESPACES: [&str; 5] = [
    "default",
    "kube-system",
    "monitoring",
    "payments",
    "staging",
];
const LONG_NAMESPACE: &str = "customer-acme-production-eu-west-1-payments";

/// A namespace option: its name, an optional match explanation and whether
/// it is selected.
#[derive(Clone, Copy)]
struct Namespace {
    name: &'static str,
    detail: Option<&'static str>,
    selected: bool,
}

fn namespace(name: &'static str, selected: bool) -> Namespace {
    Namespace {
        name,
        detail: None,
        selected,
    }
}

/// A namespace trigger 260 points wide.
fn trigger(id: &str) -> Combobox<components::NoFilter> {
    Combobox::new(id)
        .accessibility_label(id)
        .placeholder("Select namespaces")
        .search_placeholder("Search namespaces…")
        .noun("namespaces")
        .width(TRIGGER_WIDTH)
}

fn show_namespaces(combobox: Combobox<components::NoFilter>, ui: &mut Ui, options: &[Namespace]) {
    combobox
        .filter_by(|option: &Namespace| option.name)
        .show_items(ui, options, |cb, option| {
            match option.detail {
                Some(detail) => {
                    cb.item_with_status_detail(option.name, detail, option.selected, None)
                }
                None => cb.item(option.name, option.selected),
            };
        });
}

/// The tooltip of a truncated value (`.tip`), shown in place under its
/// trigger.
fn tooltip(ui: &mut Ui, text: &str) {
    let theme = ui.tokens();
    let role = type_scale::META.with_weight(Weight::Medium);
    let padding = vec2(10.0, 6.0);
    let shadow = theme.shadow.tooltip;
    let galley = role.galley(
        ui.ctx(),
        text,
        theme.components.tooltip_text,
        TOOLTIP_MAX_WIDTH - 2.0 * padding.x,
    );
    // Shrink-to-fit: as wide as the unwrapped text, at most the max width.
    let unwrapped = role
        .galley(ui.ctx(), text, theme.components.tooltip_text, f32::INFINITY)
        .rect
        .width();
    let width = (unwrapped + 2.0 * padding.x).min(TOOLTIP_MAX_WIDTH);
    let (rect, _) = ui.allocate_exact_size(
        vec2(width, galley.rect.height() + 2.0 * padding.y),
        egui::Sense::hover(),
    );
    shadow.paint(ui.painter(), rect, radius::MENU_ITEM);
    ui.painter()
        .rect_filled(rect, radius::MENU_ITEM, theme.components.tooltip_bg);
    components::design::type_scale::paint_galley(
        ui.painter(),
        rect.min + padding,
        galley,
        theme.components.tooltip_text,
    );
}

fn closed_states(ui: &mut Ui) {
    section(ui, "Closed states", |ui| {
        column(ui, CLOSED_GAP, |ui| {
            let md = size::CONTROL_MD;
            let none: [Namespace; 0] = [];
            captioned(ui, md, "Placeholder", |ui| {
                show_namespaces(trigger("Placeholder"), ui, &none);
            });
            captioned(ui, md, "Hover", |ui| {
                show_namespaces(
                    trigger("Hover").force_state(ControlState::Hovered),
                    ui,
                    &none,
                );
            });
            captioned(ui, md, "Single value with status", |ui| {
                show_namespaces(
                    trigger("Single value")
                        .selected_text("kube-system")
                        .selected_status(Some(true)),
                    ui,
                    &none,
                );
            });
            captioned(ui, md, "Multiple values", |ui| {
                show_namespaces(
                    trigger("Multiple values")
                        .selected_text("kube-system, monitoring")
                        .selected_status(Some(true))
                        .selected_more(2),
                    ui,
                    &none,
                );
            });
            captioned(ui, md, "All namespaces", |ui| {
                show_namespaces(
                    trigger("All namespaces")
                        .selected_text("All namespaces")
                        .selected_icon(Icon::GlobeAlt),
                    ui,
                    &none,
                );
            });
            column(ui, DEMO_CAPTION_GAP, |ui| {
                show_namespaces(
                    trigger("Truncated")
                        .selected_text(LONG_NAMESPACE)
                        .selected_status(Some(true))
                        .force_state(ControlState::Hovered),
                    ui,
                    &none,
                );
                ui.add_space(TOOLTIP_OFFSET);
                tooltip(ui, LONG_NAMESPACE);
                type_scale::META.label(
                    ui,
                    "Truncated — full value in tooltip on hover",
                    ui.tokens().text.muted,
                );
            });
            captioned(ui, md, "Disabled (no cluster)", |ui| {
                show_namespaces(
                    trigger("Disabled")
                        .enabled(false)
                        .disabled_reason("No cluster"),
                    ui,
                    &none,
                );
            });
        });
    });
}

fn multi_select_open(ui: &mut Ui) {
    section(ui, "Multi-select, open", |ui| {
        let options =
            NAMESPACES.map(|name| namespace(name, name == "kube-system" || name == "monitoring"));
        show_namespaces(
            trigger("Multi-select")
                .selected_text("kube-system, monitoring")
                .selected_status(Some(true))
                .multi_select()
                .select_all("All namespaces", CheckState::Mixed)
                .preview_open("", Some(2), POPOVER_GAP),
            ui,
            &options,
        );
    });
}

fn search_narrows(ui: &mut Ui) {
    section(ui, "Search narrows the list", |ui| {
        let options = [
            namespace("monitoring", true),
            Namespace {
                name: "Customer: Acme",
                detail: Some("payments · matched on label team=money"),
                selected: false,
            },
            namespace("default", false),
            namespace("staging", false),
            namespace("kube-dns", false),
        ];
        show_namespaces(
            trigger("Search narrows")
                .selected_text("kube-system")
                .selected_status(Some(true))
                .multi_select()
                .preview_open("m", Some(0), POPOVER_GAP),
            ui,
            &options,
        );
    });
}

fn no_results(ui: &mut Ui) {
    section(ui, "No results", |ui| {
        let options = NAMESPACES.map(|name| namespace(name, false));
        show_namespaces(
            trigger("No results")
                .preview_open("prodd", None, 0.0)
                .preview_without_trigger(),
            ui,
            &options,
        );
    });
}

fn single_select_open(ui: &mut Ui) {
    let theme = ui.tokens();
    column(ui, 0.0, |ui| {
        section(ui, "Single select, open", |ui| {
            let profiles = [
                "General",
                "Baseline",
                "Restricted",
                "Network admin",
                "System admin",
            ];
            Combobox::new("Debug profile")
                .accessibility_label("Debug profile")
                .selected_text("General")
                .size(FieldSize::Sm)
                .width(SMALL_TRIGGER_WIDTH)
                .searchable(false)
                .preview_open("", Some(1), POPOVER_GAP)
                .filter_by(|profile: &&str| profile)
                .show_items(ui, &profiles, |cb, profile| {
                    cb.item(*profile, *profile == "General");
                });
        });
        ui.add_space(space::S12 + NOTE_OFFSET);
        ui.set_max_width(NOTE_WIDTH);
        type_scale::META.label(
            ui,
            "Keyboard highlight is the neutral row fill; selection is the trailing check. A \
             highlighted row is never a solid accent fill.",
            theme.text.muted,
        );
    });
}

fn c05_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            cell(
                ui,
                vec2(CLOSED_COLUMN, ui.available_height()),
                Layout::top_down(Align::Min),
                0.0,
                closed_states,
            );
            column(ui, 0.0, multi_select_open);
            column(ui, SECTION_GAP, |ui| {
                search_narrows(ui);
                no_results(ui);
            });
            single_select_open(ui);
        });
    });
}

#[test]
fn c05_select_board() {
    snapshot_board("c05_select", c05_scene);
}
