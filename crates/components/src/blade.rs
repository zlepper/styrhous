//! Reusable right-side overlay blades with optional navigation history.

use crate::colors::{WHITE, gray};
use crate::design::spacing;
use crate::icons;
use crate::{ButtonSize, ButtonVariant, TailwindButton};
use egui::{Color32, Id, Order, Pos2, Rect, Sense, Ui};
use egui_extras::{Size, StripBuilder};

/// The fixed width used by every foreground and history blade.
pub const BLADE_WIDTH: f32 = 744.0;
const WIDTH: f32 = BLADE_WIDTH;
const INSET: f32 = 8.0;
const PADDING: i8 = spacing::XL as i8;
const FRAME_STROKE_WIDTH: f32 = 1.0;
const CONTENT_WIDTH: f32 = WIDTH - spacing::XL * 2.0 - FRAME_STROKE_WIDTH * 2.0;
const TRANSITION_DURATION: f32 = 0.25;
const HISTORY_SCALES: [f32; 2] = [0.9, 0.8];
/// Horizontal recession is proportional to the scale of every earlier blade,
/// matching the visual spacing of the inspector implementation this replaces.
const HISTORY_X_TRANSLATIONS: [f32; 2] = [
    1.0 / 3.0,
    (1.0 / 3.0) * (1.0 + HISTORY_SCALES[1] / HISTORY_SCALES[0]),
];

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum BladeTransition {
    Opening,
    Forward,
    Back,
    Closing,
}

#[derive(Debug, Clone)]
pub struct BladeNavigator<T> {
    current: T,
    back_stack: Vec<T>,
    forward_stack: Vec<T>,
    transition: Option<BladeTransition>,
    transition_started_at: Option<f64>,
}

impl<T> BladeNavigator<T> {
    pub fn new(current: T) -> Self {
        Self {
            current,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            transition: Some(BladeTransition::Opening),
            transition_started_at: None,
        }
    }
    pub fn current(&self) -> &T {
        &self.current
    }
    pub fn current_mut(&mut self) -> &mut T {
        &mut self.current
    }
    pub fn back_stack(&self) -> &[T] {
        &self.back_stack
    }
    pub fn forward_stack(&self) -> &[T] {
        &self.forward_stack
    }
    pub fn back_stack_mut(&mut self) -> &mut Vec<T> {
        &mut self.back_stack
    }
    pub fn forward_stack_mut(&mut self) -> &mut Vec<T> {
        &mut self.forward_stack
    }
    pub fn transition(&self) -> Option<BladeTransition> {
        self.transition
    }
    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }
    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }
    pub fn push(&mut self, next: T) -> Vec<T> {
        self.back_stack
            .push(std::mem::replace(&mut self.current, next));
        self.transition = Some(BladeTransition::Forward);
        self.transition_started_at = None;
        std::mem::take(&mut self.forward_stack)
    }
    pub fn go_back(&mut self) -> bool {
        let Some(previous) = self.back_stack.pop() else {
            return false;
        };
        self.forward_stack
            .push(std::mem::replace(&mut self.current, previous));
        self.transition = Some(BladeTransition::Back);
        self.transition_started_at = None;
        true
    }
    pub fn go_forward(&mut self) -> bool {
        let Some(next) = self.forward_stack.pop() else {
            return false;
        };
        self.back_stack
            .push(std::mem::replace(&mut self.current, next));
        self.transition = Some(BladeTransition::Forward);
        self.transition_started_at = None;
        true
    }
    pub fn begin_close(&mut self) -> bool {
        if matches!(self.transition, Some(BladeTransition::Closing)) {
            return false;
        }
        self.transition = Some(BladeTransition::Closing);
        self.transition_started_at = None;
        true
    }
    pub fn clear_transition(&mut self) {
        self.transition = None;
        self.transition_started_at = None;
    }
    pub fn entries(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.current)
            .chain(&self.back_stack)
            .chain(&self.forward_stack)
    }
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut T> {
        std::iter::once(&mut self.current)
            .chain(&mut self.back_stack)
            .chain(&mut self.forward_stack)
    }
    fn seed_transition(&mut self, ctx: &egui::Context) {
        if self.transition.is_some() && self.transition_started_at.is_none() {
            self.transition_started_at = Some(ctx.input(|input| input.time));
        }
    }
}

#[derive(Clone, Copy)]
pub struct BladeLayer {
    /// Stable content identity for the entry, independent of its visible stack position.
    pub content_id: Id,
    pub is_foreground: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

pub struct BladeResponse<H, R> {
    /// The custom header callback's result for the active blade.
    pub header: H,
    pub active: R,
    pub dismissed: bool,
    pub close_finished: bool,
}

pub struct BladeStack {
    id: Id,
}

impl BladeStack {
    pub fn new(id_source: impl std::hash::Hash) -> Self {
        Self {
            id: Id::new(id_source),
        }
    }

    /// The fixed outer width used for every blade layer and its content frame.
    pub const fn width(&self) -> f32 {
        WIDTH
    }

    pub fn seed_transition<T>(&self, ctx: &egui::Context, navigator: &mut BladeNavigator<T>) {
        navigator.seed_transition(ctx);
    }

    /// Render a blade with caller-provided header content. The shared chrome
    /// always supplies Back, Forward, and Close controls around that content.
    pub fn show<T, H, R>(
        &self,
        ctx: &egui::Context,
        navigator: &mut BladeNavigator<T>,
        item_id: impl Fn(&T) -> Id,
        mut render_header: impl FnMut(&mut Ui, &mut T, BladeLayer) -> H,
        mut render_content: impl FnMut(&mut Ui, &mut T, BladeLayer) -> R,
    ) -> BladeResponse<H, R> {
        let viewport = ctx.content_rect();
        navigator.seed_transition(ctx);
        let transition = navigator
            .transition
            .map(|kind| (kind, progress(ctx, navigator)));
        let closing_progress = matches!(transition, Some((BladeTransition::Closing, _)))
            .then(|| transition.expect("transition exists").1)
            .unwrap_or_default();
        paint_scrim(ctx, self.id, viewport, closing_progress);

        let history_len = navigator.back_stack.len();
        let first_history = history_len.saturating_sub(HISTORY_SCALES.len());
        for (index, entry) in navigator.back_stack_mut()[first_history..]
            .iter_mut()
            .enumerate()
        {
            let depth = history_len - first_history - index - 1;
            let transform = history_layer_transform(viewport, depth, transition);
            let content_id = item_id(entry);
            show_layer(
                ctx,
                self.layer_id(content_id),
                viewport,
                transform,
                false,
                |ui| {
                    let layer = BladeLayer {
                        content_id,
                        is_foreground: false,
                        can_go_back: first_history + index > 0,
                        can_go_forward: true,
                    };
                    let _ = show_header(ui, layer, |ui| render_header(ui, entry, layer));
                    render_content(ui, entry, layer);
                },
            );
        }

        if let Some((BladeTransition::Back, value)) = transition
            && value < 1.0
        {
            let can_go_forward = navigator.forward_stack().len() > 1;
            if let Some(entry) = navigator.forward_stack_mut().last_mut() {
                let content_id = item_id(entry);
                show_layer(
                    ctx,
                    self.layer_id(content_id),
                    viewport,
                    Transform {
                        position: active_transform(viewport).position
                            + egui::vec2(value * (WIDTH + INSET * 2.0), 0.0),
                        scale: 1.0,
                    },
                    false,
                    |ui| {
                        let layer = BladeLayer {
                            content_id,
                            is_foreground: false,
                            can_go_back: true,
                            can_go_forward,
                        };
                        let _ = show_header(ui, layer, |ui| render_header(ui, entry, layer));
                        render_content(ui, entry, layer);
                    },
                );
            }
        }

        let active_blade_transform = match transition {
            Some((BladeTransition::Opening | BladeTransition::Forward, value)) => Transform {
                position: active_transform(viewport).position
                    + egui::vec2((1.0 - value) * WIDTH, 0.0),
                scale: 1.0,
            },
            Some((BladeTransition::Back, value)) => interpolate(
                history_transform(viewport, 0),
                active_transform(viewport),
                value,
            ),
            Some((BladeTransition::Closing, value)) => {
                closing_transform(viewport, active_transform(viewport), value)
            }
            None => active_transform(viewport),
        };
        let active_rect = Rect::from_min_size(
            active_blade_transform.position,
            egui::vec2(WIDTH, height(viewport)) * active_blade_transform.scale,
        );
        let outgoing = matches!(transition, Some((BladeTransition::Back, value)) if value < 1.0);
        let active_content_id = item_id(navigator.current());
        let active_area_id = self.layer_id(active_content_id);
        let (header, active, header_action) = show_layer(
            ctx,
            active_area_id,
            viewport,
            active_blade_transform,
            !matches!(transition, Some((BladeTransition::Closing, _)))
                && !outgoing
                && active_blade_transform == active_transform(viewport),
            |ui| {
                let can_go_back = navigator.can_go_back();
                let can_go_forward = navigator.can_go_forward();
                let layer = BladeLayer {
                    content_id: active_content_id,
                    is_foreground: true,
                    can_go_back,
                    can_go_forward,
                };
                let (header, header_action) = show_header(ui, layer, |ui| {
                    render_header(ui, navigator.current_mut(), layer)
                });
                let active = render_content(ui, navigator.current_mut(), layer);
                (header, active, header_action)
            },
        );
        match header_action {
            HeaderAction::Back => {
                navigator.go_back();
            }
            HeaderAction::Forward => {
                navigator.go_forward();
            }
            HeaderAction::Close => {
                navigator.begin_close();
            }
            HeaderAction::None => {}
        }
        if should_promote_active_blade(history_len > 0, transition) {
            ctx.move_to_top(egui::LayerId::new(Order::Foreground, active_area_id));
        }
        let dismissed = show_input_scrim(ctx, self.id, viewport, active_rect);
        BladeResponse {
            header,
            active,
            dismissed,
            close_finished: matches!(transition, Some((BladeTransition::Closing, value)) if value >= 1.0),
        }
    }

    /// Render a blade with the shared title treatment.
    pub fn show_with_title<T, R>(
        &self,
        ctx: &egui::Context,
        navigator: &mut BladeNavigator<T>,
        item_id: impl Fn(&T) -> Id,
        title: impl Fn(&T) -> String,
        render_content: impl FnMut(&mut Ui, &mut T, BladeLayer) -> R,
    ) -> BladeResponse<(), R> {
        self.show(
            ctx,
            navigator,
            item_id,
            |ui, entry, _| {
                ui.label(
                    egui::RichText::new(title(entry))
                        .font(crate::design::typography::page_title())
                        .color(gray::_900),
                );
            },
            render_content,
        )
    }

    fn layer_id(&self, content_id: Id) -> Id {
        self.id.with(("blade", content_id))
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Transform {
    position: Pos2,
    scale: f32,
}
fn duration(ctx: &egui::Context) -> f32 {
    if ctx.style().animation_time == 0.0 {
        0.0
    } else {
        TRANSITION_DURATION
    }
}
fn progress<T>(ctx: &egui::Context, navigator: &BladeNavigator<T>) -> f32 {
    let duration = duration(ctx);
    if duration == 0.0 {
        return 1.0;
    }
    let now = ctx.input(|input| input.time);
    let started_at = navigator.transition_started_at.unwrap_or(now);
    let linear_progress = ((now - started_at) / f64::from(duration)).clamp(0.0, 1.0) as f32;
    if linear_progress < 1.0 {
        ctx.request_repaint();
    }
    egui::emath::easing::cubic_in_out(linear_progress)
}
fn height(viewport: Rect) -> f32 {
    viewport.height() - INSET * 2.0
}
fn active_transform(viewport: Rect) -> Transform {
    Transform {
        position: egui::pos2(viewport.right() - WIDTH - INSET, viewport.top() + INSET),
        scale: 1.0,
    }
}
fn history_transform(viewport: Rect, index: usize) -> Transform {
    let depth = index.min(HISTORY_SCALES.len() - 1);
    let scale = HISTORY_SCALES[depth];
    let h = height(viewport);
    Transform {
        position: egui::pos2(
            active_transform(viewport).position.x - WIDTH * HISTORY_X_TRANSLATIONS[depth],
            viewport.top() + INSET + (h - h * scale) / 2.0,
        ),
        scale,
    }
}
fn should_promote_active_blade(
    has_history_layers: bool,
    transition: Option<(BladeTransition, f32)>,
) -> bool {
    has_history_layers
        && !matches!(
            transition,
            Some((BladeTransition::Back, progress)) if progress < 1.0
        )
}
fn history_layer_transform(
    viewport: Rect,
    depth: usize,
    transition: Option<(BladeTransition, f32)>,
) -> Transform {
    let target = history_transform(viewport, depth);
    match transition {
        Some((BladeTransition::Closing, progress)) => closing_transform(viewport, target, progress),
        Some((BladeTransition::Forward, progress)) => {
            let start = if depth == 0 {
                active_transform(viewport)
            } else {
                history_transform(viewport, depth - 1)
            };
            interpolate(start, target, progress)
        }
        Some((BladeTransition::Back, progress)) => {
            interpolate(history_transform(viewport, depth + 1), target, progress)
        }
        Some((BladeTransition::Opening, _)) | None => target,
    }
}
fn closing_transform(viewport: Rect, transform: Transform, progress: f32) -> Transform {
    Transform {
        position: egui::pos2(
            egui::lerp(transform.position.x..=viewport.right() + INSET, progress),
            transform.position.y,
        ),
        scale: transform.scale,
    }
}
fn interpolate(from: Transform, to: Transform, value: f32) -> Transform {
    Transform {
        position: from.position + (to.position - from.position) * value,
        scale: from.scale + (to.scale - from.scale) * value,
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum HeaderAction {
    None,
    Back,
    Forward,
    Close,
}

fn show_header<H>(
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
fn show_layer<R>(
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
                egui::ScrollArea::vertical()
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
fn paint_scrim(ctx: &egui::Context, id: Id, viewport: Rect, closing: f32) {
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
fn show_input_scrim(ctx: &egui::Context, id: Id, viewport: Rect, active: Rect) -> bool {
    let mut clicked = false;
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
        clicked |= egui::Area::new(area_id)
            .order(Order::Foreground)
            .fixed_pos(region.min)
            .show(ctx, |ui| {
                ui.set_min_size(region.size());
                ui.interact(ui.max_rect(), ui.id().with("dismiss"), Sense::click())
                    .clicked()
            })
            .inner;
        ctx.move_to_top(egui::LayerId::new(Order::Foreground, area_id));
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;
    use egui_kittest::{Harness, SnapshotOptions};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct TestBlade {
        id: u64,
        title: &'static str,
    }

    fn render_test_blade(ui: &mut Ui, blade: &mut TestBlade, layer: BladeLayer) {
        ui.label(format!(
            "{} · {}",
            if layer.is_foreground {
                "Active"
            } else {
                "History"
            },
            blade.title
        ));
        ui.label(format!(
            "back: {} · forward: {}",
            layer.can_go_back, layer.can_go_forward
        ));
    }

    // WGPU produces up to 1.84634 YIQ-squared variance at transformed shadow edges.
    // Keep the tolerance below the first meaningful visual difference.
    fn transformed_blade_snapshot_options() -> SnapshotOptions {
        SnapshotOptions::new().threshold(2.1)
    }

    #[test]
    fn snapshots_a_single_blade_and_its_visible_history() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-component-snapshot");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| Id::new(blade.id),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.set_size(egui::vec2(1536.0, 1024.0));
        harness.run();
        harness.snapshot_options("blades/single", &transformed_blade_snapshot_options());

        navigator.borrow_mut().push(TestBlade {
            id: 2,
            title: "Second",
        });
        navigator.borrow_mut().push(TestBlade {
            id: 3,
            title: "Third",
        });
        navigator.borrow_mut().clear_transition();
        harness.run();
        harness.snapshot_options("blades/history", &transformed_blade_snapshot_options());

        navigator.borrow_mut().push(TestBlade {
            id: 4,
            title: "Fourth",
        });
        navigator.borrow_mut().clear_transition();
        harness.run();
        harness.snapshot_options("blades/history_cap", &transformed_blade_snapshot_options());
    }

    #[test]
    fn snapshots_opening_and_forward_animation_frames() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        let stack = BladeStack::new("blade-animation-snapshot");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| Id::new(blade.id),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.ctx.style_mut(|style| style.animation_time = 1.0);
        harness.set_size(egui::vec2(1536.0, 1024.0));

        // Harness construction renders once before the test can configure the
        // clock. Restart this transition so the frames below use only our
        // explicit timestamps.
        {
            let mut navigator = navigator.borrow_mut();
            navigator.transition = Some(BladeTransition::Opening);
            navigator.transition_started_at = None;
        }
        harness.input_mut().time = Some(1.0);
        harness.step();
        harness.snapshot_options(
            "blades/opening_first_frame",
            &transformed_blade_snapshot_options(),
        );
        harness.input_mut().time = Some(1.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness.snapshot_options(
            "blades/opening_mid_frame",
            &transformed_blade_snapshot_options(),
        );
        harness.input_mut().time = Some(1.0 + f64::from(TRANSITION_DURATION));
        harness.step();
        harness.snapshot_options(
            "blades/opening_final_frame",
            &transformed_blade_snapshot_options(),
        );

        navigator.borrow_mut().push(TestBlade {
            id: 2,
            title: "Second",
        });
        harness.input_mut().time = Some(20.0);
        harness.step();
        harness.snapshot_options(
            "blades/forward_first_frame",
            &transformed_blade_snapshot_options(),
        );
        harness.input_mut().time = Some(20.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness.snapshot_options(
            "blades/forward_mid_frame",
            &transformed_blade_snapshot_options(),
        );

        assert!(navigator.borrow_mut().go_back());
        harness.input_mut().time = Some(30.0);
        harness.step();
        harness.snapshot_options(
            "blades/back_first_frame",
            &transformed_blade_snapshot_options(),
        );
        harness.input_mut().time = Some(30.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness.snapshot_options(
            "blades/back_mid_frame",
            &transformed_blade_snapshot_options(),
        );
    }

    #[test]
    fn snapshots_custom_header_content_with_shared_controls() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-custom-header-snapshot");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| Id::new(blade.id),
                |ui, blade, _| {
                    ui.label(egui::RichText::new(format!("Custom: {}", blade.title)).strong());
                },
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.set_size(egui::vec2(1536.0, 1024.0));
        harness.run();
        harness.snapshot_options(
            "blades/custom_header",
            &transformed_blade_snapshot_options(),
        );
    }

    #[test]
    fn only_the_two_most_recent_history_blades_are_rendered() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let stack = BladeStack::new("blade-history-cap");
        let navigator_for_ui = Rc::clone(&navigator);
        let rendered_for_ui = Rc::clone(&rendered);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| Id::new(blade.id),
                |blade| blade.title.to_owned(),
                |ui, blade, layer| {
                    rendered_for_ui.borrow_mut().push(blade.id);
                    render_test_blade(ui, blade, layer);
                },
            );
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.set_size(egui::vec2(1536.0, 1024.0));
        harness.run();
        for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
            navigator.borrow_mut().push(TestBlade { id, title });
        }
        navigator.borrow_mut().clear_transition();
        rendered.borrow_mut().clear();
        harness.run();

        let rendered = rendered.borrow();
        assert!(!rendered.contains(&1), "the oldest blade must be hidden");
        assert!(
            rendered.chunks_exact(3).all(|frame| frame == [2, 3, 4]),
            "only the two newest history blades and the active blade should render: {rendered:?}"
        );
    }

    #[test]
    fn stable_content_ids_preserve_child_state_through_history_navigation() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let initialized = Rc::new(RefCell::new(Vec::new()));
        let stack = BladeStack::new("blade-child-state");
        let navigator_for_ui = Rc::clone(&navigator);
        let initialized_for_ui = Rc::clone(&initialized);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| Id::new(blade.id),
                |blade| blade.title.to_owned(),
                |ui, blade, layer| {
                    let state_id = layer.content_id.with("child-state");
                    if !ui
                        .ctx()
                        .data(|data| data.get_temp::<bool>(state_id).unwrap_or(false))
                    {
                        ui.ctx().data_mut(|data| data.insert_temp(state_id, true));
                        initialized_for_ui.borrow_mut().push(blade.id);
                    }
                    render_test_blade(ui, blade, layer);
                },
            );
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();
        navigator.borrow_mut().push(TestBlade {
            id: 2,
            title: "Second",
        });
        navigator.borrow_mut().clear_transition();
        harness.run();
        assert!(navigator.borrow_mut().go_back());
        navigator.borrow_mut().clear_transition();
        harness.run();
        assert!(navigator.borrow_mut().go_forward());
        navigator.borrow_mut().clear_transition();
        harness.run();

        assert_eq!(&*initialized.borrow(), &[1, 2]);
    }

    #[test]
    fn hidden_history_blades_restore_their_existing_child_state() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let initialized = Rc::new(RefCell::new(Vec::new()));
        let stack = BladeStack::new("blade-hidden-child-state");
        let navigator_for_ui = Rc::clone(&navigator);
        let initialized_for_ui = Rc::clone(&initialized);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| Id::new(blade.id),
                |blade| blade.title.to_owned(),
                |ui, blade, layer| {
                    let state_id = layer.content_id.with("child-state");
                    if !ui
                        .ctx()
                        .data(|data| data.get_temp::<bool>(state_id).unwrap_or(false))
                    {
                        ui.ctx().data_mut(|data| data.insert_temp(state_id, true));
                        initialized_for_ui.borrow_mut().push(blade.id);
                    }
                    render_test_blade(ui, blade, layer);
                },
            );
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.run();

        for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.run();
        }
        assert_eq!(navigator.borrow().current().id, 4);
        assert_eq!(&*initialized.borrow(), &[1, 2, 3, 4]);

        for _ in 0..3 {
            assert!(navigator.borrow_mut().go_back());
            navigator.borrow_mut().clear_transition();
            harness.run();
        }

        assert_eq!(navigator.borrow().current().id, 1);
        assert_eq!(
            &*initialized.borrow(),
            &[1, 2, 3, 4],
            "returning to a hidden entry must use its original egui content id"
        );
    }
    #[test]
    fn navigator_restores_entries_and_discards_forward_history() {
        let mut navigator = BladeNavigator::new("one");
        assert!(navigator.push("two").is_empty());
        assert!(navigator.go_back());
        assert_eq!(navigator.current(), &"one");
        assert_eq!(navigator.push("three"), vec!["two"]);
        assert!(!navigator.can_go_forward());
    }

    #[test]
    fn shared_header_controls_navigate_and_close_the_active_blade() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        navigator.borrow_mut().push(TestBlade {
            id: 2,
            title: "Second",
        });
        navigator.borrow_mut().clear_transition();
        let close_finished = Rc::new(RefCell::new(false));
        let stack = BladeStack::new("blade-shared-header-controls");
        let navigator_for_ui = Rc::clone(&navigator);
        let close_finished_for_ui = Rc::clone(&close_finished);
        let mut harness = Harness::new_ui(move |ui| {
            let response = stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| Id::new(blade.id),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
            *close_finished_for_ui.borrow_mut() = response.close_finished;
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.set_size(egui::vec2(1536.0, 1024.0));
        harness.run();

        let back = harness.get_by_label("Back").rect();
        let close = harness.get_by_label("Close blade").rect();
        assert_eq!(
            close.right() - back.left(),
            CONTENT_WIDTH,
            "back: {back:?}, close: {close:?}"
        );

        harness.get_by_label("Back").click_accesskit();
        harness.run();
        assert_eq!(navigator.borrow().current().id, 1);

        harness.get_by_label("Forward").click_accesskit();
        harness.run();
        assert_eq!(navigator.borrow().current().id, 2);

        harness.get_by_label("Close blade").click_accesskit();
        harness.run();
        assert!(*close_finished.borrow());
    }
    #[test]
    fn closing_is_idempotent() {
        let mut navigator = BladeNavigator::new(());
        assert!(navigator.begin_close());
        assert!(!navigator.begin_close());
    }

    #[test]
    fn exposes_the_shared_blade_width() {
        assert_eq!(BladeStack::new("blade-width").width(), BLADE_WIDTH);
    }

    #[test]
    fn body_receives_the_fixed_blade_content_width() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-content-width");
        let navigator_for_ui = Rc::clone(&navigator);
        let observed_width = Rc::new(RefCell::new(None));
        let observed_width_for_ui = Rc::clone(&observed_width);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| Id::new(blade.id),
                |blade| blade.title.to_owned(),
                |ui, _, _| {
                    *observed_width_for_ui.borrow_mut() = Some(ui.available_width());
                },
            );
        });
        crate::test_support::setup_egui(&harness.ctx);
        harness.set_size(egui::vec2(1536.0, 1024.0));
        harness.run();

        assert_eq!(*observed_width.borrow(), Some(CONTENT_WIDTH));
    }
}
