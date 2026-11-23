//! The C07 "Menu & tooltip" component board
//! (`docs/design-oracles/harbour/src/C07.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C07`.

use components::design::size;
use components::icons::Icon;
use components::menu::{self, Menu};
use components::test_support::board::{
    BoardHeader, board, captioned, cell, column, row, section, snapshot_board,
};
use components::tooltip::{self, TooltipContent};
use components::{ButtonGroup, ControlState, MenuItem, MoreButton, ToggleButton};
use egui::{Align, Layout, Ui, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C07 · components::{Menu, MoreButton, Tooltip}",
    title: "Menu & tooltip",
    description: "Menus share the popover surface with selects. Items are 32 px; icons sit in \
                  a 16 px column so labels align; destructive items go last, after a separator. \
                  Tooltips are cream with dark text, 12 px, and appear after 500 ms.",
};

const COLUMN_GAP: f32 = 48.0;
/// Gap between the sections of the first column (`.col` gap 12 around an
/// 8 point spacer).
const SECTION_GAP: f32 = 12.0;
const SECTION_SPACER: f32 = 8.0;
const TRIGGER_GAP: f32 = 8.0;
const MENU_WIDTH: f32 = 220.0;
const DANGER_MENU_WIDTH: f32 = 200.0;
/// Gap between the actions menu and its submenu.
const SUBMENU_GAP: f32 = 4.0;
/// How far the submenu sits below the top of the actions menu.
const SUBMENU_OFFSET: f32 = 72.0;
const TOOLTIP_COLUMN: f32 = 380.0;
const TOOLTIP_DEMO_GAP: f32 = 16.0;
/// Gap between the label tooltip and the toggle it explains.
const TOOLTIP_TO_CONTROL: f32 = 4.0;

fn trigger_states(ui: &mut Ui) {
    section(ui, "Trigger states (rest · hover · open · focus)", |ui| {
        row(ui, 0.0, TRIGGER_GAP, Align::Min, |ui| {
            for state in [
                ControlState::Rest,
                ControlState::Hovered,
                ControlState::Pressed,
                ControlState::Focused,
            ] {
                MoreButton::new("More actions")
                    .force_state(state)
                    .show(ui, |_| {});
            }
        });
    });
}

fn resource_actions(menu: &mut Menu<'_>) {
    menu.item(
        MenuItem::new("Edit YAML")
            .icon(Icon::PencilSquare)
            .shortcut("E"),
    );
    menu.item(
        MenuItem::new("View logs")
            .icon(Icon::DocumentText)
            .shortcut("L")
            .force_state(ControlState::Hovered),
    );
    menu.submenu(MenuItem::new("Shell").icon(Icon::CommandLine), |_| {});
    menu.item(
        MenuItem::new("Copy name")
            .icon(Icon::ClipboardDocument)
            .shortcut("⌘C"),
    );
    menu.separator();
    menu.item(MenuItem::new("Restart rollout").icon(Icon::ArrowPath));
    menu.item(MenuItem::new("Scale…").icon(Icon::ArrowsUpDown));
    menu.separator();
    menu.item(
        MenuItem::new("Delete")
            .icon(Icon::Trash)
            .shortcut("⌫")
            .danger(),
    );
}

fn debug_images(menu: &mut Menu<'_>) {
    menu.heading("Debug image");
    menu.item(
        MenuItem::new("Busybox")
            .detail("General profile")
            .force_state(ControlState::Hovered),
    );
    menu.item(MenuItem::new("Ubuntu").detail("General profile"));
    menu.item(MenuItem::new("Netshoot").detail("Network admin profile"));
    menu.separator();
    menu.item(MenuItem::new("Configure presets…").icon(Icon::Cog6Tooth));
}

fn actions_menu(ui: &mut Ui) {
    section(ui, "Resource actions menu with submenu", |ui| {
        row(ui, 0.0, SUBMENU_GAP, Align::Min, |ui| {
            menu::preview(ui, MENU_WIDTH, "Resource actions", resource_actions);
            column(ui, 0.0, |ui| {
                ui.add_space(SUBMENU_OFFSET);
                menu::preview(ui, MENU_WIDTH, "Shell", debug_images);
            });
        });
    });
}

fn danger_and_disabled(ui: &mut Ui) {
    section(ui, "Danger highlight & disabled item", |ui| {
        menu::preview(ui, DANGER_MENU_WIDTH, "Actions", |menu| {
            menu.item(MenuItem::new("Edit").icon(Icon::PencilSquare));
            menu.item(
                MenuItem::new("Force delete")
                    .icon(Icon::Trash)
                    .danger()
                    .force_state(ControlState::Hovered),
            );
            menu.item(
                MenuItem::new("Restart rollout")
                    .icon(Icon::ArrowPath)
                    .enabled(false)
                    .disabled_reason("Deployment is paused"),
            );
        });
    });
}

fn label_tooltip(ui: &mut Ui) {
    column(ui, 0.0, |ui| {
        let tip = tooltip::show_inline(ui, TooltipContent::Text("Show timestamps"));
        ui.add_space(TOOLTIP_TO_CONTROL);
        let group = size::CONTROL_SM + 2.0 * size::TRAY_PADDING;
        ui.horizontal(|ui| {
            ui.add_space((tip.rect.width() - group) / 2.0);
            ButtonGroup::new("Timestamps")
                .toggle(ToggleButton::icon(Icon::Clock, "Timestamps", false))
                .show(ui);
        });
    });
}

fn tooltips(ui: &mut Ui) {
    section(ui, "Tooltip", |ui| {
        column(ui, TOOLTIP_DEMO_GAP, |ui| {
            demo(ui, "Label tooltip", label_tooltip);
            demo(ui, "Key / value tooltip (container status)", |ui| {
                tooltip::show_inline(
                    ui,
                    TooltipContent::KeyValues(&[
                        ("Container", "sidecar"),
                        ("State", "Waiting · ContainerCreating"),
                        ("Restarts", "3"),
                        ("Message", "Waiting for volume mount"),
                    ]),
                );
            });
            demo(
                ui,
                "Truncation tooltip — only when text is actually clipped",
                |ui| {
                    tooltip::show_inline(
                        ui,
                        TooltipContent::Text("kube-controller-manager-kind-control-plane"),
                    );
                },
            );
            demo(ui, "Shortcut tooltip", |ui| {
                tooltip::show_inline(
                    ui,
                    TooltipContent::Shortcut {
                        label: "Next match",
                        key: "Enter",
                    },
                );
            });
        });
    });
}

/// A tooltip demo above its caption.
fn demo(ui: &mut Ui, caption: &str, add_demo: impl FnOnce(&mut Ui)) {
    captioned(ui, 0.0, caption, add_demo);
}

fn c07_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            column(ui, SECTION_GAP, |ui| {
                trigger_states(ui);
                // The spacer sits between two gaps.
                ui.add_space(SECTION_SPACER + SECTION_GAP);
                actions_menu(ui);
            });
            column(ui, 0.0, danger_and_disabled);
            cell(
                ui,
                vec2(TOOLTIP_COLUMN, ui.available_height()),
                Layout::top_down(Align::Min),
                0.0,
                tooltips,
            );
        });
    });
}

#[test]
fn c07_menu_tooltip_board() {
    snapshot_board("c07_menu_tooltip", c07_scene);
}
