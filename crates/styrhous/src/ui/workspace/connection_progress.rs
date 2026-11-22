//! The page shown until a cluster's API server has answered: what is being waited for, and
//! for how long, so a slow credential plugin or an unreachable server is visibly not stuck.

use super::super::state::{ClusterConnectionState, ClusterState};
use super::super::widgets::workspace_connection_progress;
use components::{ChecklistStep, ChecklistStepState, ProgressChecklist};

/// An in-progress step shows how long it has taken once that becomes noticeable.
const SHOW_ELAPSED_AFTER_SECONDS: f64 = 1.0;

/// Shows the connection's progress, and its error with a retry when it failed. Returns
/// whether retry was clicked.
pub(super) fn show_connection_progress(ui: &mut egui::Ui, cluster: &ClusterState) -> bool {
    let now = ui.input(|input| input.time);
    let name = cluster.presentation.display_name(&cluster.name);
    let steps = connection_steps(cluster, now);
    let (title, error) = match &cluster.connection {
        ClusterConnectionState::Failed(error) => {
            (format!("Could not connect to {name}"), Some(error.as_str()))
        }
        _ => (format!("Connecting to {name}"), None),
    };
    workspace_connection_progress(ui, ProgressChecklist::new(&title, &steps), error)
}

/// Loading the kubeconfig and credentials (which may run an exec plugin), then the first
/// authenticated answer from the API server. Durations are in seconds of `now`'s clock.
pub(super) fn connection_steps(cluster: &ClusterState, now: f64) -> Vec<ChecklistStep> {
    let progress = cluster.connection_progress;
    let failed = matches!(cluster.connection, ClusterConnectionState::Failed(_));
    // An answer from the API server also proves the credentials were loaded.
    let authenticated = progress.authenticated.or(progress.reachable);
    let authenticate = match authenticated {
        Some(at) => done(at - progress.started),
        None if failed => (ChecklistStepState::Failed, None),
        None => in_progress(now - progress.started),
    };
    let contact = match (authenticated, progress.reachable) {
        (Some(authenticated), Some(at)) => done(at - authenticated),
        (Some(_), None) if failed => (ChecklistStepState::Failed, None),
        (Some(authenticated), None) => in_progress(now - authenticated),
        (None, _) => (ChecklistStepState::Pending, None),
    };
    [
        ("Authenticate", authenticate),
        ("Contact API server", contact),
    ]
    .into_iter()
    .map(|(label, (state, detail))| ChecklistStep {
        label: label.to_owned(),
        state,
        detail,
    })
    .collect()
}

fn done(seconds: f64) -> (ChecklistStepState, Option<String>) {
    (ChecklistStepState::Done, Some(format_duration(seconds)))
}

fn in_progress(seconds: f64) -> (ChecklistStepState, Option<String>) {
    (
        ChecklistStepState::InProgress,
        (seconds >= SHOW_ELAPSED_AFTER_SECONDS).then(|| format_duration(seconds)),
    )
}

/// "0.4 s" below ten seconds, "12 s" from then on.
fn format_duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    if seconds < 10.0 {
        format!("{seconds:.1} s")
    } else {
        format!("{seconds:.0} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::state::ConnectionProgress;

    fn cluster(connection: ClusterConnectionState, progress: ConnectionProgress) -> ClusterState {
        let mut cluster = ClusterState::new(1, "dev".to_owned());
        cluster.connection = connection;
        cluster.connection_progress = progress;
        cluster
    }

    fn summary(steps: &[ChecklistStep]) -> Vec<(ChecklistStepState, Option<&str>)> {
        steps
            .iter()
            .map(|step| (step.state, step.detail.as_deref()))
            .collect()
    }

    #[test]
    fn authenticating_shows_its_elapsed_time_only_once_it_is_noticeable() {
        let connecting = cluster(
            ClusterConnectionState::Connecting,
            ConnectionProgress::started_at(10.0),
        );

        assert_eq!(
            summary(&connection_steps(&connecting, 10.5)),
            vec![
                (ChecklistStepState::InProgress, None),
                (ChecklistStepState::Pending, None)
            ]
        );
        assert_eq!(
            summary(&connection_steps(&connecting, 13.25)),
            vec![
                (ChecklistStepState::InProgress, Some("3.2 s")),
                (ChecklistStepState::Pending, None)
            ]
        );
    }

    #[test]
    fn contacting_the_api_server_follows_authentication() {
        let connected = cluster(
            ClusterConnectionState::Connected,
            ConnectionProgress {
                authenticated: Some(10.4),
                ..ConnectionProgress::started_at(10.0)
            },
        );

        assert_eq!(
            summary(&connection_steps(&connected, 22.4)),
            vec![
                (ChecklistStepState::Done, Some("0.4 s")),
                (ChecklistStepState::InProgress, Some("12 s"))
            ]
        );
    }

    #[test]
    fn the_step_that_was_running_is_the_one_that_failed() {
        let credentials_failed = cluster(
            ClusterConnectionState::Failed("exec plugin failed".to_owned()),
            ConnectionProgress::started_at(10.0),
        );
        let server_failed = cluster(
            ClusterConnectionState::Failed("Unauthorized".to_owned()),
            ConnectionProgress {
                authenticated: Some(10.4),
                ..ConnectionProgress::started_at(10.0)
            },
        );

        assert_eq!(
            summary(&connection_steps(&credentials_failed, 12.0)),
            vec![
                (ChecklistStepState::Failed, None),
                (ChecklistStepState::Pending, None)
            ]
        );
        assert_eq!(
            summary(&connection_steps(&server_failed, 12.0)),
            vec![
                (ChecklistStepState::Done, Some("0.4 s")),
                (ChecklistStepState::Failed, None)
            ]
        );
    }

    #[test]
    fn an_answer_before_the_connection_reported_completes_both_steps() {
        let answered = cluster(
            ClusterConnectionState::Connecting,
            ConnectionProgress {
                reachable: Some(10.3),
                ..ConnectionProgress::started_at(10.0)
            },
        );

        assert_eq!(
            summary(&connection_steps(&answered, 11.0)),
            vec![
                (ChecklistStepState::Done, Some("0.3 s")),
                (ChecklistStepState::Done, Some("0.0 s"))
            ]
        );
    }
}
