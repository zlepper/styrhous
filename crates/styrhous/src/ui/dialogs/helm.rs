use super::*;
use crate::cluster_connection_manager::HelmRollbackTarget;
use crate::helm_release::HelmRelease;
use crate::ui::state::PendingHelmRollback;

pub(crate) const HELM_INSTALL_GUIDANCE: &str =
    "Install Helm and make sure `helm` is on PATH: https://helm.sh/docs/intro/install/";

fn revision_label(release: &HelmRelease) -> String {
    let chart = match (release.chart.is_empty(), release.chart_version.is_empty()) {
        (true, _) => "unknown chart".to_owned(),
        (false, true) => release.chart.clone(),
        (false, false) => format!("{} {}", release.chart, release.chart_version),
    };
    format!("revision {} ({chart})", release.revision)
}

/// The dialog's text and whether it may confirm, from the release's revisions
/// as they are now: another rollback or upgrade can land while it is open.
struct RollbackPlan<'a> {
    message: String,
    unavailable_message: Option<&'static str>,
    current: Option<&'a HelmRelease>,
}

fn rollback_plan<'a>(
    releases: &'a [HelmRelease],
    pending: &PendingHelmRollback,
) -> RollbackPlan<'a> {
    let current = releases.iter().max_by_key(|release| release.revision);
    let target = releases
        .iter()
        .find(|release| release.revision == pending.target_revision);
    let name = &pending.release.release_name;
    let namespace = &pending.release.namespace;
    match (current, target) {
        (Some(current), Some(target)) => RollbackPlan {
            message: format!(
                "Helm will create a new revision of {name} in the {namespace} namespace from {}. \
                 The current one is {}.",
                revision_label(target),
                revision_label(current),
            ),
            unavailable_message: (current.revision == target.revision)
                .then_some("This revision is already the current one."),
            current: Some(current),
        },
        _ => RollbackPlan {
            message: format!(
                "Revision {} of {name} in the {namespace} namespace is no longer in the release history.",
                pending.target_revision
            ),
            unavailable_message: Some("There is nothing to roll back to."),
            current: None,
        },
    }
}

pub(crate) fn show_helm_rollback_confirmation(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let Some(cluster_id) = ui_state.selected_cluster else {
        return;
    };
    let Some(pending) = ui_state
        .clusters
        .get(&cluster_id)
        .and_then(|cluster| cluster.pending_helm_rollback.clone())
    else {
        return;
    };
    let releases = ui_state.helm_releases(
        cluster_id,
        &pending.release.namespace,
        &pending.release.release_name,
    );
    let plan = rollback_plan(&releases, &pending);
    let title = format!(
        "Roll back {} to revision {}?",
        pending.release.release_name, pending.target_revision
    );
    let action = ConfirmationDialog {
        id: egui::Id::new("helm-rollback-confirmation"),
        eyebrow: "HELM RELEASE",
        title: &title,
        message: &plan.message,
        unavailable_message: plan.unavailable_message,
        cancel_label: "Cancel",
        confirm_label: "Roll back",
        kind: ConfirmationDialogKind::Primary,
        confirm_enabled: plan.unavailable_message.is_none(),
        warning: None,
        acknowledgement: None,
    }
    .show(ctx);

    let Some(cluster) = ui_state.clusters.get_mut(&cluster_id) else {
        return;
    };
    match (action, plan.current) {
        (ConfirmationDialogAction::None, _) => {}
        (ConfirmationDialogAction::Cancel, _) => cluster.pending_helm_rollback = None,
        (ConfirmationDialogAction::Confirm, Some(current)) => {
            cluster.pending_helm_rollback = None;
            let operation_id = cluster.helm_rollbacks.start(pending.release.clone());
            commands_to_send.push(Box::new(RollbackHelmRelease {
                cluster_key: cluster.cluster_key,
                operation_id,
                target: HelmRollbackTarget {
                    // Shells and `helm` resolve the context from the same kubeconfig.
                    kube_context: cluster.name.clone(),
                    release: pending.release,
                    storage: current.storage,
                    revision: pending.target_revision,
                },
            }));
        }
        (ConfirmationDialogAction::Confirm, None) => {}
    }
}

impl WorkerResult for HelmRollbackCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if ui
            .settle_operation(self.cluster_key, self.operation_id, |cluster| {
                &mut cluster.helm_rollbacks
            })
            .is_some()
        {
            ui.record_operation_outcome(
                self.cluster_key,
                OperationOutcome::Success,
                "Helm release rolled back",
                Some(&self.release.namespace),
                self.release.release_name,
                Some(format!("Rolled back to revision {}", self.revision)),
            );
        }
    }
}

impl WorkerResult for HelmRollbackFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if ui
            .settle_operation(self.cluster_key, self.operation_id, |cluster| {
                &mut cluster.helm_rollbacks
            })
            .is_some()
        {
            let details = if self.helm_missing {
                HELM_INSTALL_GUIDANCE.to_owned()
            } else {
                self.error
            };
            ui.record_operation_outcome(
                self.cluster_key,
                OperationOutcome::Failure,
                format!("Couldn’t roll back to revision {}", self.revision),
                Some(&self.release.namespace),
                self.release.release_name,
                Some(details),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_labels_fall_back_for_missing_chart_metadata() {
        let release = |chart: &str, chart_version: &str| HelmRelease {
            storage: crate::helm_release::StorageDriver::Secret,
            storage_name: String::new(),
            name: "demo".to_owned(),
            namespace: "default".to_owned(),
            revision: 4,
            status: String::new(),
            description: String::new(),
            notes: String::new(),
            chart: chart.to_owned(),
            chart_version: chart_version.to_owned(),
            app_version: String::new(),
            first_deployed: String::new(),
            last_deployed: String::new(),
            values: serde_json::Value::Null,
            manifest: String::new(),
            storage_labels: Default::default(),
            storage_annotations: Default::default(),
        };
        assert_eq!(
            revision_label(&release("nginx", "1.2.3")),
            "revision 4 (nginx 1.2.3)"
        );
        assert_eq!(revision_label(&release("nginx", "")), "revision 4 (nginx)");
        assert_eq!(
            revision_label(&release("", "1.2.3")),
            "revision 4 (unknown chart)"
        );
    }
}
