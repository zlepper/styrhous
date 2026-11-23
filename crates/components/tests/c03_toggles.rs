//! The C03 "Toggle button, button group & segmented control" component board
//! (`docs/design-oracles/harbour/src/C03.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C03`.

use components::design::theme::{self, ThemeExt, ThemeScope};
use components::design::type_scale::{self, Weight};
use components::design::{radius, size, space};
use components::icons::Icon;
use components::test_support::board::{
    BoardHeader, board, captioned, captioned_height, cell, column, row, section, snapshot_board,
};
use components::{ButtonGroup, ControlState, Segmented, SegmentedSize, ToggleButton};
use egui::{Align, Layout, Margin, Ui};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C03 · components::{ToggleButton, ButtonGroup, Segmented}",
    title: "Toggle button, button group & segmented control",
    description: "Pressed-state controls for view options. Groups share one 32 px frame with 2 px \
                  padding; every child is a 28 px pill so groups line up with inputs and buttons \
                  on the same toolbar.",
};

/// The two columns of the board.
const LEFT_COLUMN: egui::Vec2 = egui::vec2(640.0, 562.0);
const COLUMN_GAP: f32 = 64.0;
/// Gap between the sections of a column.
const SECTION_GAP: f32 = 28.0;
/// Gap between the demos of the toggle states row.
const STATES_GAP: f32 = 28.0;
/// Gap between the demos of the standalone row.
const STANDALONE_GAP: f32 = 20.0;
/// Gap between stacked demos.
const STACK_GAP: f32 = 20.0;
/// The "Replaces" card.
const CARD_WIDTH: f32 = 560.0;
const CARD_TITLE_GAP: f32 = 8.0;
const CARD_TEXT_GAP: f32 = 6.0;

fn toggle_states(ui: &mut Ui) {
    section(ui, "Toggle button states (inside a group)", |ui| {
        let md = size::CONTROL_MD;
        row(ui, captioned_height(md), STATES_GAP, Align::Center, |ui| {
            let line_numbers =
                |pressed| ToggleButton::icon(Icon::NumberedList, "Line numbers", pressed);
            let states = [
                ("Off", line_numbers(false)),
                (
                    "Off · hover",
                    line_numbers(false).force_state(ControlState::Hovered),
                ),
                ("On", line_numbers(true)),
                (
                    "On · hover",
                    line_numbers(true).force_state(ControlState::Hovered),
                ),
                (
                    "Focus",
                    line_numbers(false).force_state(ControlState::Focused),
                ),
                ("Disabled", line_numbers(false).enabled(false)),
            ];
            for (caption, toggle) in states {
                captioned(ui, md, caption, |ui| {
                    ButtonGroup::new(caption).toggle(toggle).show(ui);
                });
            }
        });
    });
}

fn button_groups(ui: &mut Ui) {
    section(ui, "Button groups", |ui| {
        column(ui, STACK_GAP, |ui| {
            let md = size::CONTROL_MD;
            captioned(
                ui,
                md,
                "View options — Line numbers · Timestamps · ANSI colours · Source labels (2 on)",
                |ui| {
                    ButtonGroup::new("View options")
                        .toggle(ToggleButton::icon(Icon::NumberedList, "Line numbers", true))
                        .toggle(ToggleButton::icon(Icon::Clock, "Timestamps", true))
                        .toggle(ToggleButton::icon(Icon::Swatch, "ANSI colours", false))
                        .toggle(ToggleButton::icon(Icon::Tag, "Source labels", false))
                        .show(ui);
                },
            );
            captioned(
                ui,
                md,
                "Line navigation — previous / next displayed line",
                |ui| {
                    ButtonGroup::new("Line navigation")
                        .toggle(ToggleButton::action(Icon::ArrowUp, "Previous line"))
                        .toggle(ToggleButton::action(Icon::ArrowDown, "Next line"))
                        .show(ui);
                },
            );
            captioned(ui, md, "Mixed: text toggle inside a group", |ui| {
                ButtonGroup::new("Search")
                    .toggle(
                        ToggleButton::text("Filter", true)
                            .leading_icon(Icon::Funnel)
                            .accessibility_label("Filter to matches"),
                    )
                    .separator()
                    .toggle(
                        ToggleButton::text("Follow", false)
                            .leading_icon(Icon::ChevronDoubleDown)
                            .accessibility_label("Follow tail"),
                    )
                    .show(ui);
            });
        });
    });
}

fn standalone(ui: &mut Ui) {
    section(ui, "Standalone toggle", |ui| {
        let md = size::CONTROL_MD;
        row(
            ui,
            captioned_height(md),
            STANDALONE_GAP,
            Align::Center,
            |ui| {
                let follow = |pressed| {
                    ToggleButton::text("Follow", pressed).leading_icon(Icon::ChevronDoubleDown)
                };
                let demos = [
                    ("Follow · on", follow(true)),
                    ("Follow · off", follow(false)),
                    (
                        "Filter · on",
                        ToggleButton::text("Matches only", true).leading_icon(Icon::Funnel),
                    ),
                    (
                        "Icon solo · off",
                        ToggleButton::icon(Icon::Funnel, "funnel", false),
                    ),
                    ("Hover", follow(false).force_state(ControlState::Hovered)),
                ];
                for (caption, toggle) in demos {
                    captioned(ui, md, caption, |ui| {
                        toggle.show(ui);
                    });
                }
            },
        );
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Instance {
    Current,
    Previous,
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Table,
    Yaml,
}

fn segmented(ui: &mut Ui) {
    section(ui, "Segmented control", |ui| {
        column(ui, STACK_GAP, |ui| {
            let md = size::CONTROL_MD;
            let sm = size::CONTROL_SM;
            captioned(ui, md, "Log instance · md", |ui| {
                Segmented::new(&mut Instance::Current)
                    .option(Instance::Current, "Current")
                    .option(Instance::Previous, "Previous")
                    .show(ui);
            });
            captioned(ui, md, "Previous selected, Current hovered", |ui| {
                Segmented::new(&mut Instance::Previous)
                    .option(Instance::Current, "Current")
                    .option(Instance::Previous, "Previous")
                    .force_hover(0)
                    .show(ui);
            });
            captioned(ui, sm, "With icons · sm", |ui| {
                Segmented::new(&mut View::Table)
                    .option_with_icon(View::Table, Icon::QueueList, "Table")
                    .option_with_icon(View::Yaml, Icon::DocumentText, "YAML")
                    .size(SegmentedSize::Sm)
                    .show(ui);
            });
            captioned(ui, sm, "Three options", |ui| {
                Segmented::new(&mut 6)
                    .option(1, "1 h")
                    .option(6, "6 h")
                    .option(24, "24 h")
                    .size(SegmentedSize::Sm)
                    .show(ui);
            });
        });
    });
}

fn replaces_card(ui: &mut Ui) {
    let theme = ui.tokens();
    egui::Frame::new()
        .fill(theme.surface.panel)
        .corner_radius(radius::CARD)
        .inner_margin(Margin::same(space::S16 as i8))
        .show(ui, |ui| {
            theme::scope(ui, ThemeScope::Card, |ui| {
                let inner_width = CARD_WIDTH - 2.0 * space::S16;
                ui.set_width(inner_width);
                column(ui, 0.0, |ui| {
                    row(
                        ui,
                        type_scale::META.line_height(),
                        CARD_TITLE_GAP,
                        Align::Center,
                        |ui| {
                            type_scale::META.with_weight(Weight::SemiBold).label(
                                ui,
                                "Replaces",
                                theme.danger.text,
                            );
                            type_scale::META.label(ui, "log toolbar badges", theme.text.secondary);
                        },
                    );
                    ui.add_space(CARD_TEXT_GAP);
                    type_scale::META.label(
                        ui,
                        "The current log toolbar marks an active toggle with a green check badge \
                         that sits on the button edge and overlaps its neighbour, and fixes the \
                         group width (158/160 pt) so new buttons spill. A toggle now shows its on \
                         state as a cream pill inside its own 28×28 bounds, and a group sizes to \
                         its children.",
                        theme.text.secondary,
                    );
                });
            });
        });
}

fn c03_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            cell(
                ui,
                LEFT_COLUMN,
                Layout::top_down(Align::Min),
                SECTION_GAP,
                |ui| {
                    toggle_states(ui);
                    button_groups(ui);
                },
            );
            column(ui, SECTION_GAP, |ui| {
                standalone(ui);
                segmented(ui);
                replaces_card(ui);
            });
        });
    });
}

#[test]
fn c03_toggles_board() {
    snapshot_board("c03_toggles", c03_scene);
}
