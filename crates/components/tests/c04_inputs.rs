//! The C04 "Text input & search field" component board
//! (`docs/design-oracles/harbour/src/C04.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C04`.

use components::design::size;
use components::test_support::board::{
    BoardHeader, DEMO_CAPTION_GAP, board, captioned, captioned_height, cell, column, row, section,
    snapshot_board,
};
use components::{
    ControlState, FieldMessage, FormField, NumberField, SearchField, SearchStatus, TextArea,
    TextInput,
};
use egui::{Align, Layout, Ui, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C04 · components::{TextInput, SearchField, TextArea, NumberField}",
    title: "Text input & search field",
    description: "Inputs are 32 px borderless pill wells; focus draws a 1.5 px coral ring with a \
                  soft halo. The search field owns its own count, match navigation and regex \
                  toggle so every surface searches the same way.",
};

/// The two columns of the board.
const LEFT_COLUMN: f32 = 840.0;
const COLUMN_GAP: f32 = 56.0;
/// Gap between the sections of a column.
const SECTION_GAP: f32 = 28.0;
/// The text input states grid: three equal columns.
const GRID_COLUMNS: usize = 3;
const GRID_GAP_X: f32 = 24.0;
const GRID_GAP_Y: f32 = 20.0;
const STATE_INPUT_WIDTH: f32 = 260.0;
/// Gap between the search field demos.
const SEARCH_GAP: f32 = 18.0;
/// Gap between the form fields.
const FIELD_GAP: f32 = 20.0;
const FORM_INPUT_WIDTH: f32 = 360.0;
const BADGE_INPUT_WIDTH: f32 = 120.0;
const TEMPLATE_MIN_HEIGHT: f32 = 56.0;

fn input_states(ui: &mut Ui) {
    section(ui, "Text input states", |ui| {
        let column_width =
            (LEFT_COLUMN - GRID_GAP_X * (GRID_COLUMNS - 1) as f32) / GRID_COLUMNS as f32;
        let cell_size = vec2(column_width, captioned_height(size::CONTROL_MD));
        let state = |caption: &'static str,
                     value: &'static str,
                     configure: fn(TextInput<'_>) -> TextInput<'_>| {
            (caption, value, configure)
        };
        let rows = [
            [
                state("Rest", "", |input| input),
                state("Hover", "", |input| {
                    input.force_state(ControlState::Hovered)
                }),
                state("Focus", "Styrhous on linux", |input| {
                    input.force_state(ControlState::Focused)
                }),
            ],
            [
                state("Filled", "Styrhous on linux", |input| input),
                state("Error", "Styrhous on linux", |input| input.invalid(true)),
                state("Disabled", "Styrhous on linux", |input| {
                    input.enabled(false)
                }),
            ],
        ];
        column(ui, GRID_GAP_Y, |ui| {
            for states in rows {
                row(ui, cell_size.y, GRID_GAP_X, Align::Min, |ui| {
                    for (caption, value, configure) in states {
                        let layout = Layout::top_down(Align::Min);
                        cell(ui, cell_size, layout, 0.0, |ui| {
                            captioned(ui, size::CONTROL_MD, caption, |ui| {
                                let mut value = value.to_owned();
                                configure(
                                    TextInput::new(&mut value)
                                        .id_salt(caption)
                                        .placeholder("Device name")
                                        .accessibility_label(caption)
                                        .width(STATE_INPUT_WIDTH),
                                )
                                .show(ui);
                            });
                        });
                    }
                });
            }
        });
    });
}

/// One search field demo: its query, regex toggle and status.
struct SearchDemo {
    caption: &'static str,
    query: &'static str,
    regex: bool,
    status: SearchStatus,
    focused: bool,
}

fn search_fields(ui: &mut Ui) {
    let demos = [
        SearchDemo {
            caption: "Empty",
            query: "",
            regex: false,
            status: SearchStatus::Idle,
            focused: false,
        },
        SearchDemo {
            caption: "Typing — 150 ms debounce, count updates as the scan progresses",
            query: "http",
            regex: false,
            status: SearchStatus::Searching { found: 4 },
            focused: true,
        },
        SearchDemo {
            caption: "Results — current / total; ↑ ↓ move between matches",
            query: "http",
            regex: false,
            status: SearchStatus::Matches {
                active: Some(2),
                total: 12,
            },
            focused: false,
        },
        SearchDemo {
            caption: "No matches",
            query: "timeout",
            regex: false,
            status: SearchStatus::Matches {
                active: None,
                total: 0,
            },
            focused: false,
        },
        SearchDemo {
            caption: "Regex on",
            query: "GET /v1/.*",
            regex: true,
            status: SearchStatus::Matches {
                active: Some(4),
                total: 5,
            },
            focused: false,
        },
        SearchDemo {
            caption: "Invalid regex — message below, field keeps focus",
            query: "coredns[a-z",
            regex: true,
            status: SearchStatus::Invalid("Unclosed character class at position 7".to_owned()),
            focused: false,
        },
    ];
    section(
        ui,
        "Search field (find-in-surface and table filter)",
        |ui| {
            column(ui, SEARCH_GAP, |ui| {
                for demo in demos {
                    column(ui, DEMO_CAPTION_GAP, |ui| {
                        let mut query = demo.query.to_owned();
                        let mut regex = demo.regex;
                        let error = match &demo.status {
                            SearchStatus::Invalid(message) => Some(message.clone()),
                            _ => None,
                        };
                        let mut field = SearchField::new(&mut query)
                            .regex(&mut regex)
                            .id_salt(demo.caption)
                            .placeholder("Search logs…")
                            .accessibility_label(demo.caption)
                            .status(demo.status);
                        if demo.focused {
                            field = field.force_state(ControlState::Focused);
                        }
                        field.show(ui);
                        if let Some(error) = &error {
                            FieldMessage::Error(error).show(ui);
                        }
                        components::design::type_scale::META.label(
                            ui,
                            demo.caption,
                            components::design::theme::ThemeExt::tokens(ui).text.muted,
                        );
                    });
                }
            });
        },
    );
}

fn form_fields(ui: &mut Ui) {
    section(ui, "Form fields", |ui| {
        column(ui, FIELD_GAP, |ui| {
            FormField::new("Display name")
                .help("Leave blank to use the kubeconfig context name.")
                .show(ui, |ui| {
                    TextInput::new(&mut "Production Europe".to_owned())
                        .accessibility_label("Display name")
                        .width(FORM_INPUT_WIDTH)
                        .show(ui);
                });
            FormField::new("Badge")
                .error(Some("Use one or two characters."))
                .show(ui, |ui| {
                    TextInput::new(&mut "PRX".to_owned())
                        .accessibility_label("Badge")
                        .width(BADGE_INPUT_WIDTH)
                        .invalid(true)
                        .show(ui);
                });
            FormField::new("Command template")
                .help("Use {command} as the placeholder.")
                .show(ui, |ui| {
                    ui.set_max_width(FORM_INPUT_WIDTH);
                    TextArea::new(&mut "alacritty -e {command}".to_owned())
                        .monospace()
                        .min_rows(1)
                        .min_height(TEMPLATE_MIN_HEIGHT)
                        .accessibility_label("Command template")
                        .show(ui);
                });
            FormField::new("Replicas")
                .help("Current: 2 · Min 0")
                .show(ui, |ui| {
                    NumberField::new(&mut "3".to_owned())
                        .range(0..=i64::from(i32::MAX))
                        .accessibility_labels("Replicas", "Decrease", "Increase")
                        .show(ui);
                });
        });
    });
}

fn c04_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            cell(
                ui,
                vec2(LEFT_COLUMN, ui.available_height()),
                Layout::top_down(Align::Min),
                SECTION_GAP,
                |ui| {
                    input_states(ui);
                    search_fields(ui);
                },
            );
            column(ui, SECTION_GAP, form_fields);
        });
    });
}

#[test]
fn c04_inputs_board() {
    snapshot_board("c04_inputs", c04_scene);
}
