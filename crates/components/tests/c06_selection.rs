//! The C06 "Checkbox, radio & switch" component board
//! (`docs/design-oracles/harbour/src/C06.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C06`.

use components::design::theme::ThemeExt;
use components::design::{size, type_scale};
use components::test_support::board::{
    BoardHeader, board, cell, column, column_sized, row, section, snapshot_board,
};
use components::{CheckState, Checkbox, ControlState, Radio, RadioCard, Switch};
use egui::{Align, Layout, Ui, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C06 · components::{Checkbox, Radio, RadioCard, Switch}",
    title: "Checkbox, radio & switch",
    description: "16 px controls with the label as part of the hit target. Switch is for settings \
                  that apply immediately; checkboxes are for selections and settings saved with a \
                  button.",
};

const COLUMN_GAP: f32 = 64.0;
/// Gap between the sections of a column.
const SECTION_GAP: f32 = 28.0;
/// The states grid: a label column, then one column per state.
const LABEL_COLUMN: f32 = 160.0;
const STATE_COLUMN: f32 = 110.0;
const HEADER_ROW: f32 = 24.0;
const STATE_ROW: f32 = 40.0;
/// Gap between the grid's rows (the section's `.col` gap 12).
const GRID_ROW_GAP: f32 = 12.0;
/// Gap between the labelled controls.
const LABELLED_GAP: f32 = 12.0;
/// Gap between radio cards, and their width.
const CARD_GAP: f32 = 12.0;
const CARD_WIDTH: f32 = 520.0;

/// A state column of the grid.
#[derive(Clone, Copy)]
enum State {
    Forced(Option<ControlState>),
    Disabled,
}

const STATES: [(&str, State); 4] = [
    ("Rest", State::Forced(None)),
    ("Hover", State::Forced(Some(ControlState::Hovered))),
    ("Focus-visible", State::Forced(Some(ControlState::Focused))),
    ("Disabled", State::Disabled),
];

/// A control of the grid's rows.
#[derive(Clone, Copy)]
enum Control {
    Checkbox(CheckState),
    Radio(bool),
    Switch(bool),
}

const ROWS: [(&str, Control); 7] = [
    ("Checkbox · off", Control::Checkbox(CheckState::Off)),
    ("Checkbox · on", Control::Checkbox(CheckState::On)),
    ("Checkbox · mixed", Control::Checkbox(CheckState::Mixed)),
    ("Radio · off", Control::Radio(false)),
    ("Radio · on", Control::Radio(true)),
    ("Switch · off", Control::Switch(false)),
    ("Switch · on", Control::Switch(true)),
];

/// Where a bare control sits in its row. In the board each control is an
/// inline box in a `<span>`, standing on the span's 13/20 text baseline
/// (its own baseline is the bottom of its first child: the check or dot
/// when it has one, else its bottom edge). The span's line box is centred
/// in the row. Returns the line box height and the control's top in it.
fn inline_box(ui: &Ui, control: Control) -> (f32, f32) {
    let role = type_scale::UI;
    let above_text = role.css_baseline(ui.pixels_per_point());
    let below_text = role.line_height() - above_text;
    let (height, baseline) = match control {
        Control::Switch(_) => (size::SWITCH.y, size::SWITCH.y),
        Control::Checkbox(CheckState::Off) | Control::Radio(false) => {
            (size::CHECKBOX, size::CHECKBOX)
        }
        Control::Checkbox(_) => (size::CHECKBOX, (size::CHECKBOX + size::ICON_XS) / 2.0),
        Control::Radio(true) => (size::CHECKBOX, (size::CHECKBOX + size::RADIO_DOT) / 2.0),
    };
    let above = above_text.max(baseline);
    let below = below_text.max(height - baseline);
    (above + below, above - baseline)
}

fn show_control(ui: &mut Ui, control: Control, state: State, name: &str) {
    let (forced, enabled) = match state {
        State::Forced(forced) => (forced, true),
        State::Disabled => (None, false),
    };
    match control {
        Control::Checkbox(check) => {
            let mut checkbox = Checkbox::with_state(check, "")
                .accessibility_label(name)
                .enabled(enabled);
            if let Some(forced) = forced {
                checkbox = checkbox.force_state(forced);
            }
            checkbox.show(ui);
        }
        Control::Radio(selected) => {
            let mut current = selected;
            let mut radio = Radio::new(&mut current, true, "")
                .accessibility_label(name)
                .enabled(enabled);
            if let Some(forced) = forced {
                radio = radio.force_state(forced);
            }
            radio.show(ui);
        }
        Control::Switch(on) => {
            let mut on = on;
            let mut switch = Switch::new(&mut on, "")
                .accessibility_label(name)
                .enabled(enabled);
            if let Some(forced) = forced {
                switch = switch.force_state(forced);
            }
            switch.show(ui);
        }
    }
}

fn states(ui: &mut Ui) {
    let theme = ui.tokens();
    section(ui, "States", |ui| {
        column(ui, GRID_ROW_GAP, |ui| {
            let layout = Layout::left_to_right(Align::Center);
            row(ui, HEADER_ROW, 0.0, Align::Center, |ui| {
                cell(ui, vec2(LABEL_COLUMN, HEADER_ROW), layout, 0.0, |_| {});
                for (label, _) in STATES {
                    cell(ui, vec2(STATE_COLUMN, HEADER_ROW), layout, 0.0, |ui| {
                        type_scale::META.label(ui, label, theme.text.muted);
                    });
                }
            });
            for (name, control) in ROWS {
                row(ui, STATE_ROW, 0.0, Align::Center, |ui| {
                    cell(ui, vec2(LABEL_COLUMN, STATE_ROW), layout, 0.0, |ui| {
                        type_scale::UI_CONTROL.label(ui, name, theme.text.primary);
                    });
                    for (state_name, state) in STATES {
                        cell(ui, vec2(STATE_COLUMN, STATE_ROW), layout, 0.0, |ui| {
                            let (height, top) = inline_box(ui, control);
                            column_sized(ui, height, 0.0, |ui| {
                                ui.add_space(top);
                                show_control(ui, control, state, &format!("{name} · {state_name}"));
                            });
                        });
                    }
                });
            }
        });
    });
}

#[derive(Clone, Copy, PartialEq)]
enum Server {
    Cloud,
    SelfHosted,
}

fn with_labels(ui: &mut Ui) {
    section(ui, "With labels", |ui| {
        column(ui, LABELLED_GAP, |ui| {
            Checkbox::new(&mut true, "Show system namespaces").show(ui);
            Checkbox::new(&mut false, "Include terminated pods")
                .description("Completed and failed pods stay visible for 1 hour.")
                .show(ui);
            let mut server = Server::SelfHosted;
            Radio::new(&mut server, Server::Cloud, "Styrhous cloud").show(ui);
            Radio::new(&mut server, Server::SelfHosted, "Self-hosted server").show(ui);
            Switch::new(&mut true, "Check for updates automatically").show(ui);
        });
    });
}

fn radio_cards(ui: &mut Ui) {
    section(ui, "Radio card", |ui| {
        ui.set_max_width(CARD_WIDTH);
        column(ui, CARD_GAP, |ui| {
            RadioCard::new(true, "Automatic", "Use your system’s preferred terminal.").show(ui);
            RadioCard::new(
                false,
                "Custom launcher",
                "Use a command template for your preferred terminal.",
            )
            .show(ui);
            RadioCard::new(
                false,
                "Hover",
                "Surface lightens; the whole card is the hit target.",
            )
            .force_state(ControlState::Hovered)
            .show(ui);
        });
    });
}

fn c06_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        row(ui, 0.0, COLUMN_GAP, Align::Min, |ui| {
            column(ui, SECTION_GAP, states);
            column(ui, SECTION_GAP, |ui| {
                with_labels(ui);
                radio_cards(ui);
            });
        });
    });
}

#[test]
fn c06_selection_board() {
    snapshot_board("c06_selection", c06_scene);
}
