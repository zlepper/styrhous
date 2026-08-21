use super::*;
use super::{interaction::*, transforms::*};

pub(super) fn show_header<H>(
    ui: &mut Ui,
    layer: BladeLayer,
    add_content: impl FnOnce(&mut Ui) -> H,
) -> (H, HeaderAction) {
    let header_height = 36.0;
    let navigation_width = 80.0;
    let close_width = 36.0;
    let mut action = HeaderAction::None;
    let header_width = ui.available_width();
    let content = ui
        .allocate_ui_with_layout(
            egui::vec2(header_width, header_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let mut content = None;
                StripBuilder::new(ui)
                    .size(Size::exact(navigation_width))
                    .size(Size::exact(spacing::MD))
                    .size(Size::remainder())
                    .size(Size::exact(spacing::MD))
                    .size(Size::exact(close_width))
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .horizontal(|mut strip| {
                        strip.cell(|ui| {
                            let back_clicked = ui
                                .add_enabled_ui(layer.can_go_back, |ui| {
                                    TailwindButton::icon(
                                        icons::arrow_left_icon()
                                            .fit_to_exact_size(egui::Vec2::splat(16.0))
                                            .tint(gray::_700),
                                    )
                                    .variant(ButtonVariant::Secondary)
                                    .size(ButtonSize::Sm)
                                    .accessibility_label(if layer.is_foreground {
                                        "Back"
                                    } else {
                                        "Back in background blade"
                                    })
                                    .show(ui)
                                    .clicked()
                                })
                                .inner;
                            if back_clicked {
                                action = HeaderAction::Back;
                            }
                            let forward_clicked = ui
                                .add_enabled_ui(layer.can_go_forward, |ui| {
                                    TailwindButton::icon(
                                        icons::arrow_right_icon()
                                            .fit_to_exact_size(egui::Vec2::splat(16.0))
                                            .tint(gray::_700),
                                    )
                                    .variant(ButtonVariant::Secondary)
                                    .size(ButtonSize::Sm)
                                    .accessibility_label(if layer.is_foreground {
                                        "Forward"
                                    } else {
                                        "Forward in background blade"
                                    })
                                    .show(ui)
                                    .clicked()
                                })
                                .inner;
                            if forward_clicked {
                                action = HeaderAction::Forward;
                            }
                        });
                        strip.empty();
                        strip.cell(|ui| content = Some(add_content(ui)));
                        strip.empty();
                        strip.cell(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if TailwindButton::icon(
                                        icons::x_mark_icon()
                                            .fit_to_exact_size(egui::Vec2::splat(16.0))
                                            .tint(gray::_700),
                                    )
                                    .variant(ButtonVariant::Secondary)
                                    .size(ButtonSize::Sm)
                                    .accessibility_label(if layer.is_foreground {
                                        "Close blade"
                                    } else {
                                        "Close background blade"
                                    })
                                    .show(ui)
                                    .clicked()
                                    {
                                        action = HeaderAction::Close;
                                    }
                                },
                            );
                        });
                    });
                content.expect("header content cell is always rendered")
            },
        )
        .inner;
    ui.add_space(spacing::LG);
    (content, action)
}
pub(super) fn show_layer<R>(
    ctx: &egui::Context,
    id: Id,
    viewport: Rect,
    transform: Transform,
    interactable: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let origin = active_transform(viewport).position;
    let visual = egui::emath::TSTransform::new(
        transform.position.to_vec2() - origin.to_vec2() * transform.scale,
        transform.scale,
    );
    egui::Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(origin)
        .fade_in(false)
        .interactable(interactable)
        .show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.set_height(height(viewport));
            ui.with_visual_transform(visual, |ui| {
                crate::scroll::vertical()
                    .id_salt(ui.id().with("scroll"))
                    .auto_shrink([false, false])
                    .min_scrolled_height(height(viewport))
                    .max_height(height(viewport))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        egui::Frame::new()
                            .fill(WHITE)
                            .stroke(egui::Stroke::new(FRAME_STROKE_WIDTH, gray::_200))
                            .shadow(egui::Shadow {
                                offset: [-4, 0],
                                blur: 16,
                                spread: 0,
                                color: Color32::BLACK.gamma_multiply(0.12),
                            })
                            .inner_margin(egui::Margin::same(PADDING))
                            .show(ui, |ui| {
                                // The frame owns the content geometry, so callers can
                                // draw their body without managing parent widths.
                                ui.set_width(CONTENT_WIDTH);
                                ui.set_min_height(height(viewport) - f32::from(PADDING) * 2.0);
                                add(ui)
                            })
                            .inner
                    })
                    .inner
            })
            .inner
        })
        .inner
}

/// Keep clipped history layers visible to egui without painting their contents.
///
/// An [`egui::Area`] that disappears for a frame is automatically promoted when
/// it returns. That would put an older history blade above newer history when
/// it re-enters the two-layer display cap. Retaining the area off-screen keeps
/// its established position in the foreground display stack.
pub(super) fn retain_hidden_layer(ctx: &egui::Context, id: Id, viewport: Rect) {
    egui::Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(egui::pos2(viewport.right() + INSET, viewport.top() + INSET))
        .fade_in(false)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_min_size(egui::Vec2::ZERO);
        });
}
pub(super) fn paint_scrim(ctx: &egui::Context, id: Id, viewport: Rect, closing: f32) {
    egui::Area::new(id.with("scrim"))
        .order(Order::Foreground)
        .fixed_pos(viewport.min)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_min_size(viewport.size());
            ui.painter().rect_filled(
                ui.max_rect(),
                0.0,
                Color32::BLACK.gamma_multiply(0.58 * (1.0 - closing)),
            );
        });
}
pub(super) fn show_input_scrim(
    ctx: &egui::Context,
    id: Id,
    viewport: Rect,
    active: Rect,
    history: &[(Id, Rect, usize)],
    promote: bool,
) -> (bool, Option<usize>) {
    let mut clicked = false;
    let mut history_selection = None;
    let regions = if active.intersects(viewport) {
        let active = active.intersect(viewport);
        vec![
            (
                "left",
                Rect::from_min_max(viewport.min, egui::pos2(active.min.x, viewport.max.y)),
            ),
            (
                "top",
                Rect::from_min_max(egui::pos2(active.min.x, viewport.min.y), active.min),
            ),
            (
                "bottom",
                Rect::from_min_max(
                    egui::pos2(active.min.x, active.max.y),
                    egui::pos2(active.max.x, viewport.max.y),
                ),
            ),
            (
                "right",
                Rect::from_min_max(egui::pos2(active.max.x, viewport.min.y), viewport.max),
            ),
        ]
    } else {
        vec![("full", viewport)]
    };
    for (name, region) in regions {
        if !region.is_positive() {
            continue;
        }
        let area_id = id.with(("input-scrim", name));
        let (dismissed, selection) = egui::Area::new(area_id)
            .order(Order::Foreground)
            .fixed_pos(region.min)
            .show(ctx, |ui| {
                ui.set_min_size(region.size());
                let dismissed = ui.interact(ui.max_rect(), ui.id().with("dismiss"), Sense::click());
                ui.ctx().accesskit_node_builder(dismissed.id, |builder| {
                    builder.set_label("Dismiss blade");
                });
                let mut selection = None;
                for (index, (content_id, rect, steps)) in history.iter().enumerate() {
                    let target = history_navigation_rect(active, history, *rect).intersect(region);
                    if !target.is_positive() {
                        continue;
                    }
                    let response = ui.interact(
                        target,
                        ui.id().with(("history-navigation", index, content_id)),
                        Sense::click(),
                    );
                    response.widget_info(|| {
                        WidgetInfo::labeled(
                            WidgetType::Button,
                            true,
                            history_navigation_label(*steps),
                        )
                    });
                    if response.clicked() {
                        selection = Some(*steps);
                    }
                }
                if selection.is_none() && dismissed.clicked() {
                    selection = ctx.input(|input| {
                        input.pointer.interact_pos().and_then(|position| {
                            history.iter().find_map(|(_, rect, steps)| {
                                history_navigation_rect(active, history, *rect)
                                    .contains(position)
                                    .then_some(*steps)
                            })
                        })
                    });
                }
                (dismissed.clicked() && selection.is_none(), selection)
            })
            .inner;
        clicked |= dismissed;
        history_selection = history_selection.or(selection);
        if promote {
            ctx.move_to_top(egui::LayerId::new(Order::Foreground, area_id));
        }
    }
    (clicked, history_selection)
}

pub(super) fn history_navigation_rect(
    active: Rect,
    history: &[(Id, Rect, usize)],
    rect: Rect,
) -> Rect {
    let right = history
        .iter()
        .filter(|(_, other, _)| other.min.x > rect.min.x)
        .map(|(_, other, _)| other.min.x)
        .chain(std::iter::once(active.min.x))
        .fold(rect.max.x, f32::min);
    Rect::from_min_max(rect.min, egui::pos2(right, rect.max.y))
}
