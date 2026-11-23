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
    let message = Prose::new()
        .plain("Starts a rolling replacement of the pods for ")
        .strong(pending.resource_name.as_str())
        .plain(" in ")
        .strong(pending.namespace.as_str())
        .plain(".");
    let action = ConfirmationDialog {
        id: egui::Id::new("restart-deployment-rollout-confirmation"),
        icon: components::icons::Icon::ArrowPath,
        title: "Restart rollout?",
        message,
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
    let message = Prose::new()
        .plain("Starts one Job from the current template of ")
        .strong(pending.resource_name.as_str())
        .plain(" in ")
        .strong(pending.namespace.as_str())
        .plain(".");
    let action = ConfirmationDialog {
        id: egui::Id::new("run-cron-job-confirmation"),
        icon: components::icons::Icon::Play,
        title: "Run CronJob now?",
        message,
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
        let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) else {
            return;
        };
        let operation_id = cluster.cron_job_runs.start(());
        cluster.pending_cron_job_run = None;
        commands_to_send.push(Box::new(RunCronJob {
            cluster_key,
            operation_id,
            namespace: pending.namespace,
            resource_name: pending.resource_name,
        }));
    }
}
