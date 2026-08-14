//! Reusable right-side overlay blades with optional navigation history.

use crate::colors::{WHITE, gray};
use crate::design::spacing;
use crate::icons;
use crate::{ButtonSize, ButtonVariant, TailwindButton};
use egui::{Color32, Id, Order, Pos2, Rect, Sense, Ui, WidgetInfo, WidgetType};
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
    back_steps: usize,
}

impl<T> BladeNavigator<T> {
    pub fn new(current: T) -> Self {
        Self {
            current,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            transition: Some(BladeTransition::Opening),
            transition_started_at: None,
            back_steps: 0,
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
        self.back_steps = 0;
        std::mem::take(&mut self.forward_stack)
    }
    pub fn go_back(&mut self) -> bool {
        self.go_back_steps(1)
    }
    /// Move directly to an earlier entry in the back history.
    ///
    /// The resulting transition promotes the selected entry in one animation,
    /// rather than playing an animation for every intermediate entry.
    pub fn go_back_steps(&mut self, steps: usize) -> bool {
        if steps == 0 || steps > self.back_stack.len() {
            return false;
        }
        for _ in 0..steps {
            let previous = self
                .back_stack
                .pop()
                .expect("step count was checked against the back stack");
            self.forward_stack
                .push(std::mem::replace(&mut self.current, previous));
        }
        self.transition = Some(BladeTransition::Back);
        self.transition_started_at = None;
        self.back_steps = steps;
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
        self.back_steps = 0;
        true
    }
    pub fn begin_close(&mut self) -> bool {
        if matches!(self.transition, Some(BladeTransition::Closing)) {
            return false;
        }
        self.transition = Some(BladeTransition::Closing);
        self.transition_started_at = None;
        self.back_steps = 0;
        true
    }
    pub fn clear_transition(&mut self) {
        self.transition = None;
        self.transition_started_at = None;
        self.back_steps = 0;
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

    /// Consume the navigator and return every entry it owns.  This lets a
    /// parent coordinator perform lifecycle cleanup when it replaces or
    /// closes an entire stack.
    pub fn into_entries(self) -> impl Iterator<Item = T> {
        std::iter::once(self.current)
            .chain(self.back_stack)
            .chain(self.forward_stack)
    }
    fn seed_transition(&mut self, ctx: &egui::Context) {
        if self.transition.is_some() && self.transition_started_at.is_none() {
            self.transition_started_at = Some(ctx.input(|input| input.time));
        }
    }

    fn back_steps(&self) -> usize {
        self.back_steps.max(1)
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

#[derive(Clone)]
pub struct BladeStack {
    id: Id,
}

impl BladeStack {
    pub fn new(id_source: impl std::hash::Hash + std::fmt::Debug) -> Self {
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
        mut render_header: impl FnMut(&mut Ui, &mut T, BladeLayer) -> H,
        mut render_content: impl FnMut(&mut Ui, &mut T, BladeLayer) -> R,
    ) -> BladeResponse<H, R> {
        let viewport = ctx.content_rect();
        navigator.seed_transition(ctx);
        let transition = navigator
            .transition
            .map(|kind| (kind, progress(ctx, navigator)));
        let transition_back_steps = matches!(transition, Some((BladeTransition::Back, _)))
            .then(|| navigator.back_steps())
            .unwrap_or(1);
        let history_len = navigator.back_stack.len();
        let is_closing = matches!(transition, Some((BladeTransition::Closing, _)));
        let active_interactable =
            !is_closing && !matches!(transition, Some((_, progress)) if progress < 1.0);
        let mouse_navigation = mouse_navigation_action(ctx, self.id, is_closing);
        let closing_progress = matches!(transition, Some((BladeTransition::Closing, _)))
            .then(|| transition.expect("transition exists").1)
            .unwrap_or_default();
        paint_scrim(ctx, self.id, viewport, closing_progress);

        let first_history = history_len.saturating_sub(HISTORY_SCALES.len());
        for stack_index in 0..first_history {
            retain_hidden_layer(ctx, self.layer_id(stack_index), viewport);
        }
        if let Some((BladeTransition::Forward, value)) = transition
            && value < 1.0
            && first_history > 0
        {
            let stack_index = first_history - 1;
            let content_id = self.content_id(stack_index);
            let entry = &mut navigator.back_stack_mut()[stack_index];
            show_layer(
                ctx,
                self.layer_id(stack_index),
                viewport,
                history_transform(viewport, HISTORY_SCALES.len() - 1),
                false,
                |ui| {
                    let layer = BladeLayer {
                        content_id,
                        is_foreground: false,
                        can_go_back: stack_index > 0,
                        can_go_forward: true,
                    };
                    let _ = show_header(ui, layer, |ui| render_header(ui, entry, layer));
                    render_content(ui, entry, layer);
                },
            );
        }

        let mut history_targets = Vec::new();
        for (index, entry) in navigator.back_stack_mut()[first_history..]
            .iter_mut()
            .enumerate()
        {
            let stack_index = first_history + index;
            let depth = history_len - first_history - index - 1;
            let transform =
                history_layer_transform(viewport, depth, transition, transition_back_steps);
            let content_id = self.content_id(stack_index);
            let steps = depth + 1;
            if active_interactable {
                history_targets.push((content_id, transformed_rect(viewport, transform), steps));
            }
            show_layer(
                ctx,
                self.layer_id(stack_index),
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
            let can_go_forward = navigator.forward_stack().len() > transition_back_steps;
            for (index, entry) in navigator
                .forward_stack_mut()
                .iter_mut()
                .rev()
                .take(transition_back_steps)
                .enumerate()
            {
                let stack_index = history_len + 1 + index;
                let content_id = self.content_id(stack_index);
                let start = if index + 1 < transition_back_steps {
                    history_transform(viewport, index)
                } else {
                    active_transform(viewport)
                };
                show_layer(
                    ctx,
                    self.layer_id(stack_index),
                    viewport,
                    closing_transform(viewport, start, value),
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
                history_transform(viewport, transition_back_steps - 1),
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
        let active_content_id = self.content_id(history_len);
        let active_area_id = self.layer_id(history_len);
        let popup_was_open = egui::Popup::is_any_open(ctx);
        let (scrim_dismissed, scrim_history_selection) = show_input_scrim(
            ctx,
            self.id,
            viewport,
            active_rect,
            &history_targets,
            !popup_was_open,
        );
        let opening_popup = ctx.input(|input| input.pointer.any_click());
        if !popup_was_open
            && !opening_popup
            && should_promote_active_blade(history_len > 0, transition)
        {
            ctx.move_to_top(egui::LayerId::new(Order::Foreground, active_area_id));
        }
        let (header, active, header_action) = show_layer(
            ctx,
            active_area_id,
            viewport,
            active_blade_transform,
            active_interactable,
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
        // The input scrim keeps workspace and history layers from receiving a
        // click outside a popup. While a popup is open its click is intentionally
        // ignored here: egui closes the popup, while the scrim prevents the click
        // from reaching the covered layer or dismissing the blade.
        let (dismissed, history_selection) = if popup_was_open {
            (false, None)
        } else {
            (scrim_dismissed, scrim_history_selection)
        };
        match if header_action == HeaderAction::None {
            mouse_navigation
        } else {
            header_action
        } {
            HeaderAction::Back => {
                navigator.go_back();
            }
            HeaderAction::Forward => {
                navigator.go_forward();
            }
            HeaderAction::Close => {
                navigator.begin_close();
            }
            HeaderAction::None => {
                if let Some(steps) = history_selection {
                    navigator.go_back_steps(steps);
                }
            }
        }
        record_topmost_blade_stack(ctx, self.id, !is_closing);
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
        title: impl Fn(&T) -> String,
        render_content: impl FnMut(&mut Ui, &mut T, BladeLayer) -> R,
    ) -> BladeResponse<(), R> {
        self.show(
            ctx,
            navigator,
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

    fn content_id(&self, stack_index: usize) -> Id {
        self.id.with(("blade-content", stack_index))
    }

    fn layer_id(&self, stack_index: usize) -> Id {
        self.id.with(("blade", stack_index))
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Transform {
    position: Pos2,
    scale: f32,
}
fn duration(ctx: &egui::Context) -> f32 {
    if ctx.global_style().animation_time == 0.0 {
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
    back_steps: usize,
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
        Some((BladeTransition::Back, progress)) => interpolate(
            history_transform(viewport, depth + back_steps),
            target,
            progress,
        ),
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
fn transformed_rect(viewport: Rect, transform: Transform) -> Rect {
    Rect::from_min_size(
        transform.position,
        egui::vec2(WIDTH, height(viewport)) * transform.scale,
    )
}
fn history_navigation_label(steps: usize) -> String {
    match steps {
        1 => "Go back one blade".to_owned(),
        2 => "Go back two blades".to_owned(),
        _ => format!("Go back {steps} blades"),
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum HeaderAction {
    None,
    Back,
    Forward,
    Close,
}

/// Dispatch a side-button press to the stack that was topmost in the event
/// pass. The subsequent transition immediately replaces any animation in
/// progress, while presses on a closing stack are discarded.
fn mouse_navigation_action(ctx: &egui::Context, stack_id: Id, is_closing: bool) -> HeaderAction {
    let pass = ctx.cumulative_pass_nr();
    let pending_action = ctx.data_mut(|data| {
        let mut state = data
            .get_temp::<MouseNavigationState>(topmost_blade_stack_id())
            .unwrap_or_default();
        if state.pass != pass {
            state.pending = state.current.zip(state.captured.take());
            state.current = None;
            state.pass = pass;
        }
        let action = if state.pending.map(|(id, _)| id) != Some(stack_id) {
            HeaderAction::None
        } else if is_closing {
            state.pending.take();
            HeaderAction::None
        } else {
            state
                .pending
                .take()
                .expect("pending action belongs to this stack")
                .1
        };
        data.insert_temp(topmost_blade_stack_id(), state);
        action
    });
    let should_capture = ctx.data(|data| {
        data.get_temp::<MouseNavigationState>(topmost_blade_stack_id())
            .is_none_or(|state| state.captured.is_none())
    });
    let captured_action = should_capture
        .then(|| {
            ctx.input_mut(|input| {
                let action = input.events.iter().find_map(|event| match event {
                    egui::Event::PointerButton {
                        button: egui::PointerButton::Extra1,
                        pressed: true,
                        ..
                    } => Some(HeaderAction::Back),
                    egui::Event::PointerButton {
                        button: egui::PointerButton::Extra2,
                        pressed: true,
                        ..
                    } => Some(HeaderAction::Forward),
                    _ => None,
                });
                input.events.retain(|event| {
                    !matches!(
                        event,
                        egui::Event::PointerButton {
                            button: egui::PointerButton::Extra1 | egui::PointerButton::Extra2,
                            pressed: true,
                            ..
                        }
                    )
                });
                action
            })
        })
        .flatten();
    if let Some(action) = captured_action {
        ctx.data_mut(|data| {
            let mut state = data
                .get_temp::<MouseNavigationState>(topmost_blade_stack_id())
                .unwrap_or_default();
            state.captured = Some(action);
            data.insert_temp(topmost_blade_stack_id(), state);
        });
        ctx.request_repaint();
    }
    pending_action
}

#[derive(Clone, Copy, Default)]
struct MouseNavigationState {
    pass: u64,
    current: Option<Id>,
    captured: Option<HeaderAction>,
    pending: Option<(Id, HeaderAction)>,
}

fn topmost_blade_stack_id() -> Id {
    Id::new("topmost-blade-stack")
}

fn record_topmost_blade_stack(ctx: &egui::Context, stack_id: Id, accepts_navigation: bool) {
    ctx.data_mut(|data| {
        let mut state = data
            .get_temp::<MouseNavigationState>(topmost_blade_stack_id())
            .unwrap_or_default();
        state.current = accepts_navigation.then_some(stack_id);
        data.insert_temp(topmost_blade_stack_id(), state);
    });
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
fn retain_hidden_layer(ctx: &egui::Context, id: Id, viewport: Rect) {
    egui::Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(egui::pos2(viewport.right() + INSET, viewport.top() + INSET))
        .fade_in(false)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_min_size(egui::Vec2::ZERO);
        });
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
fn show_input_scrim(
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

fn history_navigation_rect(active: Rect, history: &[(Id, Rect, usize)], rect: Rect) -> Rect {
    let right = history
        .iter()
        .filter(|(_, other, _)| other.min.x > rect.min.x)
        .map(|(_, other, _)| other.min.x)
        .chain(std::iter::once(active.min.x))
        .fold(rect.max.x, f32::min);
    Rect::from_min_max(rect.min, egui::pos2(right, rect.max.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{AccessibilitySnapshot, UiHarnessSnapshot};
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Debug, PartialEq)]
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

    #[test]
    fn popup_option_overlapping_the_input_scrim_receives_a_pointer_click() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "History",
        })));
        navigator.borrow_mut().push(TestBlade {
            id: 2,
            title: "Popup",
        });
        navigator.borrow_mut().clear_transition();
        let selected = Rc::new(RefCell::new(false));
        let underlying_action_clicked = Rc::new(RefCell::new(false));
        let dismissed = Rc::new(RefCell::new(false));
        let stack = BladeStack::new("blade-popup-input-order");
        let navigator_for_ui = Rc::clone(&navigator);
        let selected_for_ui = Rc::clone(&selected);
        let underlying_action_clicked_for_ui = Rc::clone(&underlying_action_clicked);
        let dismissed_for_ui = Rc::clone(&dismissed);
        let mut harness = Harness::new_ui(move |ui| {
            if ui.button("Underlying workspace action").clicked() {
                *underlying_action_clicked_for_ui.borrow_mut() = true;
            }
            let response = stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                |ui, _blade, _layer| {
                    let trigger = ui.button("Open popup");
                    egui::Popup::menu(&trigger)
                        .align(egui::RectAlign::BOTTOM_END)
                        .width(320.0)
                        .show(|ui| {
                            if ui.button("Choose popup option").clicked() {
                                *selected_for_ui.borrow_mut() = true;
                            }
                        });
                },
            );
            *dismissed_for_ui.borrow_mut() = response.dismissed;
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        harness
            .get_all_by_label("Open popup")
            .max_by(|left, right| left.rect().left().total_cmp(&right.rect().left()))
            .expect("the foreground blade must render an open-popup button")
            .click();
        harness.run();

        harness.get_by_label("Underlying workspace action").click();
        harness.run();
        assert!(
            !*underlying_action_clicked.borrow(),
            "a click outside the popup must not reach the workspace beneath the blade"
        );
        assert!(
            !*dismissed.borrow(),
            "a click outside the popup must close the popup without dismissing the blade"
        );
        assert!(
            harness.query_by_label("Choose popup option").is_none(),
            "the outside click must close the popup"
        );

        harness
            .get_all_by_label("Open popup")
            .max_by(|left, right| left.rect().left().total_cmp(&right.rect().left()))
            .expect("the foreground blade must render an open-popup button")
            .click();
        harness.run();
        harness.run_steps(1);

        let option = harness.get_by_label("Choose popup option");
        let blade_left = harness.ctx.content_rect().right() - INSET - WIDTH;
        assert!(
            option.rect().left() < blade_left,
            "the popup option must extend into the input-scrim region"
        );

        option.click();
        harness.run();
        assert!(
            *selected.borrow(),
            "the popup option must receive the physical pointer click"
        );
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
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.ui_harness("blades/snapshots_a_single_blade_and_its_visible_history/single");

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
        harness.ui_harness("blades/snapshots_a_single_blade_and_its_visible_history/history");

        navigator.borrow_mut().push(TestBlade {
            id: 4,
            title: "Fourth",
        });
        navigator.borrow_mut().clear_transition();
        harness.run();
        harness.ui_harness("blades/snapshots_a_single_blade_and_its_visible_history/history_cap");
    }

    #[test]
    fn overlap_detection_ignores_intentionally_stacked_blade_layers() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-overlap-validation");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);

        for (id, title) in [(2, "Second"), (3, "Third")] {
            navigator.borrow_mut().push(TestBlade { id, title });
        }
        navigator.borrow_mut().clear_transition();
        harness.run();

        let background_buttons: Vec<_> = harness
            .get_all_by_label("Back in background blade")
            .collect();
        assert_eq!(background_buttons.len(), 2);
        assert!(
            background_buttons[0]
                .rect()
                .intersects(background_buttons[1].rect()),
            "the test must exercise intentional blade overlap"
        );
        assert!(
            harness
                .illegal_accessibility_overlaps(
                    &crate::test_support::AccessibilityTreeOptions::default()
                )
                .is_empty()
        );
    }

    #[test]
    fn snapshots_history_order_when_a_blade_returns_to_the_display_stack() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-returned-to-display-stack-snapshot");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);

        for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
            navigator.borrow_mut().push(TestBlade { id, title });
        }
        navigator.borrow_mut().clear_transition();
        harness.run();

        assert!(navigator.borrow_mut().go_back());
        navigator.borrow_mut().clear_transition();
        harness.run();
        harness.ui_harness("blades/snapshots_history_order_when_a_blade_returns_to_the_display_stack/restored_history_display_stack");
    }

    #[test]
    fn snapshots_history_order_after_crossing_the_display_cap_repeatedly() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-deep-history-cycle-snapshot");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        for (id, title) in [
            (2, "Second"),
            (3, "Third"),
            (4, "Fourth"),
            (5, "Fifth"),
            (6, "Sixth"),
        ] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.run();
        }
        for _ in 0..3 {
            assert!(navigator.borrow_mut().go_back());
            navigator.borrow_mut().clear_transition();
            harness.run();
        }
        for _ in 0..2 {
            assert!(navigator.borrow_mut().go_forward());
            navigator.borrow_mut().clear_transition();
            harness.run();
        }

        assert_eq!(navigator.borrow().current().id, 5);
        harness.ui_harness("blades/snapshots_history_order_after_crossing_the_display_cap_repeatedly/deep_history_cycle");
    }

    #[test]
    fn snapshots_an_interrupted_back_to_forward_transition() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-interrupted-transition-snapshot");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness
            .ctx
            .global_style_mut(|style| style.animation_time = 1.0);
        harness.input_mut().time = Some(1.0);
        harness.step();

        for (id, title) in [(2, "Second"), (3, "Third")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.step();
        }

        assert!(navigator.borrow_mut().go_back());
        harness.input_mut().time = Some(10.0);
        harness.step();
        harness.input_mut().time = Some(10.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();

        assert!(navigator.borrow_mut().go_forward());
        harness.input_mut().time = Some(20.0);
        harness.step();
        harness.input_mut().time = Some(20.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness.ui_harness("blades/snapshots_an_interrupted_back_to_forward_transition/interrupted_back_to_forward");
    }

    #[test]
    fn snapshots_an_interrupted_forward_to_back_transition() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-interrupted-transition-snapshot");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness
            .ctx
            .global_style_mut(|style| style.animation_time = 1.0);
        harness.input_mut().time = Some(1.0);
        harness.step();

        for (id, title) in [(2, "Second"), (3, "Third")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.step();
        }

        assert!(navigator.borrow_mut().go_back());
        harness.input_mut().time = Some(10.0);
        harness.step();
        harness.input_mut().time = Some(10.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();

        assert!(navigator.borrow_mut().go_forward());
        harness.input_mut().time = Some(20.0);
        harness.step();
        harness.input_mut().time = Some(20.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        assert!(navigator.borrow_mut().go_back());
        harness.input_mut().time = Some(30.0);
        harness.step();
        harness.input_mut().time = Some(30.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness.ui_harness("blades/snapshots_an_interrupted_forward_to_back_transition/interrupted_forward_to_back");
    }

    #[test]
    fn snapshots_a_reopened_stack_without_stale_layers() {
        let navigator = Rc::new(RefCell::new(Some(BladeNavigator::new(TestBlade {
            id: 1,
            title: "Original",
        }))));
        navigator
            .borrow_mut()
            .as_mut()
            .expect("navigator is open")
            .clear_transition();
        let stack = BladeStack::new("blade-reopened-stack-snapshot");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            if let Some(navigator) = navigator_for_ui.borrow_mut().as_mut() {
                stack.show_with_title(
                    ui.ctx(),
                    navigator,
                    |blade| blade.title.to_owned(),
                    render_test_blade,
                );
            }
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        navigator.borrow_mut().take();
        harness.run();

        *navigator.borrow_mut() = Some(BladeNavigator::new(TestBlade {
            id: 2,
            title: "Reopened",
        }));
        navigator
            .borrow_mut()
            .as_mut()
            .expect("navigator was reopened")
            .clear_transition();
        harness.run();
        harness.ui_harness("blades/snapshots_a_reopened_stack_without_stale_layers/reopened_stack");
    }

    #[test]
    fn snapshots_restored_history_after_resizing_the_viewport() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-resized-history-snapshot");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.run();
        }
        assert!(navigator.borrow_mut().go_back());
        navigator.borrow_mut().clear_transition();
        harness.set_size(egui::vec2(1024.0, 768.0));
        harness.run();
        harness.ui_harness("blades/snapshots_restored_history_after_resizing_the_viewport/resized_restored_history");
    }

    #[test]
    fn discarded_forward_history_is_never_rendered_again() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let stack = BladeStack::new("blade-discarded-forward-history");
        let navigator_for_ui = Rc::clone(&navigator);
        let rendered_for_ui = Rc::clone(&rendered);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                |ui, blade, layer| {
                    rendered_for_ui.borrow_mut().push(blade.id);
                    render_test_blade(ui, blade, layer);
                },
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        for (id, title) in [(2, "Second"), (3, "Third")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.run();
        }
        assert!(navigator.borrow_mut().go_back());
        navigator.borrow_mut().clear_transition();
        harness.run();
        assert_eq!(
            navigator
                .borrow()
                .forward_stack()
                .last()
                .map(|blade| blade.id),
            Some(3)
        );

        let discarded = navigator.borrow_mut().push(TestBlade {
            id: 4,
            title: "Replacement",
        });
        assert_eq!(discarded.len(), 1);
        assert_eq!(discarded[0].id, 3);
        navigator.borrow_mut().clear_transition();
        rendered.borrow_mut().clear();
        harness.run();

        assert!(
            !rendered.borrow().contains(&3),
            "discarded forward history must not remain in the display stack"
        );
    }

    #[test]
    fn snapshots_the_most_recently_rendered_stack_above_other_stacks() {
        let first_navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First stack",
        })));
        let second_navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 2,
            title: "Second stack",
        })));
        first_navigator.borrow_mut().clear_transition();
        second_navigator.borrow_mut().clear_transition();
        let first_stack = BladeStack::new("first-concurrent-blade-stack");
        let second_stack = BladeStack::new("second-concurrent-blade-stack");
        let first_for_ui = Rc::clone(&first_navigator);
        let second_for_ui = Rc::clone(&second_navigator);
        let mut harness = Harness::new_ui(move |ui| {
            first_stack.show_with_title(
                ui.ctx(),
                &mut first_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
            second_stack.show_with_title(
                ui.ctx(),
                &mut second_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.ui_harness("blades/snapshots_the_most_recently_rendered_stack_above_other_stacks/concurrent_stacks");
    }

    #[test]
    fn restored_history_keeps_focus_and_keyboard_navigation_on_the_active_blade() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-restored-history-accessibility");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.run();
        }
        assert!(navigator.borrow_mut().go_back());
        navigator.borrow_mut().clear_transition();
        harness.run();

        assert_eq!(
            harness.get_all_by_label("Back in background blade").count(),
            2,
            "the restored history blades must not expose foreground controls"
        );
        harness.get_by_label("Back").focus();
        harness.run();
        assert!(harness.get_by_label("Back").is_focused());

        harness.key_press(egui::Key::Enter);
        harness.run();
        assert_eq!(navigator.borrow().current().id, 2);
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
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness
            .ctx
            .global_style_mut(|style| style.animation_time = 1.0);

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
        harness.ui_harness(
            "blades/snapshots_opening_and_forward_animation_frames/opening_first_frame",
        );
        harness.input_mut().time = Some(1.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness
            .ui_harness("blades/snapshots_opening_and_forward_animation_frames/opening_mid_frame");
        harness.input_mut().time = Some(1.0 + f64::from(TRANSITION_DURATION));
        harness.step();
        harness.ui_harness(
            "blades/snapshots_opening_and_forward_animation_frames/opening_final_frame",
        );

        navigator.borrow_mut().push(TestBlade {
            id: 2,
            title: "Second",
        });
        harness.input_mut().time = Some(20.0);
        harness.step();
        harness.ui_harness(
            "blades/snapshots_opening_and_forward_animation_frames/forward_first_frame",
        );
        harness.input_mut().time = Some(20.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness
            .ui_harness("blades/snapshots_opening_and_forward_animation_frames/forward_mid_frame");

        assert!(navigator.borrow_mut().go_back());
        harness.input_mut().time = Some(30.0);
        harness.step();
        harness
            .ui_harness("blades/snapshots_opening_and_forward_animation_frames/back_first_frame");
        harness.input_mut().time = Some(30.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness.ui_harness("blades/snapshots_opening_and_forward_animation_frames/back_mid_frame");
    }

    #[test]
    fn snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-history-overflow-animation");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness
            .ctx
            .global_style_mut(|style| style.animation_time = 1.0);
        harness.input_mut().time = Some(1.0);
        harness.step();

        for (id, title) in [(2, "Second"), (3, "Third")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.step();
        }

        navigator.borrow_mut().push(TestBlade {
            id: 4,
            title: "Fourth",
        });
        harness.input_mut().time = Some(10.0);
        harness.step();
        harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/history_overflow_first_frame");
        // The capped history blade remains fully visible until the other
        // history layers have completed their transition.
        harness.input_mut().time = Some(10.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/history_overflow_mid_frame");
        harness.input_mut().time = Some(10.0 + f64::from(TRANSITION_DURATION));
        harness.step();
        harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/history_overflow_final_frame");

        assert!(navigator.borrow_mut().go_back_steps(2));
        harness.input_mut().time = Some(20.0);
        harness.step();
        harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/direct_two_step_back_first_frame");
        harness.input_mut().time = Some(20.0 + f64::from(TRANSITION_DURATION / 2.0));
        harness.step();
        harness.ui_harness("blades/snapshots_history_overflow_delayed_removal_and_direct_two_step_back_animation/direct_two_step_back_mid_frame");
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
                |ui, blade, _| {
                    ui.label(egui::RichText::new(format!("Custom: {}", blade.title)).strong());
                },
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        harness.ui_harness(
            "blades/snapshots_custom_header_content_with_shared_controls/custom_header",
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
                |blade| blade.title.to_owned(),
                |ui, blade, layer| {
                    rendered_for_ui.borrow_mut().push(blade.id);
                    render_test_blade(ui, blade, layer);
                },
            );
        });
        crate::test_support::setup_egui(&mut harness);
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
        crate::test_support::setup_egui(&mut harness);
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
    fn content_ids_are_synthesized_from_stack_positions() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack_source = "blade-stack-position-content-ids";
        let stack = BladeStack::new(stack_source);
        let rendered = Rc::new(RefCell::new(Vec::new()));
        let navigator_for_ui = Rc::clone(&navigator);
        let rendered_for_ui = Rc::clone(&rendered);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                |ui, blade, layer| {
                    rendered_for_ui
                        .borrow_mut()
                        .push((blade.id, layer.content_id));
                    render_test_blade(ui, blade, layer);
                },
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        navigator.borrow_mut().push(TestBlade {
            id: 2,
            title: "Second",
        });
        navigator.borrow_mut().push(TestBlade {
            id: 3,
            title: "Third",
        });
        navigator.borrow_mut().clear_transition();
        rendered.borrow_mut().clear();
        harness.run();

        let expected = [
            (1, Id::new(stack_source).with(("blade-content", 0))),
            (2, Id::new(stack_source).with(("blade-content", 1))),
            (3, Id::new(stack_source).with(("blade-content", 2))),
        ];
        assert!(
            rendered
                .borrow()
                .chunks_exact(expected.len())
                .all(|frame| frame == expected),
            "content IDs should be derived from each blade's stack position: {rendered:?}"
        );
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
        crate::test_support::setup_egui(&mut harness);
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
    fn navigator_can_jump_back_multiple_steps() {
        let mut navigator = BladeNavigator::new("one");
        navigator.push("two");
        navigator.push("three");
        navigator.push("four");

        assert!(navigator.go_back_steps(2));
        assert_eq!(navigator.current(), &"two");
        assert_eq!(navigator.back_stack(), &["one"]);
        assert_eq!(navigator.forward_stack(), &["four", "three"]);
        assert_eq!(navigator.transition(), Some(BladeTransition::Back));
        assert_eq!(navigator.back_steps(), 2);

        assert!(!navigator.go_back_steps(0));
        assert!(!navigator.go_back_steps(2));
        assert_eq!(navigator.current(), &"two");
    }

    #[test]
    fn visible_history_blades_are_clickable_without_dismissing_the_stack() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let dismissed = Rc::new(RefCell::new(false));
        let stack = BladeStack::new("blade-clickable-history");
        let navigator_for_ui = Rc::clone(&navigator);
        let dismissed_for_ui = Rc::clone(&dismissed);
        let mut harness = Harness::new_ui(move |ui| {
            let response = stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
            *dismissed_for_ui.borrow_mut() = response.dismissed;
        });
        crate::test_support::setup_egui(&mut harness);
        for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.run();
        }

        harness.get_by_label("Go back two blades").click();
        harness.run();

        assert_eq!(navigator.borrow().current().id, 2);
        assert_eq!(
            navigator
                .borrow()
                .forward_stack()
                .last()
                .map(|blade| blade.id),
            Some(3)
        );
        assert!(!*dismissed.borrow());
    }

    #[test]
    fn clicking_the_nearest_history_blade_goes_back_one_step() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-clickable-nearest-history");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        for (id, title) in [(2, "Second"), (3, "Third")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.run();
        }

        harness.get_by_label("Go back one blade").click();
        harness.run();

        assert_eq!(navigator.borrow().current().id, 2);
    }

    #[test]
    fn clicking_overlapping_history_blades_selects_the_topmost_blade() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-overlapping-history-click");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.run();
        }

        let viewport = harness.ctx.content_rect();
        let older = transformed_rect(viewport, history_transform(viewport, 1));
        let nearer = transformed_rect(viewport, history_transform(viewport, 0));
        let active = transformed_rect(viewport, active_transform(viewport));
        let overlap = older.intersect(nearer);
        assert!(overlap.is_positive(), "the history blades should overlap");

        let click_position = egui::pos2((nearer.min.x + active.min.x) / 2.0, overlap.center().y);
        assert!(older.contains(click_position));
        assert!(nearer.contains(click_position));
        assert!(!active.contains(click_position));
        harness.event(egui::Event::PointerMoved(click_position));
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: click_position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
        harness.run();

        assert_eq!(
            navigator.borrow().current().id,
            3,
            "the nearer history blade must win its overlap with the older blade"
        );
    }

    #[test]
    fn clicking_history_under_the_foreground_blade_keeps_the_foreground_active() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        let dismissed = Rc::new(RefCell::new(false));
        let stack = BladeStack::new("blade-foreground-overlap-click");
        let navigator_for_ui = Rc::clone(&navigator);
        let dismissed_for_ui = Rc::clone(&dismissed);
        let mut harness = Harness::new_ui(move |ui| {
            let response = stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
            *dismissed_for_ui.borrow_mut() = response.dismissed;
        });
        crate::test_support::setup_egui(&mut harness);
        for (id, title) in [(2, "Second"), (3, "Third"), (4, "Fourth")] {
            navigator.borrow_mut().push(TestBlade { id, title });
            navigator.borrow_mut().clear_transition();
            harness.run();
        }

        let viewport = harness.ctx.content_rect();
        let history = transformed_rect(viewport, history_transform(viewport, 0));
        let active = transformed_rect(viewport, active_transform(viewport));
        let overlap = history.intersect(active);
        assert!(
            overlap.is_positive(),
            "history should extend under the foreground blade"
        );
        let click_position = overlap.center();
        harness.event(egui::Event::PointerMoved(click_position));
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: click_position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            });
        }
        harness.run();

        assert_eq!(navigator.borrow().current().id, 4);
        assert!(!*dismissed.borrow());
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
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
            *close_finished_for_ui.borrow_mut() = response.close_finished;
        });
        crate::test_support::setup_egui(&mut harness);
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
    fn extra_mouse_buttons_navigate_blade_history() {
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
        let stack = BladeStack::new("blade-extra-mouse-navigation");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        for button in [egui::PointerButton::Extra1, egui::PointerButton::Extra2] {
            harness.event(egui::Event::PointerButton {
                pos: egui::pos2(0.0, 0.0),
                button,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            });
            harness.run();
        }

        assert_eq!(navigator.borrow().current().id, 2);
        assert_eq!(navigator.borrow().back_stack().len(), 1);
        assert!(navigator.borrow().forward_stack().is_empty());
    }

    #[test]
    fn extra_mouse_buttons_do_not_navigate_without_available_history() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "Only",
        })));
        navigator.borrow_mut().clear_transition();
        let stack = BladeStack::new("blade-extra-mouse-navigation-unavailable");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        for button in [egui::PointerButton::Extra1, egui::PointerButton::Extra2] {
            harness.event(egui::Event::PointerButton {
                pos: egui::pos2(0.0, 0.0),
                button,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            });
            harness.run();
        }

        assert_eq!(navigator.borrow().current().id, 1);
        assert!(navigator.borrow().back_stack().is_empty());
        assert!(navigator.borrow().forward_stack().is_empty());
    }

    #[test]
    fn extra_mouse_buttons_immediately_replace_blade_transitions() {
        let navigator = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "First",
        })));
        navigator.borrow_mut().clear_transition();
        navigator.borrow_mut().push(TestBlade {
            id: 2,
            title: "Second",
        });
        let stack = BladeStack::new("blade-extra-mouse-navigation-transition");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness
            .ctx
            .global_style_mut(|style| style.animation_time = 1.0);
        // `Harness::new_ui` renders once before the configured test clock.
        // Restart this transition so the next frame is its first animation frame.
        {
            let mut navigator = navigator.borrow_mut();
            navigator.transition = Some(BladeTransition::Forward);
            navigator.transition_started_at = None;
        }
        harness.input_mut().time = Some(1.0);
        harness.event(egui::Event::PointerButton {
            pos: egui::pos2(0.0, 0.0),
            button: egui::PointerButton::Extra1,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        harness.step();
        assert_eq!(navigator.borrow().current().id, 2);

        harness.input_mut().time = Some(1.0 + f64::from(TRANSITION_DURATION));
        harness.step();
        assert_eq!(navigator.borrow().current().id, 1);
    }

    #[test]
    fn extra_mouse_buttons_navigate_only_the_topmost_blade_stack() {
        let background = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "Background first",
        })));
        let foreground = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 3,
            title: "Foreground first",
        })));
        for (navigator, next) in [
            (
                &background,
                TestBlade {
                    id: 2,
                    title: "Background second",
                },
            ),
            (
                &foreground,
                TestBlade {
                    id: 4,
                    title: "Foreground second",
                },
            ),
        ] {
            navigator.borrow_mut().clear_transition();
            navigator.borrow_mut().push(next);
            navigator.borrow_mut().clear_transition();
        }
        let background_stack = BladeStack::new("background-blade-stack");
        let foreground_stack = BladeStack::new("foreground-blade-stack");
        let background_for_ui = Rc::clone(&background);
        let foreground_for_ui = Rc::clone(&foreground);
        let mut harness = Harness::new_ui(move |ui| {
            background_stack.show_with_title(
                ui.ctx(),
                &mut background_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
            foreground_stack.show_with_title(
                ui.ctx(),
                &mut foreground_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        harness.event(egui::Event::PointerButton {
            pos: egui::pos2(0.0, 0.0),
            button: egui::PointerButton::Extra1,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();

        assert_eq!(background.borrow().current().id, 2);
        assert_eq!(foreground.borrow().current().id, 3);
    }

    #[test]
    fn extra_mouse_buttons_navigate_the_remaining_stack_after_the_foreground_closes() {
        let background = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 1,
            title: "Background first",
        })));
        background.borrow_mut().clear_transition();
        background.borrow_mut().push(TestBlade {
            id: 2,
            title: "Background second",
        });
        background.borrow_mut().clear_transition();
        let foreground = Rc::new(RefCell::new(BladeNavigator::new(TestBlade {
            id: 3,
            title: "Foreground",
        })));
        foreground.borrow_mut().clear_transition();
        let show_foreground = Rc::new(RefCell::new(true));
        let background_stack = BladeStack::new("remaining-background-blade-stack");
        let foreground_stack = BladeStack::new("removed-foreground-blade-stack");
        let background_for_ui = Rc::clone(&background);
        let foreground_for_ui = Rc::clone(&foreground);
        let show_foreground_for_ui = Rc::clone(&show_foreground);
        let mut harness = Harness::new_ui(move |ui| {
            background_stack.show_with_title(
                ui.ctx(),
                &mut background_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
            if *show_foreground_for_ui.borrow() {
                foreground_stack.show_with_title(
                    ui.ctx(),
                    &mut foreground_for_ui.borrow_mut(),
                    |blade| blade.title.to_owned(),
                    render_test_blade,
                );
            }
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        *show_foreground.borrow_mut() = false;
        harness.event(egui::Event::PointerButton {
            pos: egui::pos2(0.0, 0.0),
            button: egui::PointerButton::Extra1,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();

        assert_eq!(background.borrow().current().id, 1);
    }

    #[test]
    fn extra_mouse_buttons_are_ignored_while_a_blade_is_closing() {
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
        let stack = BladeStack::new("blade-extra-mouse-navigation-closing");
        let navigator_for_ui = Rc::clone(&navigator);
        let mut harness = Harness::new_ui(move |ui| {
            stack.show_with_title(
                ui.ctx(),
                &mut navigator_for_ui.borrow_mut(),
                |blade| blade.title.to_owned(),
                render_test_blade,
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        assert!(navigator.borrow_mut().begin_close());
        harness.event(egui::Event::PointerButton {
            pos: egui::pos2(0.0, 0.0),
            button: egui::PointerButton::Extra1,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        harness.run();

        assert_eq!(navigator.borrow().current().id, 2);
        assert_eq!(navigator.borrow().back_stack().len(), 1);
        assert!(navigator.borrow().forward_stack().is_empty());
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
                |blade| blade.title.to_owned(),
                |ui, _, _| {
                    *observed_width_for_ui.borrow_mut() = Some(ui.available_width());
                },
            );
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();

        assert_eq!(*observed_width.borrow(), Some(CONTENT_WIDTH));
    }
}
