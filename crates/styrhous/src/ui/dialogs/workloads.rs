use super::*;

pub(crate) fn show_deployment_restart_confirmation(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
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
        warning: None,
        acknowledgement: None,
    }
    .show(ctx);

    if action == ConfirmationDialogAction::Cancel {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_deployment_restart = None;
        }
    } else if action == ConfirmationDialogAction::Confirm {
        commands_to_send.push(Box::new(RestartDeployment {
            cluster_key,
            namespace: pending.namespace,
            resource_name: pending.resource_name,
        }));
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_deployment_restart = None;
        }
    }
}

pub(crate) fn show_deployment_restart_error(ctx: &egui::Context, ui_state: &mut UiState) {
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
            message: "Styrhous could not request a rolling restart for this Deployment.",
            details: Some(error),
            recovery: Some("Check your Kubernetes permissions and the Deployment’s current state."),
            primary_action_label: None,
        })
        .show(ctx),
        ErrorDialogAction::Dismiss
    ) && let Some(cluster) = ui_state.clusters.get_mut(&cluster_id)
    {
        cluster.deployment_restart_error = None;
    }
}

pub(crate) fn show_cron_job_run_confirmation(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(cluster) = ui_state.clusters.get(&cluster_id) else {
        return;
    };
    let Some(pending) = cluster.pending_cron_job_run.clone() else {
        return;
    };
    let cluster_key = cluster.cluster_key;
    let message = format!(
        "This creates and starts one Job from the current template for {} in the {} namespace.",
        pending.resource_name, pending.namespace
    );
    let action = ConfirmationDialog {
        id: egui::Id::new("run-cron-job-confirmation"),
        eyebrow: "CRONJOB",
        title: "Run CronJob now?",
        message: &message,
        unavailable_message: None,
        cancel_label: "Cancel",
        confirm_label: "Run now",
        kind: ConfirmationDialogKind::Primary,
        confirm_enabled: true,
        warning: None,
        acknowledgement: None,
    }
    .show(ctx);

    if action == ConfirmationDialogAction::Cancel {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_cron_job_run = None;
        }
    } else if action == ConfirmationDialogAction::Confirm {
        commands_to_send.push(Box::new(RunCronJob {
            cluster_key,
            namespace: pending.namespace,
            resource_name: pending.resource_name,
        }));
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_cron_job_run = None;
        }
    }
}

pub(crate) fn show_cron_job_run_error(ctx: &egui::Context, ui_state: &mut UiState) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(error) = ui_state
        .clusters
        .get(&cluster_id)
        .and_then(|cluster| cluster.cron_job_run_error.as_deref())
    else {
        return;
    };
    if matches!(
        (ErrorDialog {
            id: egui::Id::new("run-cron-job-error"),
            eyebrow: "CRONJOB",
            title: "Couldn’t run CronJob",
            message: "Styrhous could not create a Job from this CronJob.",
            details: Some(error),
            recovery: Some("Check your Kubernetes permissions and the CronJob’s current state."),
            primary_action_label: None,
        })
        .show(ctx),
        ErrorDialogAction::Dismiss
    ) && let Some(cluster) = ui_state.clusters.get_mut(&cluster_id)
    {
        cluster.cron_job_run_error = None;
    }
}
