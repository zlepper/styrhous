use super::*;

/// Where a blade is: its top left, and how far it is inset from the full
/// blade height at the top and the bottom (a history blade steps down).
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Transform {
    pub(super) position: Pos2,
    pub(super) inset: f32,
}
pub(super) fn duration(ctx: &egui::Context) -> f32 {
    if ctx.global_style().animation_time == 0.0 {
        0.0
    } else {
        TRANSITION_DURATION
    }
}
pub(super) fn progress<T>(ctx: &egui::Context, navigator: &BladeNavigator<T>) -> f32 {
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
pub(super) fn height(viewport: Rect) -> f32 {
    (viewport.height() - INSET * 2.0).max(0.0)
}
/// The width every blade is drawn at: [`WIDTH`], at most half the viewport
/// but never under [`size::BLADE_MIN`], and less when the viewport cannot
/// fit that plus its [`INSET`] on both sides.
pub(super) fn blade_width(viewport: Rect) -> f32 {
    let share = (viewport.width() * MAX_WINDOW_SHARE).max(size::BLADE_MIN);
    WIDTH
        .min(share)
        .min((viewport.width() - INSET * 2.0).max(0.0))
}

/// The largest share of the viewport a blade takes (S10: max 50%).
const MAX_WINDOW_SHARE: f32 = 0.5;
pub(super) fn active_transform(viewport: Rect) -> Transform {
    Transform {
        position: egui::pos2(
            viewport.right() - blade_width(viewport) - INSET,
            viewport.top() + INSET,
        ),
        inset: 0.0,
    }
}
/// The peek of the history blade `index` steps behind the active one (0 is
/// the nearest): [`size::BLADE_PEEK`] further left and [`size::BLADE_PEEK_STEP`]
/// further in from the top and bottom than the one in front of it.
pub(super) fn history_transform(viewport: Rect, index: usize) -> Transform {
    let steps = (index.min(HISTORY_PEEKS - 1) + 1) as f32;
    let active = active_transform(viewport);
    Transform {
        position: egui::pos2(
            active.position.x - size::BLADE_PEEK * steps,
            active.position.y + size::BLADE_PEEK_STEP * steps,
        ),
        inset: size::BLADE_PEEK_STEP * steps,
    }
}
pub(super) fn should_promote_active_blade(
    has_history_layers: bool,
    transition: Option<(BladeTransition, f32)>,
) -> bool {
    has_history_layers
        && !matches!(
            transition,
            Some((BladeTransition::Back, progress)) if progress < 1.0
        )
}
pub(super) fn history_layer_transform(
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
pub(super) fn closing_transform(viewport: Rect, transform: Transform, progress: f32) -> Transform {
    Transform {
        position: egui::pos2(
            egui::lerp(transform.position.x..=viewport.right() + INSET, progress),
            transform.position.y,
        ),
        inset: transform.inset,
    }
}
pub(super) fn interpolate(from: Transform, to: Transform, value: f32) -> Transform {
    Transform {
        position: from.position + (to.position - from.position) * value,
        inset: from.inset + (to.inset - from.inset) * value,
    }
}
pub(super) fn transformed_rect(viewport: Rect, transform: Transform) -> Rect {
    Rect::from_min_size(
        transform.position,
        egui::vec2(
            blade_width(viewport),
            (height(viewport) - 2.0 * transform.inset).max(0.0),
        ),
    )
}
pub(super) fn history_navigation_label(steps: usize) -> String {
    match steps {
        1 => "Go back one blade".to_owned(),
        2 => "Go back two blades".to_owned(),
        _ => format!("Go back {steps} blades"),
    }
}
