use super::*;

pub(crate) fn show_delete_confirmation(
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

pub(crate) fn show_bulk_delete_confirmation(
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
        .map(super::super::state::BulkDeleteTarget::display_name)
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

pub(crate) fn show_bulk_delete_error(ctx: &egui::Context, ui_state: &mut UiState) {
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

pub(crate) fn show_force_delete_confirmation(
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

pub(crate) fn show_force_delete_error(ctx: &egui::Context, ui_state: &mut UiState) {
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
            message: "Styrhous could not remove the finalizers from this resource.",
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
