//! The Node drain confirmation and the outcomes of Node maintenance.

use super::*;
use crate::cluster_connection_manager::{DrainPod, NodeDrainReport};
use crate::ui::state::PendingNodeDrain;
use crate::ui::widgets::count_noun;

pub(crate) fn show_node_drain_confirmation(
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
    let Some(PendingNodeDrain { node_name }) = cluster.pending_node_drain.clone() else {
        return;
    };
    // Another drain of the Node would only repeat its evictions and its outcome.
    let draining = cluster.node_drains.contains(&node_name);
    let message = Prose::new()
        .plain("Cordons ")
        .strong(node_name.as_str())
        .plain(" and evicts its Pods, honouring their PodDisruptionBudgets. DaemonSet and static Pods stay on the Node.");
    let action = ConfirmationDialog {
        id: egui::Id::new("drain-node-confirmation"),
        icon: components::icons::Icon::ArrowDownTray,
        title: "Drain Node?",
        message,
        unavailable_message: draining.then_some("This Node is still being drained."),
        cancel_label: "Cancel",
        confirm_label: "Drain",
        kind: ConfirmationDialogKind::Destructive,
        confirm_enabled: !draining,
        warning: None,
        acknowledgement: None,
    }
    .show(ctx);
    match action {
        ConfirmationDialogAction::None => {}
        ConfirmationDialogAction::Cancel => cluster.pending_node_drain = None,
        ConfirmationDialogAction::Confirm => {
            cluster.pending_node_drain = None;
            let operation_id = cluster.node_drains.start(node_name.clone());
            commands_to_send.push(Box::new(DrainNode {
                cluster_key: cluster.cluster_key,
                operation_id,
                node_name,
            }));
        }
    }
}

fn cordon_verb(cordon: bool) -> &'static str {
    if cordon { "cordon" } else { "uncordon" }
}

impl WorkerResult for NodeCordonCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if ui
            .settle_operation(self.cluster_key, self.operation_id, |cluster| {
                &mut cluster.node_cordons
            })
            .is_none()
        {
            return;
        }
        ui.record_operation_outcome(
            self.cluster_key,
            OperationOutcome::Success,
            format!("Node {}ed", cordon_verb(self.cordon)),
            None,
            self.node_name,
            None,
        );
    }
}

impl WorkerResult for NodeCordonFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if ui
            .settle_operation(self.cluster_key, self.operation_id, |cluster| {
                &mut cluster.node_cordons
            })
            .is_none()
        {
            return;
        }
        ui.record_operation_outcome(
            self.cluster_key,
            OperationOutcome::Failure,
            format!("Couldn’t {} Node", cordon_verb(self.cordon)),
            None,
            self.node_name,
            Some(self.error),
        );
    }
}

impl WorkerResult for NodeDrainCompleted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let Some(_) = ui.settle_operation(self.cluster_key, self.operation_id, |cluster| {
            &mut cluster.node_drains
        }) else {
            return;
        };
        let (outcome, title) = if self.report.is_complete() {
            (OperationOutcome::Success, "Node drained")
        } else if self.report.blocked.is_empty() {
            (OperationOutcome::Failure, "Node drain incomplete")
        } else {
            (OperationOutcome::Failure, "Couldn’t drain Node")
        };
        info!("Drained Node {}: {:?}", self.node_name, self.report);
        ui.record_operation_outcome(
            self.cluster_key,
            outcome,
            title,
            None,
            self.node_name,
            Some(drain_report_details(&self.report)),
        );
    }
}

impl WorkerResult for NodeDrainFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        let Some(_) = ui.settle_operation(self.cluster_key, self.operation_id, |cluster| {
            &mut cluster.node_drains
        }) else {
            return;
        };
        ui.record_operation_outcome(
            self.cluster_key,
            OperationOutcome::Failure,
            "Couldn’t drain Node",
            None,
            self.node_name,
            Some(self.error),
        );
    }
}

/// What a drain did, one statement per line, then the Pods it could not
/// remove with their reasons.
pub(crate) fn drain_report_details(report: &NodeDrainReport) -> String {
    let mut lines = Vec::new();
    if !report.blocked.is_empty() {
        lines.push(format!(
            "The Node is cordoned, but no Pods were evicted: {} would be lost.",
            count_noun(report.blocked.len(), "Pod")
        ));
        lines.extend(
            report
                .blocked
                .iter()
                .map(|(pod, blocker)| pod_line(pod, blocker.reason())),
        );
        return lines.join("\n");
    }
    let total = report.evicted.len() + report.failed.len();
    lines.push(if report.failed.is_empty() {
        format!("Evicted {}.", count_noun(total, "Pod"))
    } else {
        format!(
            "Evicted {} of {}. The Node stays cordoned.",
            report.evicted.len(),
            count_noun(total, "Pod")
        )
    });
    let kept = report.daemon_set_pods + report.mirror_pods;
    if kept > 0 {
        let kinds = [
            (report.daemon_set_pods, "DaemonSet"),
            (report.mirror_pods, "static"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, kind)| format!("{count} {kind}"))
        .collect::<Vec<_>>();
        lines.push(format!(
            "Left {} Pod{} running.",
            kinds.join(" and "),
            if kept == 1 { "" } else { "s" }
        ));
    }
    lines.extend(
        report
            .failed
            .iter()
            .map(|(pod, error)| pod_line(pod, error)),
    );
    lines.join("\n")
}

fn pod_line(pod: &DrainPod, reason: &str) -> String {
    format!("{}: {reason}", pod.display_name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster_connection_manager::DrainBlocker;

    fn pod(name: &str) -> DrainPod {
        DrainPod {
            namespace: "apps".to_owned(),
            name: name.to_owned(),
            uid: None,
        }
    }

    #[test]
    fn drain_details_count_evictions_and_the_pods_left_running() {
        assert_eq!(
            drain_report_details(&NodeDrainReport {
                evicted: vec![pod("api"), pod("web")],
                daemon_set_pods: 2,
                mirror_pods: 1,
                ..Default::default()
            }),
            "Evicted 2 Pods.\nLeft 2 DaemonSet and 1 static Pods running."
        );
        assert_eq!(
            drain_report_details(&NodeDrainReport {
                evicted: vec![pod("api")],
                failed: vec![(pod("db-0"), "Still refused after 120s: budget".to_owned())],
                daemon_set_pods: 1,
                ..Default::default()
            }),
            "Evicted 1 of 2 Pods. The Node stays cordoned.\nLeft 1 DaemonSet Pod running.\n\
             apps/db-0: Still refused after 120s: budget"
        );
    }

    #[test]
    fn drain_details_name_the_pods_that_stopped_the_drain() {
        assert_eq!(
            drain_report_details(&NodeDrainReport {
                blocked: vec![(pod("bare"), DrainBlocker::NoController)],
                ..Default::default()
            }),
            "The Node is cordoned, but no Pods were evicted: 1 Pod would be lost.\n\
             apps/bare: declares no controller, so nothing would recreate it"
        );
    }
}
