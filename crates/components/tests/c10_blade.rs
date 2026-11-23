//! The C10 "Blade" component board (`docs/design-oracles/harbour/src/C10.html`)
//! rebuilt from the real components, compared with its oracle by
//! `scripts/design-oracles/compare.sh C10`.

use components::design::theme::ThemeExt;
use components::design::type_scale::{self, Weight};
use components::design::{radius, size, space, stroke};
use components::icons::Icon;
use components::test_support::board::{BoardHeader, board, snapshot_board};
use components::{
    BladeFrame, BladeNavigation, Button, ButtonSize, ButtonVariant, blade_title,
    blade_title_with_actions, paint_blade_peek,
};
use egui::{Rect, Ui, pos2, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C10 · components::{Blade, BladeStack}",
    title: "Blade",
    description: "Right-anchored panel stack for inspecting and editing. Width 720 px (min 560, \
                  max 50% of the window), inset 8 px. Up to two history peeks show 96 px each \
                  and step down 16 px.",
};

/// The stage's top below the board's content top (the board puts it at 176).
const STAGE_OFFSET: f32 = 8.0;
const STAGE: egui::Vec2 = vec2(1440.0, 700.0);
/// Inset of the anatomy notes and the footer demo from the stage.
const NOTE_INSET: f32 = 40.0;
const NOTE_WIDTH: f32 = 420.0;
const NOTE_GAP: f32 = 6.0;
const NOTE_PADDING: f32 = space::S16;
/// The footer demo blade: its top within the stage.
const EDIT_TOP: f32 = 300.0;
/// The stack: peeks and active blade, 640 wide on this stage.
const STACK_BLADE_WIDTH: f32 = 640.0;
const FIRST_PEEK_LEFT: f32 = 600.0;

const NOTES: [(&str, &str); 3] = [
    (
        "Header · 56 px",
        "Back · Forward | Kind badge · Title (truncates) · More · Close",
    ),
    (
        "Body · 20 px / 24 px padding, scrolls",
        "Sections separated by 24 px; cards 16 px radius.",
    ),
    (
        "Footer · 64 px (optional)",
        "Only when the blade holds unsaved input.",
    ),
];

fn notes(ui: &mut Ui, origin: egui::Pos2) {
    let mut column = ui.new_child(
        egui::UiBuilder::new().max_rect(Rect::from_min_size(origin, vec2(NOTE_WIDTH, 400.0))),
    );
    column.spacing_mut().item_spacing = vec2(0.0, NOTE_GAP);
    for (title, detail) in NOTES {
        let theme = column.tokens();
        let top = column.cursor().top();
        let height = 2.0 * NOTE_PADDING + 2.0 * type_scale::META.line_height();
        let (card, _) = column.allocate_exact_size(vec2(NOTE_WIDTH, height), egui::Sense::hover());
        column
            .painter()
            .rect_filled(card, radius::CARD, theme.card_fill());
        let mut text = column.new_child(egui::UiBuilder::new().max_rect(
            Rect::from_min_size(pos2(card.left(), top), card.size()).shrink(NOTE_PADDING),
        ));
        text.spacing_mut().item_spacing = egui::Vec2::ZERO;
        type_scale::META
            .with_weight(Weight::SemiBold)
            .label(&mut text, title, theme.text.primary);
        type_scale::META.label(&mut text, detail, theme.text.secondary);
    }
}

fn ghost(icon: Icon, label: &str) -> Button {
    Button::icon(icon, label)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
}

fn body_text(ui: &mut Ui, text: &str) {
    type_scale::BODY.label(ui, text, ui.tokens().text.secondary);
}

fn c10_scene(ui: &mut Ui) {
    board(ui, HEADER, |ui| {
        let theme = ui.tokens();
        let stage = Rect::from_min_size(ui.cursor().min + vec2(0.0, STAGE_OFFSET), STAGE);
        ui.painter()
            .rect_filled(stage, radius::CARD, theme.surface.sunken);
        let inner = stage.shrink(size::HAIRLINE);
        let inner_radius = radius::CARD - size::HAIRLINE as u8;
        ui.painter()
            .rect_filled(inner, inner_radius, theme.shade.scrim);
        ui.painter().rect_stroke(
            stage,
            radius::CARD,
            stroke::divider(&theme),
            egui::StrokeKind::Inside,
        );
        let mut stage_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner));
        stage_ui.set_clip_rect(inner);
        let ui = &mut stage_ui;
        let at = |x: f32, y: f32| inner.min + vec2(x, y);

        notes(ui, at(NOTE_INSET, NOTE_INSET));

        BladeFrame::new(
            Rect::from_min_max(
                at(NOTE_INSET, EDIT_TOP),
                pos2(
                    inner.left() + NOTE_INSET + NOTE_WIDTH,
                    inner.bottom() - NOTE_INSET,
                ),
            ),
            "Edit context",
            BladeNavigation::new(true, false),
        )
        .show(
            ui,
            |ui| {
                blade_title(ui, None, "Edit context");
            },
            |ui| {
                body_text(
                    ui,
                    "Blades with unsaved input get a footer with the primary action on the right.",
                );
            },
            Some(&mut |ui: &mut Ui| {
                Button::primary("Save changes").show(ui);
                Button::secondary("Cancel").show(ui);
            }),
        );

        let shades = [theme.components.peek_shade_1, theme.components.peek_shade_2];
        let peeks = [
            ("Deployment", "coredns"),
            ("ReplicaSet", "coredns-66bc5c9577"),
        ];
        for (depth, ((kind, name), shade)) in peeks.into_iter().zip(shades).enumerate() {
            let step = (peeks.len() - depth) as f32 * size::BLADE_PEEK_STEP + space::BLADE_INSET;
            let left = FIRST_PEEK_LEFT + depth as f32 * size::BLADE_PEEK;
            let rect = Rect::from_min_max(
                at(left, step),
                pos2(
                    inner.left() + left + STACK_BLADE_WIDTH,
                    inner.bottom() - step,
                ),
            );
            paint_blade_peek(ui, rect, Some(kind), name, shade);
        }

        let left = FIRST_PEEK_LEFT + peeks.len() as f32 * size::BLADE_PEEK;
        BladeFrame::new(
            Rect::from_min_max(
                at(left, space::BLADE_INSET),
                pos2(
                    inner.left() + left + STACK_BLADE_WIDTH,
                    inner.bottom() - space::BLADE_INSET,
                ),
            ),
            "coredns-66bc5c9577-ffw2s",
            BladeNavigation::new(true, false),
        )
        .show(
            ui,
            |ui| {
                blade_title_with_actions(ui, Some("Pod"), "coredns-66bc5c9577-ffw2s", |ui| {
                    ghost(Icon::EllipsisHorizontal, "More actions").show(ui);
                });
            },
            |ui| {
                body_text(
                    ui,
                    "Active blade. Back returns to ReplicaSet; the peeks behind are clickable \
                     history entries.",
                );
            },
            None,
        );
    });
}

#[test]
fn c10_blade_board() {
    snapshot_board("c10_blade", c10_scene);
}
