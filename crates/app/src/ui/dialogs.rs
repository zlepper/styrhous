use super::state::UiState;
use crate::terminal_launcher::TerminalLaunchSettings;
use crate::worker::WorkerCommand;
use components::{
    ConfirmationDialog, ConfirmationDialogAction, ConfirmationDialogKind, ErrorDialog,
    ErrorDialogAction,
};
use std::time::Instant;

pub(super) fn show_delete_confirmation(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(cluster) = ui_state.clusters.get(&cluster_id) else {
        return;
    };
    let Some(pending) = cluster.pending_delete.clone() else {
        return;
    };
    let cluster_key = cluster.cluster_key;
    let now = Instant::now();
    let remaining = pending
        .confirmation_available_at
        .saturating_duration_since(now);
    let confirm_enabled = remaining.is_zero();
    if !confirm_enabled {
        ctx.request_repaint_after(remaining);
    }
    let seconds = (remaining.as_millis() + 999) / 1_000;
    let unavailable_message =
        (!confirm_enabled).then(|| format!("Delete will be available in {seconds} seconds."));
    let scope_text = pending.namespace.as_deref().map_or_else(
        || "This will delete the cluster-wide resource.".to_owned(),
        |namespace| format!("This will delete the resource from namespace {namespace}."),
    );
    let action = ConfirmationDialog {
        id: egui::Id::new("delete-resource-confirmation"),
        eyebrow: "DELETE RESOURCE",
        title: "Delete resource?",
        message: &scope_text,
        unavailable_message: unavailable_message.as_deref(),
        cancel_label: "Cancel",
        confirm_label: &format!("Delete {}", pending.resource_name),
        kind: ConfirmationDialogKind::Destructive,
        confirm_enabled,
    }
    .show(ctx);

    if action == ConfirmationDialogAction::Cancel {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_delete = None;
        }
    } else if action == ConfirmationDialogAction::Confirm {
        commands_to_send.push(WorkerCommand::DeleteResource {
            cluster_key,
            api_resource: pending.api_resource,
            namespace: pending.namespace,
            resource_name: pending.resource_name,
        });
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_delete = None;
        }
    }
}

pub(super) fn show_deployment_restart_confirmation(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(cluster) = ui_state.clusters.get(&cluster_id) else {
        return;
    };
    let Some(pending) = cluster.pending_deployment_restart.clone() else {
        return;
    };
    let cluster_key = cluster.cluster_key;
    let message = format!(
        "This updates the pod template and starts a rolling replacement of the pods for {} in the {} namespace.",
        pending.resource_name, pending.namespace
    );
    let action = ConfirmationDialog {
        id: egui::Id::new("restart-deployment-rollout-confirmation"),
        eyebrow: "DEPLOYMENT",
        title: "Restart rollout?",
        message: &message,
        unavailable_message: None,
        cancel_label: "Cancel",
        confirm_label: "Restart rollout",
        kind: ConfirmationDialogKind::Primary,
        confirm_enabled: true,
    }
    .show(ctx);

    if action == ConfirmationDialogAction::Cancel {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_deployment_restart = None;
        }
    } else if action == ConfirmationDialogAction::Confirm {
        commands_to_send.push(WorkerCommand::RestartDeployment {
            cluster_key,
            namespace: pending.namespace,
            resource_name: pending.resource_name,
        });
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_deployment_restart = None;
        }
    }
}

pub(super) fn show_deployment_restart_error(ctx: &egui::Context, ui_state: &mut UiState) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(error) = ui_state
        .clusters
        .get(&cluster_id)
        .and_then(|cluster| cluster.deployment_restart_error.as_deref())
    else {
        return;
    };
    if matches!(
        (ErrorDialog {
            id: egui::Id::new("deployment-restart-error"),
            eyebrow: "DEPLOYMENT",
            title: "Couldn’t restart rollout",
            message: "Kubernetes Dev UI could not request a rolling restart for this Deployment.",
            details: Some(error),
            recovery: Some("Check your Kubernetes permissions and the Deployment’s current state."),
            primary_action_label: None,
        })
        .show(ctx),
        ErrorDialogAction::Dismiss
    ) {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.deployment_restart_error = None;
        }
    }
}

pub(super) fn show_terminal_launch_error(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    settings: &TerminalLaunchSettings,
) {
    let Some(error) = ui_state.terminal_launch_error.clone() else {
        return;
    };
    match (ErrorDialog {
        id: egui::Id::new("terminal-launch-error"),
        eyebrow: "POD SHELL",
        title: "Couldn’t open a terminal",
        message: "Kubernetes Dev UI could not start an external terminal for this pod shell.",
        details: Some(&error),
        recovery: Some("Choose a custom command in Settings to use another installed terminal."),
        primary_action_label: Some("Open settings"),
    })
    .show(ctx)
    {
        ErrorDialogAction::PrimaryAction => {
            ui_state.open_terminal_settings(settings);
            ui_state.terminal_launch_error = None;
        }
        ErrorDialogAction::Dismiss => ui_state.terminal_launch_error = None,
        ErrorDialogAction::None => {}
    }
}
