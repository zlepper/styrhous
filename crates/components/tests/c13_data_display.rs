//! The C13 "Data display" component board
//! (`docs/design-oracles/harbour/src/C13.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C13`.

use components::design::theme::Tone;
use components::icons::Icon;
use components::test_support::board::{
    BoardHeader, board, cell, column, row, section, snapshot_board,
};
use components::{
    Card, Chip, CodeBlock, ControlState, Disclosure, ListRow, ListRowTrailing, Property,
    PropertyGrid, Tabs, UsageChart, UsageResource, chip_flow, labelled_chips, list_group,
};
use egui::{Align, Id, Layout, Ui, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C13 · components::{PropertyGrid, Disclosure, Chip, CodeBlock, UsageChart, ListRow, Tabs}",
    title: "Data display",
    description: "Read-only building blocks for inspectors and settings. Labels are 11 px \
                  condensed captions above 13 px values; every copyable value reveals a copy \
                  affordance on hover and confirms with a tooltip.",
};

/// Width of each column of the board.
const COLUMN_WIDTH: f32 = 640.0;
const COLUMN_GAP: f32 = 40.0;
/// Gap between the sections of a column.
const SECTION_GAP: f32 = 24.0;
/// Gap between the two disclosure cards.
const DISCLOSURE_GAP: f32 = 8.0;
/// The usage chart card: two charts side by side.
const CHART_WIDTH: f32 = 288.0;
const CHART_GAP: f32 = 24.0;
/// Gap between the rows of the chips and code section.
const CHIPS_GAP: f32 = 12.0;
/// Width of the list row card and the tab strip.
const NARROW_WIDTH: f32 = 560.0;

fn property_grid(ui: &mut Ui) {
    section(ui, "Property grid (with copyable value hovered)", |ui| {
        Card::new().show(ui, |ui| {
            let properties = [
                Property::text("Kind", "Pod"),
                Property::text("Name", "coredns-66bc5c9577-ffw2s"),
                Property::text("Namespace", "kube-system"),
                Property::status("Status", "Running", Tone::Success),
                Property::link("Node", "kind-control-plane", Id::new("node")),
                Property::text("QoS class", "Burstable"),
                Property::mono("Pod IP", "10.244.0.3"),
                Property::text("Age", "220d"),
                Property::mono("UID", "d1f2c3a4-b5e6-47f8-9a0b")
                    .copyable()
                    .force_state(ControlState::Hovered),
            ];
            let _ = PropertyGrid::new(&properties).show(ui);
        });
    });
}

fn label_chips(ui: &mut Ui) {
    chip_flow(ui, |ui| {
        for (key, value) in [
            ("k8s-app=", "coredns"),
            ("pod-template-hash=", "66bc5c9577"),
            ("app.kubernetes.io/name=", "kube-dns"),
            ("kubernetes.io/cluster-service=", "true"),
        ] {
            Chip::new(value).key(key).show(ui);
        }
    });
}

fn disclosures(ui: &mut Ui) {
    section(ui, "Disclosure card", |ui| {
        column(ui, DISCLOSURE_GAP, |ui| {
            for open in [false, true] {
                Disclosure::new(("labels", open), "Labels & annotations")
                    .meta("6 labels · 3 annotations")
                    .force_open(open)
                    .show(ui, label_chips);
            }
        });
    });
}

/// A design sample at `x` (of 300) and `y` (of the 72 point plot, the
/// limit at 8 and the baseline at 64) as a chart sample of `limit`.
fn sample(limit: f32, x: f32, y: f32) -> [f32; 2] {
    [x / 300.0, (64.0 - y) / 56.0 * limit]
}

fn usage_charts(ui: &mut Ui) {
    section(ui, "Usage chart", |ui| {
        Card::new().show(ui, |ui| {
            row(ui, 0.0, CHART_GAP, Align::Min, |ui| {
                let cpu = [
                    (0.0, 58.0),
                    (40.0, 57.0),
                    (80.0, 55.0),
                    (120.0, 56.0),
                    (160.0, 50.0),
                    (200.0, 52.0),
                    (240.0, 47.0),
                    (300.0, 49.0),
                ]
                .map(|(x, y)| sample(100.0, x, y));
                let memory = [
                    (0.0, 54.0),
                    (60.0, 54.0),
                    (120.0, 53.0),
                    (180.0, 53.0),
                    (240.0, 52.0),
                    (300.0, 52.0),
                ]
                .map(|(x, y)| sample(128.0, x, y));
                column(ui, 0.0, |ui| {
                    ui.set_width(CHART_WIDTH);
                    UsageChart::new("CPU", UsageResource::Cpu, "15m")
                        .limit(100.0, "Limit")
                        .format(&|value| format!("{value}m"))
                        .samples(&cpu)
                        .show(ui);
                });
                column(ui, 0.0, |ui| {
                    ui.set_width(CHART_WIDTH);
                    UsageChart::new("Memory", UsageResource::Memory, "22Mi")
                        .limit(128.0, "Limit")
                        .format(&|value| format!("{value}Mi"))
                        .samples(&memory)
                        .show(ui);
                });
            });
        });
    });
}

fn chips_and_code(ui: &mut Ui) {
    section(ui, "Chips & code", |ui| {
        column(ui, CHIPS_GAP, |ui| {
            let rows: [(&str, &[&str]); 3] = [
                ("Command", &["/coredns"]),
                (
                    "Args",
                    &[
                        "-conf",
                        "/etc/coredns/Corefile",
                        "--cluster-domain=cluster.local",
                        "--feature-gates=VeryLongFeature=true",
                    ],
                ),
                ("Ports", &["53/UDP", "53/TCP", "9153/TCP"]),
            ];
            for (label, chips) in rows {
                labelled_chips(ui, label, |ui| {
                    for chip in chips {
                        Chip::new(chip).show(ui);
                    }
                });
            }
            CodeBlock::new("[server]\nport = 8080")
                .filename("app.toml")
                .show(ui);
        });
    });
}

fn list_rows(ui: &mut Ui) {
    section(ui, "Navigation list row", |ui| {
        ui.set_max_width(NARROW_WIDTH);
        list_group(ui, |group| {
            group.row(
                ListRow::new(Icon::CircleStack, "Contexts")
                    .description("Display names and rail badges")
                    .trailing(ListRowTrailing::Meta("10 contexts")),
            );
            group.row(
                ListRow::new(Icon::AdjustmentsHorizontal, "Namespace selector")
                    .description("How namespaces are labelled and searched")
                    .force_state(ControlState::Hovered),
            );
            group.row(
                ListRow::new(Icon::UserCircle, "License & account")
                    .description("Hosted Styrhous service")
                    .trailing(ListRowTrailing::Status("Active", Tone::Success)),
            );
        });
    });
}

fn tabs(ui: &mut Ui) {
    section(ui, "Tabs", |ui| {
        ui.set_max_width(NARROW_WIDTH);
        Tabs::new("c13-tabs").force_hover(2).show(ui, |tabs| {
            tabs.tab("Overview", None, |_| {});
            tabs.tab("YAML", None, |_| {});
            tabs.tab_with_count("Events", 4, |_| {});
            tabs.tab("Logs", None, |_| {});
        });
    });
}

fn c13_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            let size = vec2(COLUMN_WIDTH, ui.available_height());
            let layout = Layout::top_down(Align::Min);
            cell(ui, size, layout, SECTION_GAP, |ui| {
                property_grid(ui);
                disclosures(ui);
                usage_charts(ui);
            });
            cell(ui, size, layout, SECTION_GAP, |ui| {
                chips_and_code(ui);
                list_rows(ui);
                tabs(ui);
            });
        });
    });
}

#[test]
fn c13_data_display_board() {
    snapshot_board("c13_data_display", c13_scene);
}
