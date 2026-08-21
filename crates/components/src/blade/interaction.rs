use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum HeaderAction {
    None,
    Back,
    Forward,
    Close,
}

/// Dispatch a side-button press to the stack that was topmost in the event
/// pass. The subsequent transition immediately replaces any animation in
/// progress, while presses on a closing stack are discarded.
pub(super) fn mouse_navigation_action(
    ctx: &egui::Context,
    stack_id: Id,
    is_closing: bool,
) -> HeaderAction {
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
pub(super) struct MouseNavigationState {
    pass: u64,
    current: Option<Id>,
    captured: Option<HeaderAction>,
    pending: Option<(Id, HeaderAction)>,
}

pub(super) fn topmost_blade_stack_id() -> Id {
    Id::new("topmost-blade-stack")
}

pub(super) fn record_topmost_blade_stack(
    ctx: &egui::Context,
    stack_id: Id,
    accepts_navigation: bool,
) {
    ctx.data_mut(|data| {
        let mut state = data
            .get_temp::<MouseNavigationState>(topmost_blade_stack_id())
            .unwrap_or_default();
        state.current = accepts_navigation.then_some(stack_id);
        data.insert_temp(topmost_blade_stack_id(), state);
    });
}
