use super::*;
use super::{interaction::*, rendering::*, transforms::*};

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
