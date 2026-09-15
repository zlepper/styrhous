use super::*;

pub(crate) fn show_log_source_confirmation(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let Some(pending) = ui_state.pending_log_sources.clone() else {
        return;
    };
    let source_count = pending.targets.len();
    let pod_count = pending
        .targets
        .iter()
        .map(|target| (&target.namespace, &target.pod_name))
        .collect::<std::collections::HashSet<_>>()
        .len();
    let details = pending
        .targets
        .iter()
        .map(crate::worker::PodLogStreamTarget::display_name)
        .collect::<Vec<_>>()
        .join("\n");
    let title = format!("Open logs from {source_count} containers?");
    let message = format!(
        "This will open {source_count} concurrent log streams across {pod_count} selected Pods."
    );
    match (ConfirmationDialog {
        id: egui::Id::new("pod-log-source-confirmation"),
        eyebrow: "LOG SOURCES",
        title: &title,
        message: &message,
        unavailable_message: None,
        cancel_label: "Cancel",
        confirm_label: "Open logs",
        kind: ConfirmationDialogKind::Primary,
        confirm_enabled: true,
        warning: Some(ConfirmationDialogWarning {
            title: "Log sources",
            message: "Review the selected sources before opening the combined stream.",
            details: Some(&details),
        }),
        acknowledgement: None,
    })
    .show(ctx)
    {
        ConfirmationDialogAction::Confirm => {
            ui_state.pending_log_sources = None;
            ui_state.open_log_window(pending.cluster_key, pending.targets, commands_to_send);
        }
        ConfirmationDialogAction::Cancel => ui_state.pending_log_sources = None,
        ConfirmationDialogAction::None => {}
    }
}
