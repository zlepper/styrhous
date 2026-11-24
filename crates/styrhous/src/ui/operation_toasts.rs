use super::state::{OperationOutcome, UiState};
use components::design::theme::Tone;
use components::{Toast, show_toast_stack};
use egui::Id;
use std::time::Instant;

pub(super) fn show(ctx: &egui::Context, ui_state: &mut UiState) {
    let now = Instant::now();
    let toasts = ui_state.visible_operation_toasts(now);
    let Some(next_expiration) = toasts.iter().map(|toast| toast.expires_at).min() else {
        return;
    };
    ctx.request_repaint_after(next_expiration.saturating_duration_since(now));

    let dismissed = show_toast_stack(ctx, Id::new("operation-outcome-toasts"), |ui| {
        let mut dismissed = Vec::new();
        for toast in &toasts {
            let tone = match toast.outcome {
                OperationOutcome::Success => Tone::Success,
                OperationOutcome::Warning => Tone::Warning,
                OperationOutcome::Failure => Tone::Danger,
            };
            let response = Toast::new(tone, &toast.title)
                .id_salt(("operation-toast", toast.sequence))
                .detail(&toast.target)
                .show(ui);
            if response.dismissed {
                dismissed.push(toast.sequence);
            }
        }
        dismissed
    });
    for sequence in dismissed {
        ui_state.dismiss_operation_toast(sequence);
    }
}
