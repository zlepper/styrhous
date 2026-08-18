use super::state::{BulkDeleteProgress, UiState};
use crate::terminal_launcher::TerminalLaunchSettings;
use crate::worker::*;
use components::colors::{WHITE, gray};
use components::design::{radius, spacing, surface, typography};
use components::{
    ButtonSize, ConfirmationDialog, ConfirmationDialogAcknowledgement, ConfirmationDialogAction,
    ConfirmationDialogKind, ConfirmationDialogWarning, ErrorDialog, ErrorDialogAction,
    PointingHand, TailwindButton,
};
use egui::{Align, Color32, Frame, Key, Margin, Modal, Modifiers, Shadow};
use std::time::Instant;
use tracing::info;

const SCALE_DIALOG_WIDTH: f32 = 530.0;

impl WorkerResult for ResourceDeleteFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        ui.settle_bulk_delete_target(
            self.cluster_key,
            self.bulk_delete_id,
            &self.api_resource,
            &self.resource_name,
            &self.namespace,
            Some(self.error),
        );
    }
}

impl WorkerResult for ResourceDeleteCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceDeleteCompleted {
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            bulk_delete_id,
        } = self;
        ui.settle_bulk_delete_target(
            cluster_key,
            bulk_delete_id,
            &api_resource,
            &resource_name,
            &namespace,
            None,
        );
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.pending_delete = None;
        }
    }
}

impl WorkerResult for ResourceForceDeleteFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.force_delete_error = Some(self.error);
        }
    }
}

impl WorkerResult for ResourceForceDeleteCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceForceDeleteCompleted {
            cluster_key,
            resource_name,
        } = self;
        info!("Finalizers removed from resource: {resource_name}");
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.pending_force_delete = None;
        }
    }
}

impl WorkerResult for DeploymentRestartFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.deployment_restart_error = Some(self.error);
        }
    }
}

impl WorkerResult for DeploymentRestartCompleted {
    fn apply(self, _ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        info!(
            "Deployment rollout restart requested: {} in {}",
            self.resource_name, self.namespace
        );
    }
}

impl WorkerResult for CronJobRunFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.cron_job_run_error = Some(self.error);
        }
    }
}

impl WorkerResult for CronJobRunCompleted {
    fn apply(self, _ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        info!(
            "Created one-off Job {} from CronJob {} in {}",
            self.job_name, self.cron_job_name, self.namespace
        );
    }
}

impl WorkerResult for ResourceScaleFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.scale_error = Some(self.error);
        }
    }
}

impl WorkerResult for ResourceScaleFetched {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let ResourceScaleFetched {
            cluster_key,
            api_resource,
            namespace,
            resource_name,
            replicas,
        } = self;
        if let Some(cluster) = ui.clusters.get_mut(&cluster_key) {
            cluster.pending_scale = Some(super::state::PendingScale {
                api_resource,
                resource_name,
                namespace,
                current_replicas: replicas,
                desired_replicas: replicas.to_string(),
            });
        }
    }
}

impl WorkerResult for ResourceScaleUpdated {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        info!("Scale updated for resource: {}", self.resource_name);
        if let Some(cluster) = ui.clusters.get_mut(&self.cluster_key) {
            cluster.pending_scale = None;
        }
    }
}

pub(super) fn show_delete_confirmation(
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
    let seconds = remaining.as_millis().div_ceil(1_000);
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
        warning: None,
        acknowledgement: None,
    }
    .show(ctx);

    if action == ConfirmationDialogAction::Cancel {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_delete = None;
        }
    } else if action == ConfirmationDialogAction::Confirm {
        commands_to_send.push(Box::new(DeleteResource {
            cluster_key,
            api_resource: pending.api_resource,
            namespace: pending.namespace,
            resource_name: pending.resource_name,
            resource_uid: None,
            bulk_delete_id: None,
        }));
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_delete = None;
        }
    }
}

pub(super) fn show_bulk_delete_confirmation(
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
    let Some(pending) = cluster.pending_bulk_delete.clone() else {
        return;
    };
    let cluster_key = cluster.cluster_key;
    let remaining = pending
        .confirmation_available_at
        .saturating_duration_since(Instant::now());
    let confirm_enabled = remaining.is_zero();
    if !confirm_enabled {
        ctx.request_repaint_after(remaining);
    }
    let target_count = pending.targets.len();
    let target_list = pending
        .targets
        .iter()
        .map(super::state::BulkDeleteTarget::display_name)
        .collect::<Vec<_>>()
        .join("\n");
    let title = format!("Delete {target_count} resources?");
    let message = format!("This will permanently delete {target_count} selected resources.");
    let confirm_label = format!("Delete {target_count} resources");
    let unavailable_message = (!confirm_enabled).then(|| {
        format!(
            "Delete will be available in {} seconds.",
            remaining.as_millis().div_ceil(1_000)
        )
    });
    let action = ConfirmationDialog {
        id: egui::Id::new("bulk-delete-resources-confirmation"),
        eyebrow: "DELETE RESOURCES",
        title: &title,
        message: &message,
        unavailable_message: unavailable_message.as_deref(),
        cancel_label: "Cancel",
        confirm_label: &confirm_label,
        kind: ConfirmationDialogKind::Destructive,
        confirm_enabled,
        warning: Some(ConfirmationDialogWarning {
            title: "Deletion targets",
            message: "Review every resource before confirming. This action cannot be undone.",
            details: Some(&target_list),
        }),
        acknowledgement: None,
    }
    .show(ctx);

    if action == ConfirmationDialogAction::Cancel {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_bulk_delete = None;
        }
    } else if action == ConfirmationDialogAction::Confirm {
        let bulk_delete_id = ui_state
            .clusters
            .get_mut(&cluster_id)
            .map(|cluster| {
                cluster.next_bulk_delete_id = cluster.next_bulk_delete_id.wrapping_add(1);
                cluster.next_bulk_delete_id
            })
            .expect("selected cluster still exists while confirming bulk deletion");
        for target in &pending.targets {
            commands_to_send.push(Box::new(DeleteResource {
                cluster_key,
                api_resource: pending.api_resource.clone(),
                namespace: target.namespace.clone(),
                resource_name: target.name.clone(),
                resource_uid: Some(target.uid.clone()),
                bulk_delete_id: Some(bulk_delete_id),
            }));
        }
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.bulk_delete_progress = Some(BulkDeleteProgress::new(
                bulk_delete_id,
                pending.api_resource,
                pending.targets,
            ));
            cluster.pending_bulk_delete = None;
        }
    }
}

pub(super) fn show_bulk_delete_error(ctx: &egui::Context, ui_state: &mut UiState) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(error) = ui_state
        .clusters
        .get(&cluster_id)
        .and_then(|cluster| cluster.bulk_delete_error.as_deref())
    else {
        return;
    };
    if matches!(
        (ErrorDialog {
            id: egui::Id::new("bulk-delete-resources-error"),
            eyebrow: "DELETE RESOURCES",
            title: "Some resources could not be deleted",
            message: "The remaining selected resources were not deleted.",
            details: Some(error),
            recovery: Some(
                "Check the resource state and your Kubernetes permissions, then try again."
            ),
            primary_action_label: None,
        })
        .show(ctx),
        ErrorDialogAction::Dismiss
    ) && let Some(cluster) = ui_state.clusters.get_mut(&cluster_id)
    {
        cluster.bulk_delete_error = None;
    }
}

pub(super) fn show_force_delete_confirmation(
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
    let Some(pending) = cluster.pending_force_delete.as_ref() else {
        return;
    };
    let cluster_key = cluster.cluster_key;
    let remaining = pending
        .confirmation_available_at
        .saturating_duration_since(Instant::now());
    let delay_elapsed = remaining.is_zero();
    if !delay_elapsed {
        ctx.request_repaint_after(remaining);
    }
    let finalizers = pending
        .finalizers
        .iter()
        .map(|finalizer| format!("• {finalizer}"))
        .collect::<Vec<_>>()
        .join("\n");
    let message = "This permanently removes every finalizer from this deleting resource.";
    let unavailable_message = (!delay_elapsed).then(|| {
        format!(
            "Remove finalizers will be available in {} seconds.",
            remaining.as_millis().div_ceil(1_000)
        )
    });
    let pending = ui_state
        .clusters
        .get_mut(&cluster_id)
        .and_then(|cluster| cluster.pending_force_delete.as_mut())
        .expect("pending force deletion was checked above");
    let acknowledgement_label = format!(
        "Type {} to acknowledge that you are bypassing cleanup:",
        pending.resource_name
    );
    let confirm_enabled = delay_elapsed && pending.acknowledgement == pending.resource_name;
    let action = ConfirmationDialog {
        id: egui::Id::new("force-delete-resource-confirmation"),
        eyebrow: "DANGEROUS: REMOVE FINALIZERS",
        title: "Force delete resource?",
        message,
        unavailable_message: unavailable_message.as_deref(),
        cancel_label: "Cancel",
        confirm_label: "Remove finalizers",
        kind: ConfirmationDialogKind::Destructive,
        confirm_enabled,
        warning: Some(ConfirmationDialogWarning {
            title: "This bypasses controller cleanup",
            message: "It can orphan external infrastructure, leak data, and corrupt application state. Only continue if you understand why the controllers cannot finish deletion.",
            details: Some(&finalizers),
        }),
        acknowledgement: Some(ConfirmationDialogAcknowledgement {
            label: &acknowledgement_label,
            hint_text: "Resource name",
            value: &mut pending.acknowledgement,
        }),
    }
    .show(ctx);

    if action == ConfirmationDialogAction::Cancel {
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_force_delete = None;
        }
    } else if action == ConfirmationDialogAction::Confirm {
        commands_to_send.push(Box::new(ForceDeleteResource {
            cluster_key,
            api_resource: pending.api_resource.clone(),
            namespace: pending.namespace.clone(),
            resource_name: pending.resource_name.clone(),
            resource_uid: pending.resource_uid.clone(),
        }));
        if let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) {
            cluster.pending_force_delete = None;
        }
    }
}

pub(super) fn show_force_delete_error(ctx: &egui::Context, ui_state: &mut UiState) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(error) = ui_state
        .clusters
        .get(&cluster_id)
        .and_then(|cluster| cluster.force_delete_error.as_deref())
    else {
        return;
    };
    if matches!(
        (ErrorDialog {
            id: egui::Id::new("force-delete-resource-error"),
            eyebrow: "REMOVE FINALIZERS",
            title: "Couldn’t remove finalizers",
            message: "Kubernetes Dev UI could not remove the finalizers from this resource.",
            details: Some(error),
            recovery: Some(
                "Check the resource’s current deletion state and your Kubernetes permissions."
            ),
            primary_action_label: None,
        })
        .show(ctx),
        ErrorDialogAction::Dismiss
    ) && let Some(cluster) = ui_state.clusters.get_mut(&cluster_id)
    {
        cluster.force_delete_error = None;
    }
}

pub(super) fn show_deployment_restart_confirmation(
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
    ) && let Some(cluster) = ui_state.clusters.get_mut(&cluster_id)
    {
        cluster.deployment_restart_error = None;
    }
}

pub(super) fn show_cron_job_run_confirmation(
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

pub(super) fn show_cron_job_run_error(ctx: &egui::Context, ui_state: &mut UiState) {
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
            message: "Kubernetes Dev UI could not create a Job from this CronJob.",
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

pub(super) fn show_scale_dialog(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) else {
        return;
    };
    let Some(pending) = cluster.pending_scale.as_mut() else {
        return;
    };

    let mut cancel = false;
    let mut scale_request = None;
    let response = Modal::new(egui::Id::new("resource-scale-dialog"))
        .area(
            Modal::default_area(egui::Id::new("resource-scale-dialog"))
                .default_width(SCALE_DIALOG_WIDTH)
                .fade_in(false),
        )
        .backdrop_color(Color32::from_black_alpha(122))
        .frame(
            Frame::new()
                .fill(WHITE)
                .stroke(surface::muted_border())
                .corner_radius(radius::surface())
                .shadow(Shadow {
                    offset: [0, 4],
                    blur: 18,
                    spread: 0,
                    color: Color32::BLACK.gamma_multiply(0.16),
                })
                .inner_margin(Margin::same(spacing::XL as i8)),
        )
        .show(ctx, |ui| {
            ui.set_width(SCALE_DIALOG_WIDTH);
            ui.label(
                egui::RichText::new(pending.api_resource.kind.to_ascii_uppercase())
                    .font(typography::metadata())
                    .color(gray::_500),
            );
            ui.add_space(spacing::SM);
            ui.label(
                egui::RichText::new(format!("Scale {}", pending.resource_name))
                    .font(typography::semibold(24.0))
                    .color(gray::_900),
            );
            ui.add_space(spacing::MD);
            let scope = pending.namespace.as_deref().map_or_else(
                || "the cluster".to_owned(),
                |namespace| format!("the {namespace} namespace"),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Set the desired replica count for this {} in {scope}.",
                    pending.api_resource.kind
                ))
                .font(typography::body())
                .color(gray::_600),
            );
            ui.add_space(spacing::LG);
            ui.label(
                egui::RichText::new("Desired replicas")
                    .font(typography::semibold(14.0))
                    .color(gray::_800),
            );
            ui.add_space(spacing::XS);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let field_width = ui.available_width() - 60.0;
                let desired_replicas = ui.add_sized(
                    egui::vec2(field_width, 30.0),
                    egui::TextEdit::singleline(&mut pending.desired_replicas)
                        .id(egui::Id::new("desired-replicas"))
                        .frame(
                            Frame::new()
                                .fill(WHITE)
                                .stroke(surface::control_border())
                                .corner_radius(radius::control())
                                .inner_margin(Margin::symmetric(spacing::SM as i8, 2)),
                        )
                        .font(typography::body())
                        .vertical_align(Align::Center),
                );
                ui.ctx()
                    .accesskit_node_builder(desired_replicas.id, |builder| {
                        builder.set_label("Desired replicas");
                    });
                Frame::new()
                    .fill(WHITE)
                    .stroke(surface::control_border())
                    .corner_radius(radius::control())
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let parsed = pending.desired_replicas.parse::<i32>().ok();
                        let decrement = show_scale_stepper_button(
                            ui,
                            "−",
                            "Decrease desired replicas",
                            parsed.is_some_and(|replicas| replicas > 0),
                        );
                        ui.separator();
                        let increment =
                            show_scale_stepper_button(ui, "+", "Increase desired replicas", true);
                        if decrement {
                            pending.desired_replicas = (parsed.unwrap_or_default() - 1).to_string();
                        }
                        if increment {
                            pending.desired_replicas = parsed
                                .unwrap_or(pending.current_replicas)
                                .saturating_add(1)
                                .to_string();
                        }
                    });
            });
            let replicas = pending
                .desired_replicas
                .parse::<i32>()
                .ok()
                .filter(|replicas| *replicas >= 0);
            ui.add_space(spacing::SM);
            ui.label(
                egui::RichText::new(format!(
                    "Current desired replicas: {}",
                    pending.current_replicas
                ))
                .font(typography::body())
                .color(gray::_500),
            );
            if replicas.is_none() {
                ui.add_space(spacing::XS);
                ui.label(
                    egui::RichText::new("Enter a whole number of zero or greater.")
                        .font(typography::metadata())
                        .color(components::design::status::DANGER),
                );
            }
            ui.add_space(spacing::XL);
            ui.separator();
            ui.add_space(spacing::MD);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled_ui(replicas.is_some(), |ui| {
                        TailwindButton::primary("Update scale")
                            .size(ButtonSize::Md)
                            .show(ui)
                    })
                    .inner
                    .clicked()
                {
                    scale_request = replicas.map(|replicas| {
                        (
                            pending.api_resource.clone(),
                            pending.namespace.clone(),
                            pending.resource_name.clone(),
                            replicas,
                        )
                    });
                }
                if TailwindButton::secondary("Cancel")
                    .size(ButtonSize::Md)
                    .show(ui)
                    .clicked()
                {
                    cancel = true;
                }
            });
        });

    let escape_pressed = response.is_top_modal
        && !response.any_popup_open
        && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
    if cancel || escape_pressed || scale_request.is_some() {
        cluster.pending_scale = None;
    }
    if let Some((api_resource, namespace, resource_name, replicas)) = scale_request {
        commands_to_send.push(Box::new(UpdateResourceScale {
            cluster_key: cluster.cluster_key,
            api_resource,
            namespace,
            resource_name,
            replicas,
        }));
    }
}

fn show_scale_stepper_button(ui: &mut egui::Ui, glyph: &str, label: &str, enabled: bool) -> bool {
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::Vec2::splat(28.0), egui::Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, radius::subtle(), components::colors::gray::_50);
            }
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                glyph,
                typography::body(),
                if enabled { gray::_700 } else { gray::_400 },
            );
            response
        })
        .inner
        .with_pointing_hand();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label.to_owned())
    });
    response.on_hover_text(label).clicked()
}

pub(super) fn show_scale_error(ctx: &egui::Context, ui_state: &mut UiState) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(error) = ui_state
        .clusters
        .get(&cluster_id)
        .and_then(|cluster| cluster.scale_error.as_deref())
    else {
        return;
    };
    if matches!(
        (ErrorDialog {
            id: egui::Id::new("resource-scale-error"),
            eyebrow: "SCALE",
            title: "Couldn’t update scale",
            message: "Kubernetes Dev UI could not read or update this resource’s scale.",
            details: Some(error),
            recovery: Some("Check the resource’s current state and your Kubernetes permissions."),
            primary_action_label: None,
        })
        .show(ctx),
        ErrorDialogAction::Dismiss
    ) && let Some(cluster) = ui_state.clusters.get_mut(&cluster_id)
    {
        cluster.scale_error = None;
    }
}

pub(super) fn show_terminal_launch_error(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    settings: &TerminalLaunchSettings,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let Some(error) = ui_state.terminal_launch_error.clone() else {
        return;
    };
    match (ErrorDialog {
        id: egui::Id::new("terminal-launch-error"),
        eyebrow: "SHELL",
        title: "Couldn’t open a terminal",
        message: "Kubernetes Dev UI could not start an external terminal for this shell.",
        details: Some(&error),
        recovery: Some("Choose a custom command in Settings to use another installed terminal."),
        primary_action_label: Some("Open settings"),
    })
    .show(ctx)
    {
        ErrorDialogAction::PrimaryAction => {
            ui_state.open_terminal_settings(settings, commands_to_send);
            ui_state.terminal_launch_error = None;
        }
        ErrorDialogAction::Dismiss => ui_state.terminal_launch_error = None,
        ErrorDialogAction::None => {}
    }
}
