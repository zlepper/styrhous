//! Helm release rollback through the locally installed `helm` CLI.
//!
//! A native rollback would have to replicate Helm's release-record bookkeeping
//! (superseding records, hooks, three-way merges), so the worker delegates to
//! `helm rollback`, the same way shells delegate to `kubectl`.

use super::cli::{CliCommand, CliTimedOut, ensure_success, run_cli};
use crate::helm_release::{HelmReleaseKey, StorageDriver};
use anyhow::Result;
use std::time::Duration;

/// Helm's own limit for each Kubernetes operation (such as one hook Job).
const HELM_OPERATION_TIMEOUT: &str = "5m0s";
/// The process limit is far above Helm's, because killing `helm` mid-rollback
/// leaves the release stuck in a pending state.
const HELM_PROCESS_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HelmRollbackTarget {
    pub(crate) kube_context: String,
    pub(crate) release: HelmReleaseKey,
    pub(crate) storage: StorageDriver,
    pub(crate) revision: i64,
}

impl HelmRollbackTarget {
    pub(crate) fn command(&self) -> CliCommand {
        CliCommand::new(
            "helm",
            [
                "rollback".to_owned(),
                format!("--namespace={}", self.release.namespace),
                format!("--kube-context={}", self.kube_context),
                format!("--timeout={HELM_OPERATION_TIMEOUT}"),
                "--".to_owned(),
                self.release.release_name.clone(),
                self.revision.to_string(),
            ],
            HELM_PROCESS_TIMEOUT,
        )
        // Helm only finds release records in the storage backend it is told
        // to use; ConfigMap-backed releases are invisible to the default one.
        .env("HELM_DRIVER", self.storage.helm_driver_name())
    }
}

pub(crate) async fn rollback_helm_release(target: &HelmRollbackTarget) -> Result<()> {
    let command = target.command();
    let output = run_cli(&command).await.map_err(|error| {
        if error.downcast_ref::<CliTimedOut>().is_some() {
            error.context(
                "Helm was stopped before it finished; the release may be left pending-rollback",
            )
        } else {
            error
        }
    })?;
    ensure_success(&command.program, &command.args, output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(storage: StorageDriver) -> HelmRollbackTarget {
        HelmRollbackTarget {
            kube_context: "kind-kind".to_owned(),
            release: HelmReleaseKey {
                namespace: "payments".to_owned(),
                release_name: "api".to_owned(),
            },
            storage,
            revision: 3,
        }
    }

    #[test]
    fn rollback_command_targets_the_release_context_and_storage_driver() {
        let command = target(StorageDriver::Secret).command();
        assert_eq!(command.program, "helm");
        assert_eq!(
            command.args,
            [
                "rollback",
                "--namespace=payments",
                "--kube-context=kind-kind",
                "--timeout=5m0s",
                "--",
                "api",
                "3"
            ]
        );
        assert_eq!(
            command.env,
            [("HELM_DRIVER".to_owned(), "secret".to_owned())]
        );
        assert!(command.timeout > Duration::from_secs(5 * 60));

        assert_eq!(
            target(StorageDriver::ConfigMap).command().env,
            [("HELM_DRIVER".to_owned(), "configmap".to_owned())]
        );
    }
}
