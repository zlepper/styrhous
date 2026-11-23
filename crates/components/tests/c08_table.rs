//! The C08 "Table" component board (`docs/design-oracles/harbour/src/C08.html`)
//! rebuilt from the real components, compared with its oracle by
//! `scripts/design-oracles/compare.sh C08`.

use std::collections::HashSet;

use components::design::theme::{ThemeExt, Tone};
use components::design::{radius, size, space, type_scale};
use components::icons::Icon;
use components::menu::{self, MenuItem};
use components::test_support::board::{
    BoardHeader, board, cell, column, row, section, snapshot_board,
};
use components::tooltip::TooltipContent;
use components::{
    BulkSelectionBar, Button, ButtonSize, CellAlign, CheckState, ControlState, EmptyState,
    HeaderState, MoreButton, ReorderableTable, SortDirection, SortState, Table, TableRowBuilder,
};
use egui::{Align, Layout, Ui, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C08 · components::{Table, ReorderableTable}",
    title: "Table",
    description: "Header 36 px, rows 40 px, 12 px cell padding, numbers right-aligned with \
                  tabular figures. Missing values are an em dash with an explaining tooltip, \
                  never “Unavail…”.",
};

const COLUMN_GAP: f32 = 40.0;
const MAIN_WIDTH: f32 = 1000.0;
const MAIN_SECTION_GAP: f32 = 20.0;
const SIDE_SECTION_GAP: f32 = 28.0;
const SMALL_TABLE_GAP: f32 = 24.0;
const SMALL_TABLE_WIDTH: f32 = 392.0;
const SIDE_WIDTH: f32 = 392.0;
const COLUMN_MENU_WIDTH: f32 = 200.0;
/// The bulk bar sits 12 points above the table (`.col` gap 12).
const BAR_TO_TABLE: f32 = space::S12;
const BULK_BAR_HEIGHT: f32 = 44.0;
/// The filtered empty state's card.
const EMPTY_CARD_HEIGHT: f32 = 132.0;
const METRICS_MISSING: &str = "Metrics server not available";

struct Pod {
    name: &'static str,
    ready: &'static str,
    containers: &'static [Tone],
    status: (&'static str, Tone),
    cpu: Option<&'static str>,
    memory: Option<&'static str>,
    restarts: &'static str,
    age: &'static str,
}

const PODS: [Pod; 6] = [
    Pod {
        name: "coredns-66bc5c9577-ffw2s",
        ready: "1/1",
        containers: &[Tone::Success],
        status: ("Running", Tone::Success),
        cpu: Some("15m"),
        memory: Some("22Mi"),
        restarts: "0",
        age: "220d",
    },
    Pod {
        name: "coredns-66bc5c9577-z9gt9",
        ready: "1/1",
        containers: &[Tone::Success],
        status: ("Running", Tone::Success),
        cpu: Some("12m"),
        memory: Some("20Mi"),
        restarts: "0",
        age: "220d",
    },
    Pod {
        name: "api-7b948f-pod",
        ready: "1/2",
        containers: &[Tone::Success, Tone::Warning],
        status: ("Pending", Tone::Warning),
        cpu: Some("125m"),
        memory: Some("96Mi"),
        restarts: "5",
        age: "3h",
    },
    Pod {
        name: "kube-controller-manager-kind-control-plane",
        ready: "1/1",
        containers: &[Tone::Success],
        status: ("Running", Tone::Success),
        cpu: None,
        memory: None,
        restarts: "0",
        age: "220d",
    },
    Pod {
        name: "job-nightly-report-28xk2",
        ready: "0/1",
        containers: &[Tone::Danger],
        status: ("Failed", Tone::Danger),
        cpu: None,
        memory: None,
        restarts: "2",
        age: "14m",
    },
    Pod {
        name: "kube-proxy-v86gd",
        ready: "1/1",
        containers: &[Tone::Success],
        status: ("Running", Tone::Success),
        cpu: Some("1m"),
        memory: Some("18Mi"),
        restarts: "0",
        age: "220d",
    },
];
/// The selected rows and the hovered row.
const SELECTED: [usize; 2] = [1, 2];
const HOVERED: usize = 2;

fn metric(ui: &mut Ui, value: Option<&str>) {
    match value {
        Some(value) => TableRowBuilder::number(ui, value),
        None => TableRowBuilder::missing(ui, METRICS_MISSING),
    }
}

fn pod_cell(ui: &mut Ui, index: usize, pod: &Pod, column: usize) {
    match column {
        0 => TableRowBuilder::text(ui, pod.name, true),
        1 => TableRowBuilder::number(ui, pod.ready),
        2 => TableRowBuilder::containers(
            ui,
            pod.containers
                .iter()
                .map(|tone| (*tone, "Container", TooltipContent::Text("Container"))),
        ),
        3 => {
            TableRowBuilder::status(ui, pod.status.0, pod.status.1);
        }
        4 => metric(ui, pod.cpu),
        5 => metric(ui, pod.memory),
        6 => TableRowBuilder::number(ui, pod.restarts),
        7 => TableRowBuilder::number(ui, pod.age),
        _ => {
            let mut button = MoreButton::new(format!("More actions for {}", pod.name));
            if index == HOVERED {
                button = button.force_state(ControlState::Hovered);
            }
            button.show(ui, |_| {});
        }
    }
}

fn resource_table(ui: &mut Ui) {
    section(
        ui,
        "Resource table — sorted by Name, two rows selected, Status header hovered",
        |ui| {
            column(ui, BAR_TO_TABLE, |ui| {
                cell(
                    ui,
                    vec2(MAIN_WIDTH, BULK_BAR_HEIGHT),
                    Layout::top_down(Align::Min),
                    0.0,
                    |ui| {
                        BulkSelectionBar::new(SELECTED.len(), PODS.len()).show(ui, |ui| {
                            Button::danger_quiet("Delete")
                                .leading_icon(Icon::Trash)
                                .size(ButtonSize::Sm)
                                .show(ui);
                        });
                    },
                );
                let height = size::TABLE_HEADER + PODS.len() as f32 * size::ROW_PITCH;
                cell(
                    ui,
                    vec2(MAIN_WIDTH, height),
                    Layout::top_down(Align::Min),
                    0.0,
                    pods_table,
                );
            });
        },
    );
}

fn pods_table(ui: &mut Ui) {
    let pods: Vec<(usize, &Pod)> = PODS.iter().enumerate().collect();
    let mut selected: HashSet<usize> = SELECTED.into();
    let sort = SortState::new("name", SortDirection::Ascending);
    Table::new("pods")
        .column("name", "Name", |col| {
            col.sortable().fill_remaining().initial_width(0.0)
        })
        .column("ready", "Ready", |col| col.numeric().initial_width(72.0))
        .column("containers", "Containers", |col| col.initial_width(104.0))
        .column("status", "Status", |col| {
            col.sortable().initial_width(120.0)
        })
        .column("cpu", "CPU", |col| col.numeric().initial_width(80.0))
        .column("memory", "Memory", |col| col.numeric().initial_width(88.0))
        .column("restarts", "Restarts", |col| {
            col.numeric().initial_width(88.0)
        })
        .column("age", "Age", |col| col.numeric().initial_width(72.0))
        .column("actions", "", |col| {
            col.align(CellAlign::Center).initial_width(48.0)
        })
        .selectable()
        .force_header_state("status", HeaderState::Hovered)
        .force_row_hovered(HOVERED)
        .show_selectable_configurable_with_row_response(
            ui,
            &pods,
            &mut selected,
            |(index, _)| Some(index),
            Some(&sort),
            |_, _, _, _| {},
            |_, _| {},
            |ui, (index, pod), column| pod_cell(ui, *index, pod, column),
            |_, _, _| {},
        );
}

fn column_resize(ui: &mut Ui) {
    section(ui, "Column resize (dragging Owner)", |ui| {
        cell(
            ui,
            vec2(SMALL_TABLE_WIDTH, size::TABLE_HEADER + size::ROW_PITCH),
            Layout::top_down(Align::Min),
            0.0,
            |ui| {
                let accent = ui.tokens().accent.text;
                Table::new("resize")
                    .column("name", "Name", |col| col.initial_width(180.0))
                    .column("owner", "Owner", |col| col.initial_width(140.0))
                    .column("ready", "Ready", |col| col.numeric().initial_width(72.0))
                    .force_header_state("owner", HeaderState::Resizing)
                    .show(ui, &[()], |ui, _, column| match column {
                        0 => TableRowBuilder::text(ui, "coredns-66bc5c9577…", true),
                        1 => TableRowBuilder::text_in(
                            ui,
                            type_scale::UI,
                            "ReplicaSet / coredns",
                            accent,
                        ),
                        _ => TableRowBuilder::number(ui, "1/1"),
                    });
            },
        );
    });
}

/// The filtered empty state: the shared empty state (C12, built by its
/// own package) drawn in place until it lands.
fn filtered_empty(ui: &mut Ui) {
    section(ui, "Filtered empty", |ui| {
        let theme = ui.tokens();
        let (rect, _) = ui.allocate_exact_size(
            vec2(SMALL_TABLE_WIDTH, EMPTY_CARD_HEIGHT),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(rect, radius::CARD, theme.surface.panel);
        let mut card = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        let _ = EmptyState::filtered("No pods match “redis”")
            .description("7 pods hidden by the filter.")
            .action("Clear filter")
            .show(&mut card);
    });
}

fn column_visibility(ui: &mut Ui) {
    section(ui, "Column visibility", |ui| {
        menu::preview(ui, COLUMN_MENU_WIDTH, "Columns", |menu| {
            menu.heading("Columns");
            for (label, visible) in [
                ("Name", true),
                ("Owner", false),
                ("Ready", true),
                ("Containers", true),
                ("Status", true),
                ("Node", false),
                ("Age", true),
            ] {
                let mut item = MenuItem::new(label).checkbox(CheckState::from(visible));
                if label == "Ready" {
                    item = item.force_state(ControlState::Hovered);
                }
                menu.item(item);
            }
            menu.separator();
            menu.item(MenuItem::new("Reset to default").icon(Icon::ArrowUturnLeft));
        });
    });
}

fn loading_rows(ui: &mut Ui) {
    section(ui, "Loading rows", |ui| {
        cell(
            ui,
            vec2(SIDE_WIDTH, size::TABLE_HEADER + 2.0 * size::ROW_PITCH),
            Layout::top_down(Align::Min),
            0.0,
            |ui| {
                Table::new("loading")
                    .column("name", "Name", |col| {
                        col.initial_width(200.0).skeleton_width(150.0)
                    })
                    .column("status", "Status", |col| {
                        col.initial_width(112.0).skeleton_width(70.0)
                    })
                    .column("age", "Age", |col| {
                        col.numeric().initial_width(80.0).skeleton_width(32.0)
                    })
                    .show_loading(ui, 2, "Loading pods");
            },
        );
    });
}

struct Preset {
    name: &'static str,
    image: &'static str,
    profile: &'static str,
}

fn reorderable_rows(ui: &mut Ui) {
    section(ui, "Reorderable rows — dragging Ubuntu", |ui| {
        cell(
            ui,
            vec2(SIDE_WIDTH, size::TABLE_HEADER + 3.0 * size::ROW_PITCH),
            Layout::top_down(Align::Min),
            0.0,
            |ui| {
                let mut presets = vec![
                    Preset {
                        name: "Busybox",
                        image: "busybox",
                        profile: "General",
                    },
                    Preset {
                        name: "Ubuntu",
                        image: "ubuntu",
                        profile: "General",
                    },
                    Preset {
                        name: "Netshoot",
                        image: "nicolaka/netshoot",
                        profile: "Network admin",
                    },
                ];
                ReorderableTable::new("debug-images")
                    .column("name", "Name", |col| col.initial_width(120.0))
                    .column("image", "Image", |col| {
                        col.fill_remaining().initial_width(0.0)
                    })
                    .column("profile", "Profile", |col| col.initial_width(100.0))
                    .preview_drag(1, 1, vec2(space::S6, -space::S6))
                    .show(
                        ui,
                        &mut presets,
                        |preset| format!("Reorder {}", preset.name),
                        |ui, preset, _, column| match column {
                            0 => TableRowBuilder::text(ui, preset.name, false),
                            1 => TableRowBuilder::mono(ui, preset.image),
                            _ => TableRowBuilder::text(ui, preset.profile, false),
                        },
                    );
            },
        );
    });
}

fn c08_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            column(ui, MAIN_SECTION_GAP, |ui| {
                resource_table(ui);
                row(ui, 0.0, SMALL_TABLE_GAP, Align::Min, |ui| {
                    column(ui, 0.0, column_resize);
                    column(ui, 0.0, filtered_empty);
                });
            });
            column(ui, SIDE_SECTION_GAP, |ui| {
                column_visibility(ui);
                loading_rows(ui);
                reorderable_rows(ui);
            });
        });
    });
}

#[test]
fn c08_table_board() {
    snapshot_board("c08_table", c08_scene);
}
