use super::*;
use super::{interaction::*, transforms::*};

/// The header of `layer` ([`chrome::show_header`]), its controls named for
/// a background blade when it is not in front.
pub(super) fn show_header<H>(
    ui: &mut Ui,
    layer: BladeLayer,
    add_content: impl FnOnce(&mut Ui) -> H,
) -> (H, HeaderAction) {
    let navigation = if layer.is_foreground {
        chrome::BladeNavigation::new(layer.can_go_back, layer.can_go_forward)
    } else {
        chrome::BladeNavigation {
            back_label: "Back in background blade",
            forward_label: "Forward in background blade",
            close_label: "Close background blade",
            ..chrome::BladeNavigation::new(layer.can_go_back, layer.can_go_forward)
        }
    };
    chrome::show_header(ui, navigation, add_content)
}

/// Show one blade of the stack: its surface, with `add` laying out its
/// header ([`show_header`]) and then its body ([`show_body`]).
pub(super) fn show_layer<R>(
    ctx: &egui::Context,
    id: Id,
    viewport: Rect,
    transform: Transform,
    interactable: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let width = blade_width(viewport);
    let blade_height = (height(viewport) - 2.0 * transform.inset).max(0.0);
    egui::Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(transform.position)
        // Blade positions are computed explicitly. egui's constraint would shift
        // the area when it is wider than the room to the right of its position,
        // desynchronising it from the history hit targets.
        .constrain(false)
        .fade_in(false)
        .interactable(interactable)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.set_height(blade_height);
            let rect = Rect::from_min_size(ui.cursor().min, egui::vec2(width, blade_height));
            chrome::paint_surface(ui, rect, ui.tokens().surface.panel);
            let mut blade = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            blade.set_clip_rect(rect.intersect(ui.clip_rect()));
            blade.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let result = theme::scope(&mut blade, ThemeScope::Blade, add);
            ui.allocate_rect(rect, Sense::hover());
            result
        })
        .inner
}

/// The body under a blade's header: the rest of the blade, scrolling, with
/// [`chrome::BODY_PADDING`] around a fixed-width column laid out by `add`.
pub(super) fn show_body<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let body_height = ui.available_height();
    let content_width = content_width(ui.available_width());
    crate::scroll::vertical()
        .id_salt(ui.id().with("scroll"))
        .auto_shrink([false, false])
        .min_scrolled_height(body_height)
        .max_height(body_height)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(chrome::BODY_PADDING)
                .show(ui, |ui| {
                    // The blade owns the body geometry, so callers can draw it
                    // without managing parent widths.
                    ui.set_width(content_width);
                    ui.set_min_height((body_height - chrome::BODY_PADDING.sum().y).max(0.0));
                    // The header is laid out without item spacing; the body
                    // keeps the spacing of the app's style.
                    ui.spacing_mut().item_spacing = ui.ctx().global_style().spacing.item_spacing;
                    show_clipped_body(ui, content_width, add)
                })
                .inner
        })
        .inner
}

/// Lay the body out in a fixed-width column and clip it to the blade.
///
/// Rows that cannot wrap may extend past the column, but only the column's
/// width is allocated, so they never resize the blade.
fn show_clipped_body<R>(ui: &mut Ui, content_width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let origin = ui.cursor().min;
    let column = Rect::from_min_size(origin, egui::vec2(content_width, ui.available_height()));
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(column));
    let padding = f32::from(chrome::BODY_PADDING.left);
    let clip = Rect::from_x_y_ranges(
        (origin.x - padding)..=(origin.x + content_width + padding),
        f32::NEG_INFINITY..=f32::INFINITY,
    );
    body.set_clip_rect(body.clip_rect().intersect(clip));
    let result = add(&mut body);
    ui.allocate_space(egui::vec2(content_width, body.min_rect().height()));
    result
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
        // Without this egui would pull the off-screen area back into view.
        .constrain(false)
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
                ui.tokens().shade.scrim.gamma_multiply(1.0 - closing),
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

/// Show a history blade as the peek it is: a sunken strip of
/// [`size::BLADE_PEEK`] points showing its `peek` and shaded by how far back
/// it is. `depth` 0 is the nearest history blade.
pub(super) fn show_peek_layer(
    ctx: &egui::Context,
    id: Id,
    viewport: Rect,
    transform: Transform,
    depth: usize,
    peek: &BladePeek,
) {
    let rect_size = transformed_rect(viewport, transform).size();
    egui::Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(transform.position)
        .constrain(false)
        .fade_in(false)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_min_size(rect_size);
            let rect = Rect::from_min_size(ui.min_rect().min, rect_size);
            let components = &ui.tokens().components;
            let shade = if depth == 0 {
                components.peek_shade_2
            } else {
                components.peek_shade_1
            };
            let mut blade = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            blade.set_clip_rect(rect.intersect(ui.clip_rect()));
            theme::scope(&mut blade, ThemeScope::Blade, |ui| {
                chrome::paint_blade_peek_on(
                    ui,
                    rect,
                    peek.kind
                        .as_deref()
                        .map(|kind| (kind, peek.category.map(|nth| ui.tokens().category.nth(nth)))),
                    &peek.name,
                    shade,
                );
            });
        });
}

/// The body of an inspector blade: all of the room under its header, for the
/// inspector to divide into its section rail and its own scrolling column.
pub(super) fn show_bare_body<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let rect = ui.available_rect_before_wrap();
    let mut body = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    body.set_clip_rect(rect.intersect(ui.clip_rect()));
    let result = add(&mut body);
    ui.allocate_rect(rect, Sense::hover());
    result
}
