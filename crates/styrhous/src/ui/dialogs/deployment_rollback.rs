use super::*;
use crate::cluster_connection_manager::{
    DeploymentRevision, DeploymentRevisionHistory, DeploymentRollbackOutcome, TemplateChange,
};
use crate::minimal_resource::format_age;
use crate::ui::state::{DeploymentKey, DeploymentRevisionsLoad};
use components::{Combobox, DetailColumn, DetailTableRow, InspectorDetails};

/// Wide enough for image references in the change table.
const DIALOG_WIDTH: f32 = 640.0;
/// Keeps Cancel and Confirm on screen however long the diff is.
const CHANGES_MAX_HEIGHT: f32 = 280.0;

fn revision_label(revision: &DeploymentRevision) -> String {
    let images = if revision.images.is_empty() {
        "no images".to_owned()
    } else {
        revision.images.join(", ")
    };
    format!(
        "Revision {} · {images} · {}",
        revision.revision,
        format_age(revision.created)
    )
}

fn change_rows(changes: &[TemplateChange]) -> Vec<DetailTableRow<'_>> {
    changes
        .iter()
        .map(|change| {
            DetailTableRow::new([
                change.container.as_str(),
                change.field.as_str(),
                change.current.as_deref().unwrap_or("-"),
                change.target.as_deref().unwrap_or("-"),
            ])
        })
        .collect()
}

fn show_changes(ui: &mut egui::Ui, revision: &DeploymentRevision) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new("Changes")
            .font(type_scale::BODY_STRONG.font_id(ui.ctx()))
            .color(theme.text.primary),
    );
    ui.add_space(space::S8);
    if revision.changes.is_empty() {
        ui.label(
            egui::RichText::new(if revision.other_changes {
                "Images, environment values and resources are unchanged."
            } else {
                "The pod template is identical to the current one."
            })
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.secondary),
        );
    } else {
        components::scroll::vertical()
            .id_salt("deployment-rollback-changes")
            .max_height(CHANGES_MAX_HEIGHT)
            .show(ui, |ui| {
                InspectorDetails::show_table(
                    ui,
                    &[
                        DetailColumn::new("Container").weight(0.8),
                        DetailColumn::new("Field").weight(1.6),
                        DetailColumn::new("Current").weight(2.0),
                        DetailColumn::new("After rollback").weight(2.0),
                    ],
                    &change_rows(&revision.changes),
                );
            });
    }
    if revision.other_changes {
        ui.add_space(space::S8);
        ui.label(
            egui::RichText::new("Other pod template fields differ as well.")
                .font(type_scale::META.font_id(ui.ctx()))
                .color(theme.text.muted),
        );
    }
}

/// The revision picker and the selected revision's changes.
fn show_revisions(
    ui: &mut egui::Ui,
    history: &DeploymentRevisionHistory,
    selected_revision: &mut Option<i64>,
) {
    let theme = ui.tokens();
    let candidates = history
        .revisions
        .iter()
        .filter(|revision| !revision.is_current)
        .collect::<Vec<_>>();
    let Some(selected) = candidates
        .iter()
        .find(|revision| Some(revision.revision) == *selected_revision)
    else {
        ui.label(
            egui::RichText::new("This Deployment has no earlier revision to roll back to.")
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(theme.text.secondary),
        );
        return;
    };
    let options = candidates
        .iter()
        .map(|revision| (revision.revision, revision_label(revision)))
        .collect::<Vec<_>>();
    Combobox::new("deployment-rollback-revision")
        .accessibility_label("Revision")
        .placeholder("Search revisions...")
        .search_accessibility_label("Search revisions")
        .selected_text(revision_label(selected))
        .width(ui.available_width())
        .filter_by(|(_, label): &(i64, String)| label)
        .show_items(ui, &options, |combobox, (revision, label)| {
            if combobox
                .item(label.as_str(), *revision == selected.revision)
                .clicked()
            {
                *selected_revision = Some(*revision);
            }
        });
    ui.add_space(space::S12);
    show_changes(ui, selected);
}

fn show_body(ui: &mut egui::Ui, revisions: &mut DeploymentRevisionsLoad) {
    let theme = ui.tokens();
    ui.add_space(space::S16);
    match revisions {
        DeploymentRevisionsLoad::Loading => {
            ui.label(
                egui::RichText::new("Loading revisions…")
                    .font(type_scale::BODY.font_id(ui.ctx()))
                    .color(theme.text.secondary),
            );
        }
        DeploymentRevisionsLoad::Failed(error) => {
            ui.label(
                egui::RichText::new(format!("Couldn’t load revisions: {error}"))
                    .font(type_scale::BODY.font_id(ui.ctx()))
                    .color(theme.danger.text),
            );
        }
        DeploymentRevisionsLoad::Loaded { history, selected } => {
            show_revisions(ui, history, selected);
        }
    }
}

pub(crate) fn show_deployment_rollback_dialog(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let Some(cluster) = ui_state
        .selected_cluster
        .and_then(|cluster_key| ui_state.clusters.get_mut(&cluster_key))
    else {
        return;
    };
    let Some(pending) = cluster.resources.pending_deployment_rollback.as_mut() else {
        return;
    };
    let (paused, target) = match &pending.revisions {
        DeploymentRevisionsLoad::Loaded { history, selected } => (history.paused, *selected),
        DeploymentRevisionsLoad::Loading | DeploymentRevisionsLoad::Failed(_) => (false, None),
    };
    let rolling_back = cluster
        .resources
        .deployment_rollbacks
        .contains(&pending.deployment);
    let unavailable_message = if paused {
        Some("Resume the Deployment's rollout before rolling it back.")
    } else if rolling_back {
        Some("A rollback of this Deployment is still running.")
    } else {
        None
    };
    let title = format!("Roll back {}?", pending.deployment.resource_name);
    let message = format!(
        "The selected revision's pod template replaces the current one in the {} namespace, \
         and the Deployment rolls it out as a new revision.",
        pending.deployment.namespace
    );
    let confirm_label = target.map_or_else(
        || "Roll back".to_owned(),
        |revision| format!("Roll back to revision {revision}"),
    );
    let action = ConfirmationDialog {
        id: egui::Id::new("deployment-rollback-dialog"),
        icon: components::icons::Icon::ArrowUturnLeft,
        title: &title,
        message: (&message).into(),
        unavailable_message,
        cancel_label: "Cancel",
        confirm_label: &confirm_label,
        kind: ConfirmationDialogKind::Primary,
        confirm_enabled: target.is_some() && unavailable_message.is_none(),
        warning: None,
        acknowledgement: None,
    }
    .show_with_body(ctx, DIALOG_WIDTH, |ui| {
        show_body(ui, &mut pending.revisions)
    });

    let resources = &mut cluster.resources;
    match (action, target) {
        (ConfirmationDialogAction::Cancel, _) => resources.pending_deployment_rollback = None,
        (ConfirmationDialogAction::Confirm, Some(revision)) => {
            let Some(pending) = resources.pending_deployment_rollback.take() else {
                return;
            };
            let operation_id = resources
                .deployment_rollbacks
                .start(pending.deployment.clone());
            commands_to_send.push(Box::new(RollbackDeployment {
                cluster_key: cluster.cluster_key,
                operation_id,
                namespace: pending.deployment.namespace,
                resource_name: pending.deployment.resource_name,
                revision,
            }));
        }
        (ConfirmationDialogAction::Confirm, None) | (ConfirmationDialogAction::None, _) => {}
    }
}

impl WorkerResult for DeploymentRevisionsFetched {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        ui.settle_deployment_revisions(self.cluster_key, self.request_id, Ok(self.history));
    }
}

impl WorkerResult for DeploymentRevisionsFetchFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        ui.settle_deployment_revisions(self.cluster_key, self.request_id, Err(self.error));
    }
}

impl DeploymentRollbackOutcome {
    fn title_and_details(self, revision: i64) -> (&'static str, String) {
        match self {
            Self::RolledBack => (
                "Deployment rolled back",
                format!("Rolling out revision {revision}"),
            ),
            Self::AlreadyAtRevision => (
                "Deployment already at that revision",
                format!("The pod template already matches revision {revision}"),
            ),
        }
    }
}

fn settle_deployment_rollback(
    ui: &mut UiState,
    cluster_key: i32,
    operation_id: u64,
) -> Option<DeploymentKey> {
    ui.settle_operation(cluster_key, operation_id, |cluster| {
        &mut cluster.deployment_rollbacks
    })
}

impl WorkerResult for DeploymentRolledBack {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let Some(deployment) = settle_deployment_rollback(ui, self.cluster_key, self.operation_id)
        else {
            return;
        };
        let (title, details) = self.outcome.title_and_details(self.revision);
        ui.record_operation_outcome(
            self.cluster_key,
            OperationOutcome::Success,
            title,
            Some(&deployment.namespace),
            deployment.resource_name,
            Some(details),
        );
    }
}

impl WorkerResult for DeploymentRollbackFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let Some(deployment) = settle_deployment_rollback(ui, self.cluster_key, self.operation_id)
        else {
            return;
        };
        ui.record_operation_outcome(
            self.cluster_key,
            OperationOutcome::Failure,
            format!("Couldn’t roll back to revision {}", self.revision),
            Some(&deployment.namespace),
            deployment.resource_name,
            Some(self.error),
        );
    }
}
