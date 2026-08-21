use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Transform {
    pub(super) position: Pos2,
    pub(super) scale: f32,
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
    viewport.height() - INSET * 2.0
}
pub(super) fn active_transform(viewport: Rect) -> Transform {
    Transform {
        position: egui::pos2(viewport.right() - WIDTH - INSET, viewport.top() + INSET),
        scale: 1.0,
    }
}
pub(super) fn history_transform(viewport: Rect, index: usize) -> Transform {
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
        scale: transform.scale,
    }
}
pub(super) fn interpolate(from: Transform, to: Transform, value: f32) -> Transform {
    Transform {
        position: from.position + (to.position - from.position) * value,
        scale: from.scale + (to.scale - from.scale) * value,
    }
}
pub(super) fn transformed_rect(viewport: Rect, transform: Transform) -> Rect {
    Rect::from_min_size(
        transform.position,
        egui::vec2(WIDTH, height(viewport)) * transform.scale,
    )
}
pub(super) fn history_navigation_label(steps: usize) -> String {
    match steps {
        1 => "Go back one blade".to_owned(),
        2 => "Go back two blades".to_owned(),
        _ => format!("Go back {steps} blades"),
    }
}
