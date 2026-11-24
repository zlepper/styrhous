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
    blade_title_with_actions,
};
use egui::{Rect, Ui, pos2, vec2};

const HEADER: BoardHeader<'static> = BoardHeader {
    eyebrow: "C10 · components::{Blade, BladeStack}",
    title: "Blade",
    description: "Right-anchored panel stack for inspecting and editing. Width 720 px (min 560, \
                  max 50% of the window), inset 8 px. Up to two history blades fan out behind it, \
                  at 90% and 80% scale, showing their real content.",
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
/// The stack: back blades and the active blade, 640 wide on this stage.
const STACK_BLADE_WIDTH: f32 = 640.0;
const ACTIVE_LEFT: f32 = 792.0;

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

/// Draw one blade of the stack in a layer of its own, as the live stack does: the
/// blades overlap by design, and the demo blade behind them is another layer.
fn stack_layer(ui: &Ui, clip: Rect, index: usize, add: impl FnOnce(&mut Ui)) {
    egui::Area::new(egui::Id::new(("c10-stack", index)))
        .order(egui::Order::Foreground)
        .fixed_pos(clip.min)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            ui.set_clip_rect(clip);
            add(ui);
        });
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

        let back_blades = [
            (
                "Deployment",
                "coredns",
                "Deployment coredns keeps two replicas running.",
            ),
            (
                "ReplicaSet",
                "coredns-66bc5c9577",
                "ReplicaSet coredns-66bc5c9577 owns the pod.",
            ),
        ];
        // The nearest back blade is the last one; each sits further left and is smaller,
        // so the furthest is painted first.
        let height = inner.height() - 2.0 * space::BLADE_INSET;
        for (depth, (kind, name, body)) in back_blades
            .into_iter()
            .rev()
            .enumerate()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let scale = size::BLADE_HISTORY_SCALES[depth];
            let recession: f32 = (0..=depth)
                .map(|step| {
                    size::BLADE_HISTORY_RECESSION
                        * size::BLADE_HISTORY_SCALES[step]
                        * (1.0 / size::BLADE_HISTORY_SCALES[0])
                })
                .sum();
            let natural = Rect::from_min_size(
                at(
                    ACTIVE_LEFT - STACK_BLADE_WIDTH * recession,
                    space::BLADE_INSET,
                ),
                vec2(STACK_BLADE_WIDTH, height),
            );
            let visible_top = natural.top() + height * (1.0 - scale) / 2.0;
            let transform = egui::emath::TSTransform::new(
                pos2(natural.left(), visible_top).to_vec2() - natural.min.to_vec2() * scale,
                scale,
            );
            stack_layer(ui, inner, depth, |ui| {
                ui.with_visual_transform(transform, |ui| {
                    BladeFrame::new(natural, name, BladeNavigation::new(depth == 0, true)).show(
                        ui,
                        |ui| {
                            blade_title(ui, Some(kind), name);
                        },
                        |ui| body_text(ui, body),
                        None,
                    );
                });
            });
        }

        let left = ACTIVE_LEFT;
        stack_layer(ui, inner, back_blades.len(), |ui| {
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
                        "Active blade. Back returns to ReplicaSet; the blades behind are clickable \
                     history entries.",
                    );
                },
                None,
            );
        });
    });
}

#[test]
fn c10_blade_board() {
    snapshot_board("c10_blade", c10_scene);
}
