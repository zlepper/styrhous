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

impl BladeLayer {
    /// The Back, Forward and Close controls of this blade's header, named for a
    /// background blade when it is not in front so they stay distinct from the
    /// active blade's.
    pub fn navigation(&self) -> chrome::BladeNavigation<'static> {
        let navigation = chrome::BladeNavigation::new(self.can_go_back, self.can_go_forward);
        if self.is_foreground {
            navigation
        } else {
            chrome::BladeNavigation {
                back_label: "Back in background blade",
                forward_label: "Forward in background blade",
                close_label: "Close background blade",
                ..navigation
            }
        }
    }
}

pub struct BladeResponse<H, R> {
    /// The custom header callback's result for the active blade.
    pub header: H,
    pub active: R,
    pub dismissed: bool,
    pub close_finished: bool,
}

/// How a blade lays its header and body out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BladeStyle {
    /// The shared 56 point header (Back, Forward, content, Close) over a
    /// padded, scrolling body.
    #[default]
    Standard,
    /// The blade draws its own header, navigation included (an
    /// [`InspectorHeader`]), and divides the whole body itself.
    Inspector,
}

/// What the stack needs to know of a blade besides its content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BladeDescription {
    pub style: BladeStyle,
}

/// The result of a blade's header and the navigation it asked for. A
/// [`BladeStyle::Standard`] header's `action` is ignored: the shared chrome
/// reports it.
pub struct BladeHeader<H> {
    pub value: H,
    pub action: HeaderAction,
}

#[derive(Clone)]
pub struct BladeStack {
    id: Id,
    /// Window chrome above the blades (a banner): blades stay below it,
    /// while the scrim still dims it.
    top_inset: f32,
}

impl BladeStack {
    pub fn new(id_source: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self {
            id: Id::new(id_source),
            top_inset: 0.0,
        }
    }

    /// Keep the blades `inset` points below the window's top, under chrome
    /// such as a banner (A11); the scrim still covers the whole window.
    pub fn set_top_inset(&mut self, inset: f32) {
        self.top_inset = inset;
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
        render_content: impl FnMut(&mut Ui, &mut T, BladeLayer) -> R,
    ) -> BladeResponse<H, R> {
        self.show_blades(
            ctx,
            navigator,
            |_| BladeDescription::default(),
            |ui, entry, layer| BladeHeader {
                value: render_header(ui, entry, layer),
                action: HeaderAction::None,
            },
            render_content,
        )
    }

    /// Render blades of any [`BladeStyle`]: `describe` says how each lays out,
    /// `render_header` draws a header (the
    /// whole of an inspector's, the content of a standard one's).
    pub fn show_blades<T, H, R>(
        &self,
        ctx: &egui::Context,
        navigator: &mut BladeNavigator<T>,
        describe: impl Fn(&T) -> BladeDescription,
        mut render_header: impl FnMut(&mut Ui, &mut T, BladeLayer) -> BladeHeader<H>,
        mut render_content: impl FnMut(&mut Ui, &mut T, BladeLayer) -> R,
    ) -> BladeResponse<H, R> {
        // One blade's header and body, in the style it asks for.
        let mut show_entry = |ui: &mut Ui, entry: &mut T, layer: BladeLayer| {
            let style = describe(entry).style;
            let (header, action) = match style {
                BladeStyle::Standard => {
                    show_header(ui, layer, |ui| render_header(ui, entry, layer).value)
                }
                BladeStyle::Inspector => {
                    let header = render_header(ui, entry, layer);
                    (header.value, header.action)
                }
            };
            let body = match style {
                BladeStyle::Standard => show_body(ui, |ui| render_content(ui, entry, layer)),
                BladeStyle::Inspector => show_bare_body(ui, |ui| render_content(ui, entry, layer)),
            };
            (header, body, action)
        };
        let window = ctx.content_rect();
        let viewport = egui::Rect::from_min_max(
            egui::pos2(window.left(), window.top() + self.top_inset),
            window.max,
        );
        // A blade behind the active one: never interactable, never in front.
        let mut show_background = |entry: &mut T,
                                   stack_index: usize,
                                   transform: Transform,
                                   can_go_back: bool,
                                   can_go_forward: bool| {
            let layer = BladeLayer {
                content_id: self.content_id(stack_index),
                is_foreground: false,
                can_go_back,
                can_go_forward,
            };
            show_layer(
                ctx,
                self.layer_id(stack_index),
                viewport,
                transform,
                false,
                |ui| {
                    let _ = show_entry(ui, entry, layer);
                },
            );
        };
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
        paint_scrim(ctx, self.id, window, closing_progress);

        let first_history = history_len.saturating_sub(HISTORY_SCALES.len());
        for stack_index in 0..first_history {
            retain_hidden_layer(ctx, self.layer_id(stack_index), viewport);
        }
        if let Some((BladeTransition::Forward, value)) = transition
            && value < 1.0
            && first_history > 0
        {
            // The blade pushed out of the fan slides in behind it.
            let stack_index = first_history - 1;
            show_background(
                &mut navigator.back_stack_mut()[stack_index],
                stack_index,
                history_transform(viewport, HISTORY_SCALES.len() - 1),
                stack_index > 0,
                true,
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
            history_targets.push((
                self.content_id(stack_index),
                transformed_rect(viewport, transform),
                depth + 1,
            ));
            show_background(entry, stack_index, transform, stack_index > 0, true);
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
                let start = if index + 1 < transition_back_steps {
                    history_transform(viewport, index)
                } else {
                    active_transform(viewport)
                };
                show_background(
                    entry,
                    stack_index,
                    closing_transform(viewport, start, value),
                    true,
                    can_go_forward,
                );
            }
        }

        let active_blade_transform = match transition {
            Some((BladeTransition::Opening | BladeTransition::Forward, value)) => Transform {
                position: active_transform(viewport).position
                    + egui::vec2((1.0 - value) * blade_width(viewport), 0.0),
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
        let active_rect = transformed_rect(viewport, active_blade_transform);
        let active_content_id = self.content_id(history_len);
        let active_area_id = self.layer_id(history_len);
        let popup_was_open = egui::Popup::is_any_open(ctx);
        let (scrim_dismissed, scrim_history_selection) = show_input_scrim(
            ctx,
            self.id,
            viewport,
            active_rect,
            &history_targets,
            active_interactable,
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
                show_entry(ui, navigator.current_mut(), layer)
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
        self.show_blades(
            ctx,
            navigator,
            |_| BladeDescription::default(),
            |ui, entry, _| {
                chrome::blade_title(ui, None, &title(entry));
                BladeHeader {
                    value: (),
                    action: HeaderAction::None,
                }
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
