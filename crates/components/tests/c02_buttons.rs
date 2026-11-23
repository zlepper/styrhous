//! The C02 "Button" component board (`docs/design-oracles/harbour/src/C02.html`)
//! rebuilt from the real components, compared with its oracle by
//! `scripts/design-oracles/compare.sh C02`.

use components::design::theme::ThemeExt;
use components::design::type_scale;
use components::icons::Icon;
use components::test_support::board::{
    BoardHeader, board, captioned, captioned_height, cell, column, row, section, snapshot_board,
};
use components::{Button, ButtonSize, ButtonVariant, ControlState};
use egui::{Align, Layout, Ui, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C02 · components::Button",
    title: "Button",
    description: "One button, six variants. Primary appears at most once per surface; solid \
                  danger is reserved for confirming a destructive dialog — everywhere else use \
                  Danger quiet.",
};

/// Gap between the board's sections (`.col` gap 28).
const SECTION_GAP: f32 = 28.0;
/// Gap between the rows of the variants grid.
const GRID_ROW_GAP: f32 = 12.0;
/// The variants grid: a label column, then one column per state.
const LABEL_COLUMN: f32 = 132.0;
const STATE_COLUMN: f32 = 160.0;
const HEADER_ROW: f32 = 24.0;
const VARIANT_ROW: f32 = 48.0;
/// Gap between the demos of the "Sizes & content" and icon-only rows.
const DEMO_GAP: f32 = 16.0;
/// Gap between the groups of the composition row.
const COMPOSITION_GAP: f32 = 32.0;
/// Gap between the buttons of one footer.
const FOOTER_GAP: f32 = 8.0;

/// One state column of the variants grid.
#[derive(Clone, Copy)]
enum State {
    Rest,
    Forced(ControlState),
    Disabled,
    Loading,
}

impl State {
    fn apply(self, button: Button) -> Button {
        match self {
            Self::Rest => button,
            Self::Forced(state) => button.force_state(state),
            Self::Disabled => button.enabled(false),
            Self::Loading => button.loading(true),
        }
    }
}

const STATES: [(&str, State); 6] = [
    ("Rest", State::Rest),
    ("Hover", State::Forced(ControlState::Hovered)),
    ("Pressed", State::Forced(ControlState::Pressed)),
    ("Focus-visible", State::Forced(ControlState::Focused)),
    ("Disabled", State::Disabled),
    ("Loading", State::Loading),
];

const VARIANTS: [(&str, ButtonVariant, &str); 6] = [
    ("Primary", ButtonVariant::Primary, "Apply changes"),
    ("Secondary", ButtonVariant::Secondary, "Refresh"),
    ("Soft", ButtonVariant::Soft, "Add cluster"),
    ("Ghost", ButtonVariant::Ghost, "Cancel"),
    ("Danger", ButtonVariant::Danger, "Delete"),
    ("Danger quiet", ButtonVariant::DangerQuiet, "Sign out"),
];

fn variants(ui: &mut Ui) {
    let theme = ui.tokens();
    section(ui, "Variants × states", |ui| {
        column(ui, GRID_ROW_GAP, |ui| {
            row(ui, HEADER_ROW, 0.0, Align::Center, |ui| {
                cell(
                    ui,
                    vec2(LABEL_COLUMN, HEADER_ROW),
                    Layout::left_to_right(Align::Center),
                    0.0,
                    |_| {},
                );
                for (label, _) in STATES {
                    cell(
                        ui,
                        vec2(STATE_COLUMN, HEADER_ROW),
                        Layout::left_to_right(Align::Center),
                        0.0,
                        |ui| {
                            type_scale::META.label(ui, label, theme.text.muted);
                        },
                    );
                }
            });
            for (name, variant, label) in VARIANTS {
                row(ui, VARIANT_ROW, 0.0, Align::Center, |ui| {
                    let layout = Layout::left_to_right(Align::Center);
                    cell(ui, vec2(LABEL_COLUMN, VARIANT_ROW), layout, 0.0, |ui| {
                        type_scale::UI_CONTROL.label(ui, name, theme.text.primary);
                    });
                    for (_, state) in STATES {
                        cell(ui, vec2(STATE_COLUMN, VARIANT_ROW), layout, 0.0, |ui| {
                            state.apply(Button::new(label).variant(variant)).show(ui);
                        });
                    }
                });
            }
        });
    });
}

fn sizes_and_content(ui: &mut Ui) {
    section(ui, "Sizes & content", |ui| {
        let tallest = captioned_height(ButtonSize::Lg.height());
        row(ui, tallest, DEMO_GAP, Align::Center, |ui| {
            for (size, caption) in [
                (ButtonSize::Sm, "sm · 28"),
                (ButtonSize::Md, "md · 32"),
                (ButtonSize::Lg, "lg · 36"),
            ] {
                captioned(ui, size.height(), caption, |ui| {
                    Button::secondary("Scan clusters").size(size).show(ui);
                });
            }
            let md = ButtonSize::Md.height();
            captioned(ui, md, "Leading icon", |ui| {
                Button::secondary("Refresh")
                    .leading_icon(Icon::ArrowPath)
                    .show(ui);
            });
            captioned(ui, md, "Trailing icon", |ui| {
                Button::secondary("Open in browser")
                    .trailing_icon(Icon::ArrowTopRightOnSquare)
                    .show(ui);
            });
            captioned(ui, type_scale::BUTTON.line_height(), "Link", |ui| {
                Button::link("Manage license").show(ui);
            });
        });
    });
}

fn icon_only(ui: &mut Ui) {
    section(ui, "Icon-only buttons", |ui| {
        let tallest = captioned_height(ButtonSize::Md.height());
        row(ui, tallest, DEMO_GAP, Align::Center, |ui| {
            let ghost_sm = |icon, label| {
                Button::icon(icon, label)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
            };
            let secondary = |icon, label| Button::icon(icon, label);
            let demos: [(Button, ButtonSize, &str); 7] = [
                (
                    ghost_sm(Icon::EllipsisHorizontal, "More actions"),
                    ButtonSize::Sm,
                    "Ghost sm",
                ),
                (
                    ghost_sm(Icon::EllipsisHorizontal, "More actions")
                        .force_state(ControlState::Hovered),
                    ButtonSize::Sm,
                    "Ghost sm hover",
                ),
                (secondary(Icon::Pencil, "Edit"), ButtonSize::Md, "Secondary"),
                (
                    secondary(Icon::Pencil, "Edit").force_state(ControlState::Hovered),
                    ButtonSize::Md,
                    "Secondary hover",
                ),
                (
                    Button::icon(Icon::Trash, "Delete").variant(ButtonVariant::DangerQuiet),
                    ButtonSize::Md,
                    "Danger quiet",
                ),
                (
                    ghost_sm(Icon::ArrowRight, "Forward").enabled(false),
                    ButtonSize::Sm,
                    "Disabled",
                ),
                (
                    ghost_sm(Icon::XMark, "Close").force_state(ControlState::Focused),
                    ButtonSize::Sm,
                    "Focus",
                ),
            ];
            for (button, size, caption) in demos {
                captioned(ui, size.height(), caption, |ui| {
                    button.show(ui);
                });
            }
        });
    });
}

fn composition(ui: &mut Ui) {
    section(ui, "Composition rules", |ui| {
        let md = ButtonSize::Md.height();
        let tallest = captioned_height(md);
        row(ui, tallest, COMPOSITION_GAP, Align::Center, |ui| {
            let footer = |ui: &mut Ui, caption: &str, buttons: Vec<Button>| {
                captioned(ui, md, caption, |ui| {
                    row(ui, md, FOOTER_GAP, Align::Center, |ui| {
                        for button in buttons {
                            button.show(ui);
                        }
                    });
                });
            };
            footer(
                ui,
                "Dialog footer",
                vec![
                    Button::secondary("Cancel"),
                    Button::primary("Restart rollout"),
                ],
            );
            footer(
                ui,
                "Destructive footer",
                vec![
                    Button::secondary("Keep editing"),
                    Button::danger("Discard changes"),
                ],
            );
            footer(
                ui,
                "Account actions",
                vec![
                    Button::secondary("Refresh license").leading_icon(Icon::ArrowPath),
                    Button::secondary("Manage account").trailing_icon(Icon::ArrowTopRightOnSquare),
                    Button::danger_quiet("Sign out"),
                ],
            );
        });
    });
}

fn c02_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        column(ui, SECTION_GAP, |ui| {
            variants(ui);
            sizes_and_content(ui);
            icon_only(ui);
            composition(ui);
        });
    });
}

#[test]
fn c02_buttons_board() {
    snapshot_board("c02_buttons", c02_scene);
}
