//! Focused snapshots of the C01 status components, so a regression shows
//! up next to the component instead of only on the whole C01 board
//! (`c01_icons_status.rs`).

use std::f32::consts::FRAC_PI_2;

use components::design::theme::{ThemeExt, Tone};
use components::design::{radius, size, space};
use components::test_support::{UiHarnessSnapshot, canvas_harness};
use components::{Avatar, CountBadge, Pill, Spinner};
use egui::{Align, Layout, Margin, Ui, vec2};
use egui_kittest::Harness;

const TONES: [Tone; 6] = [
    Tone::Neutral,
    Tone::Accent,
    Tone::Info,
    Tone::Success,
    Tone::Warning,
    Tone::Danger,
];

/// A harness `width` × `height` points showing `scene` on the canvas,
/// padded by 16 points.
fn harness(width: f32, height: f32, scene: impl Fn(&mut Ui) + 'static) -> Harness<'static> {
    canvas_harness(width, height, Some(space::S16), scene)
}

fn row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), size::RAIL_BUTTON + space::S8),
        Layout::left_to_right(Align::Center),
        add,
    );
}

#[test]
fn avatar_states() {
    let mut harness = harness(420.0, 120.0, |ui| {
        let rail = ui.tokens().nav.rail;
        egui::Frame::new()
            .fill(rail)
            .corner_radius(radius::CARD)
            .inner_margin(Margin::same(space::S16 as i8))
            .show(ui, |ui| {
                row(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = space::S20;
                    Avatar::new("DE").category(0).status(Tone::Success).show(ui);
                    Avatar::new("KK").category(1).status(Tone::Warning).show(ui);
                    Avatar::new("PR").category(3).status(Tone::Danger).show(ui);
                    Avatar::new("KL").category(2).status(Tone::Neutral).show(ui);
                    Avatar::new("CS")
                        .category(4)
                        .status(Tone::Success)
                        .selected(true)
                        .show(ui);
                    Avatar::new("AD").category(5).hovered(true).show(ui);
                });
            });
    });
    harness.ui_harness("status/avatar_states");
}

#[test]
fn count_badges() {
    let mut harness = harness(300.0, 180.0, |ui| {
        for tone in [Tone::Danger, Tone::Accent, Tone::Neutral] {
            ui.horizontal(|ui| {
                for count in [3, 12, 100] {
                    CountBadge::new(count).tone(tone).show(ui);
                }
            });
        }
        // Painted onto a rail-button-sized control, cut out by its ring.
        let (button, _) =
            ui.allocate_exact_size(egui::Vec2::splat(size::RAIL_BUTTON), egui::Sense::hover());
        let theme = ui.tokens();
        ui.painter()
            .rect_filled(button, radius::ROW, theme.nav.item_selected);
        let badge = CountBadge::new(7);
        let badge_size = badge.size(ui);
        let rect = components::corner_rect(
            button,
            egui::Align2::RIGHT_TOP,
            badge_size,
            vec2(-4.0, -4.0),
        );
        badge.paint(ui, rect, theme.surface.canvas);
    });
    harness.ui_harness("status/count_badges");
}

#[test]
fn pills() {
    let mut harness = harness(560.0, 112.0, |ui| {
        ui.horizontal(|ui| {
            for tone in TONES {
                Pill::new(&format!("{tone:?}"), tone).show(ui);
            }
        });
        ui.horizontal(|ui| {
            Pill::new("Live", Tone::Success).live().show(ui);
            Pill::new("Reconnecting…", Tone::Warning).live().show(ui);
            Pill::new("Without a lamp", Tone::Neutral)
                .without_dot()
                .show(ui);
        });
    });
    harness.ui_harness("status/pills");
}

#[test]
fn spinner() {
    let mut harness = harness(220.0, 80.0, |ui| {
        row(ui, |ui| {
            Spinner::new().rotation(0.0).show(ui);
            Spinner::new().rotation(FRAC_PI_2).show(ui);
            Spinner::new().size(size::SPINNER_LG).rotation(0.0).show(ui);
            let accent = ui.tokens().accent.fill;
            egui::Frame::new()
                .fill(accent)
                .corner_radius(radius::FULL)
                .inner_margin(Margin::symmetric(space::S8 as i8, space::S4 as i8))
                .show(ui, |ui| {
                    Spinner::new().on_fill().rotation(0.0).show(ui);
                });
        });
    });
    harness.ui_harness("status/spinner");
}
