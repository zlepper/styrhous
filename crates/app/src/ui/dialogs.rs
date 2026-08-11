use super::state::UiState;
use crate::terminal_launcher::TerminalLaunchSettings;
use crate::worker::WorkerCommand;
use components::colors::{WHITE, gray};
use components::design::{radius, spacing, surface, typography};
use components::{
    ButtonSize, ConfirmationDialog, ConfirmationDialogAcknowledgement, ConfirmationDialogAction,
    ConfirmationDialogKind, ConfirmationDialogWarning, ErrorDialog, ErrorDialogAction,
    PointingHand, TailwindButton,
};
use egui::{Align, Color32, Frame, Key, Margin, Modal, Modifiers, Shadow};
use std::time::Instant;

const SCALE_DIALOG_WIDTH: f32 = 530.0;

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

pub(super) fn show_force_delete_confirmation(
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
        commands_to_send.push(WorkerCommand::ForceDeleteResource {
            cluster_key,
            api_resource: pending.api_resource.clone(),
            namespace: pending.namespace.clone(),
            resource_name: pending.resource_name.clone(),
            resource_uid: pending.resource_uid.clone(),
        });
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
        warning: None,
        acknowledgement: None,
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
    ) && let Some(cluster) = ui_state.clusters.get_mut(&cluster_id)
    {
        cluster.deployment_restart_error = None;
    }
}

pub(super) fn show_scale_dialog(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
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
                ui.add_sized(
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
        commands_to_send.push(WorkerCommand::UpdateResourceScale {
            cluster_key: cluster.cluster_key,
            api_resource,
            namespace,
            resource_name,
            replicas,
        });
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
