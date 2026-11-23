//! The C01 "Icons & status" component board
//! (`docs/design-oracles/harbour/src/C01.html`) rebuilt from the real
//! components, compared with its oracle by `scripts/design-oracles/compare.sh C01`.

use components::design::theme::{ThemeExt, Tone};
use components::design::type_scale;
use components::design::{radius, size, space};
use components::icons::Icon;
use components::test_support::board::{
    BoardHeader, DEMO_CAPTION_GAP, board, captioned, captioned_height, cell, column, row, section,
    snapshot_board,
};
use components::{
    Avatar, CountBadge, Pill, Spinner, StatusLabel, container_square, kind_badge, skeleton_bar,
};
use egui::{Align, Layout, Margin, Ui, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C01 · components::icons, components::status",
    title: "Icons & status",
    description: "One outline icon family at 16 px by default, tinted with the surrounding text \
                  colour. Status is always dot + words, never colour alone.",
};

/// Gap between the board's stacked rows (`.col` gap 28).
const ROW_GAP: f32 = 28.0;
/// Gap between the sections of one row.
const SECTION_GAP: f32 = 56.0;
/// The icon grid: eight equal columns, 28 point rows.
const GRID_COLUMNS: usize = 8;
const GRID_ROW: f32 = 28.0;
const GRID_GAP_X: f32 = 8.0;
const GRID_GAP_Y: f32 = 4.0;

fn icon_set(ui: &mut Ui) {
    let theme = ui.tokens();
    let icons: Vec<Icon> = Icon::DESIGN
        .iter()
        .copied()
        .filter(|icon| *icon != Icon::ChevronLeft)
        .collect();
    let width = ui.available_width();
    let column_width = (width - GRID_GAP_X * (GRID_COLUMNS - 1) as f32) / GRID_COLUMNS as f32;
    section(
        ui,
        "Icon set · Heroicons outline 1.5 px stroke, MIT",
        |ui| {
            column(ui, GRID_GAP_Y, |ui| {
                for line in icons.chunks(GRID_COLUMNS) {
                    row(ui, GRID_ROW, GRID_GAP_X, Align::Center, |ui| {
                        for icon in line {
                            cell(
                                ui,
                                vec2(column_width, GRID_ROW),
                                Layout::left_to_right(Align::Center),
                                space::S8,
                                |ui| {
                                    icon.show(ui, size::ICON, theme.text.primary);
                                    type_scale::META.label(ui, icon.name(), theme.text.secondary);
                                },
                            );
                        }
                    });
                }
            });
        },
    );
}

fn sizes(ui: &mut Ui) {
    let primary = ui.tokens().text.primary;
    section(ui, "Sizes", |ui| {
        let tallest = captioned_height(size::ICON_XL);
        row(ui, tallest, space::S20, Align::Max, |ui| {
            for (icon_size, caption) in [
                (size::ICON_XS, "12 px"),
                (size::ICON_SM, "14 px"),
                (size::ICON, "16 px"),
                (size::ICON_LG, "20 px"),
                (size::ICON_XL, "24 px"),
            ] {
                captioned(ui, icon_size, caption, |ui| {
                    Icon::Cube.show(ui, icon_size, primary);
                });
            }
        });
    });
}

fn status_text(ui: &mut Ui) {
    section(ui, "Status text (tables)", |ui| {
        row(
            ui,
            type_scale::UI.line_height(),
            space::S20,
            Align::Center,
            |ui| {
                for (text, tone) in [
                    ("Running", Tone::Success),
                    ("Pending", Tone::Warning),
                    ("Failed", Tone::Danger),
                    ("Succeeded", Tone::Neutral),
                    ("Terminating", Tone::Warning),
                ] {
                    StatusLabel::new(text, tone).show(ui);
                }
            },
        );
    });
}

fn status_pills(ui: &mut Ui) {
    section(ui, "Status pills (headers)", |ui| {
        row(ui, size::PILL_HEIGHT, space::S8, Align::Center, |ui| {
            for (text, tone) in [
                ("Following", Tone::Success),
                ("Reconnecting…", Tone::Warning),
                ("Stream failed", Tone::Danger),
                ("Stream finished", Tone::Neutral),
                ("Modified", Tone::Warning),
                ("Selected", Tone::Accent),
            ] {
                Pill::new(text, tone).show(ui);
            }
        });
    });
}

fn badges(ui: &mut Ui) {
    section(ui, "Badges & indicators", |ui| {
        let skeleton_height = 2.0 * size::SKELETON_BAR + space::S6;
        let tallest = captioned_height(skeleton_height);
        row(ui, tallest, space::S24, Align::Center, |ui| {
            captioned(ui, size::PILL_HEIGHT, "Kind badge", |ui| {
                kind_badge(ui, "Deployment");
            });
            captioned(ui, size::COUNT_BADGE, "Count", |ui| {
                row(ui, size::COUNT_BADGE, space::S6, Align::Center, |ui| {
                    CountBadge::new(3).show(ui);
                    CountBadge::new(12).tone(Tone::Accent).show(ui);
                    CountBadge::new(100).tone(Tone::Neutral).show(ui);
                });
            });
            captioned(ui, size::CONTAINER_SQUARE, "Containers", |ui| {
                row(
                    ui,
                    size::CONTAINER_SQUARE,
                    size::CONTAINER_SQUARE_GAP,
                    Align::Center,
                    |ui| {
                        for (index, tone) in
                            [Tone::Success, Tone::Success, Tone::Warning, Tone::Danger]
                                .into_iter()
                                .enumerate()
                        {
                            container_square(ui, tone, &format!("Container {}", index + 1));
                        }
                    },
                );
            });
            captioned(ui, size::SPINNER, "Spinner", |ui| {
                Spinner::new().rotation(0.0).show(ui);
            });
            captioned(ui, skeleton_height, "Skeleton", |ui| {
                column(ui, space::S6, |ui| {
                    skeleton_bar(ui, 120.0);
                    skeleton_bar(ui, 80.0);
                });
            });
        });
    });
}

fn avatars(ui: &mut Ui) {
    let theme = ui.tokens();
    section(ui, "Avatar (cluster) states", |ui| {
        egui::Frame::new()
            .fill(theme.nav.rail)
            .corner_radius(radius::CARD)
            .inner_margin(Margin::symmetric(space::S20 as i8, space::S16 as i8))
            .show(ui, |ui| {
                let height = captioned_height(size::AVATAR);
                row(ui, height, space::S20, Align::Min, |ui| {
                    for (initials, category, status, selected, hovered, caption) in [
                        ("DE", 0, Tone::Success, false, false, "Connected"),
                        ("KK", 1, Tone::Warning, false, false, "Connecting"),
                        ("PR", 3, Tone::Danger, false, false, "Error"),
                        ("KL", 2, Tone::Neutral, false, false, "Idle"),
                        ("CS", 4, Tone::Success, true, false, "Selected"),
                        ("AD", 5, Tone::Neutral, false, true, "Hover"),
                    ] {
                        column(ui, DEMO_CAPTION_GAP, |ui| {
                            Avatar::new(initials)
                                .category(category)
                                .status(status)
                                .selected(selected)
                                .hovered(hovered)
                                .show(ui);
                            type_scale::META.label(ui, caption, theme.nav.text_secondary);
                        });
                    }
                });
            });
    });
}

fn c01_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        column(ui, ROW_GAP, |ui| {
            icon_set(ui);
            row(ui, 0.0, SECTION_GAP, Align::Min, |ui| {
                sizes(ui);
                status_text(ui);
                status_pills(ui);
            });
            row(ui, 0.0, SECTION_GAP, Align::Min, |ui| {
                badges(ui);
                avatars(ui);
            });
        });
    });
}

#[test]
fn c01_icons_and_status_board() {
    snapshot_board("c01_icons_status", c01_scene);
}
