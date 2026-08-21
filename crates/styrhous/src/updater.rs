//! Self-contained application update support.
//!
//! The updater is deliberately separate from the Kubernetes worker. It is compiled only when
//! the `self-updater` feature is enabled and is inert unless CI marks the binary as a release
//! build. That keeps local `cargo run` and test builds from ever contacting GitHub Releases.

#[cfg(any(feature = "self-updater", test))]
use std::env;

#[cfg(any(feature = "self-updater", test))]
const DISABLE_AUTO_UPDATE_ENV: &str = "STYRHOUS_DISABLE_AUTO_UPDATE";

#[allow(
    dead_code,
    reason = "individual variants are constructed only in release or feature-disabled builds"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateStatus {
    /// This binary was built locally, so it intentionally has no update channel.
    LocalBuild,
    /// The updater feature was omitted from this binary.
    NotIncluded,
    /// An administrator or package manager owns upgrades.
    ExternallyManaged,
    /// The release binary is resolving the current release manifest.
    Checking,
    /// A verified update is being downloaded off the UI thread.
    Downloading { version: String },
    /// A verified update will be installed before the next app window opens.
    Staged { version: String },
    /// The installed build is current.
    UpToDate,
    /// An update action failed without affecting normal application startup.
    Failed { message: String },
}

impl UpdateStatus {
    pub(crate) fn shows_badge(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading { .. } | Self::Staged { .. } | Self::Failed { .. }
        )
    }

    pub(crate) fn summary(&self) -> String {
        match self {
            Self::LocalBuild => "Updates are disabled for local builds.".into(),
            Self::NotIncluded => "This build does not include automatic updates.".into(),
            Self::ExternallyManaged => "Updates are managed externally.".into(),
            Self::Checking => "Checking for updates…".into(),
            Self::Downloading { version } => format!("Downloading version {version}…"),
            Self::Staged { version } => {
                format!("Version {version} is ready and will be installed on the next launch.")
            }
            Self::UpToDate => "You are running the latest version.".into(),
            Self::Failed { message } => format!("Automatic update failed: {message}"),
        }
    }
}

pub(crate) struct UpdaterService {
    status: UpdateStatus,
    #[cfg(feature = "self-updater")]
    results: Option<std::sync::mpsc::Receiver<UpdateStatus>>,
}

impl Default for UpdaterService {
    fn default() -> Self {
        Self {
            status: default_status(),
            #[cfg(feature = "self-updater")]
            results: None,
        }
    }
}

impl UpdaterService {
    pub(crate) fn start() -> Self {
        #[cfg(feature = "self-updater")]
        {
            let (status, public_key) = updater_activation();
            let Some(public_key) = public_key else {
                return Self {
                    status,
                    results: None,
                };
            };
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("styrhous-updater".into())
                .spawn(move || {
                    let status = download_latest_update(public_key, &sender);
                    let _ = sender.send(status);
                })
                .expect("failed to start updater thread");

            Self {
                status,
                results: Some(receiver),
            }
        }

        #[cfg(not(feature = "self-updater"))]
        Self::default()
    }

    pub(crate) fn poll(&mut self) {
        #[cfg(feature = "self-updater")]
        {
            let mut completed_status = None;
            if let Some(results) = &self.results {
                while let Ok(status) = results.try_recv() {
                    completed_status = Some(status);
                }
            }
            if let Some(status) = completed_status {
                self.status = status;
                self.results = None;
            }
        }
    }

    pub(crate) fn status(&self) -> &UpdateStatus {
        &self.status
    }

    #[cfg(test)]
    pub(crate) fn set_status_for_test(&mut self, status: UpdateStatus) {
        self.status = status;
    }
}

/// Installs a previously verified update before the application creates an eframe window.
///
/// Errors are deliberately non-fatal: a failed updater must never prevent access to a cluster.
pub(crate) fn apply_staged_update() {
    #[cfg(feature = "self-updater")]
    if let (_, Some(public_key)) = updater_activation()
        && let Err(error) = apply_staged_update_inner(public_key)
    {
        tracing::warn!(error = %error, "unable to apply staged application update");
    }
}

fn default_status() -> UpdateStatus {
    #[cfg(not(feature = "self-updater"))]
    {
        return UpdateStatus::NotIncluded;
    }

    #[cfg(feature = "self-updater")]
    {
        updater_activation().0
    }
}

#[cfg(any(feature = "self-updater", test))]
fn updates_disabled_by_environment() -> bool {
    env::var(DISABLE_AUTO_UPDATE_ENV)
        .ok()
        .is_some_and(|value| updates_disabled_by_value(&value))
}

#[cfg(any(feature = "self-updater", test))]
fn updates_disabled_by_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

#[cfg(feature = "self-updater")]
fn updater_activation() -> (UpdateStatus, Option<&'static str>) {
    if updates_disabled_by_environment() || cfg!(styrhous_package_managed_build) {
        return (UpdateStatus::ExternallyManaged, None);
    }
    if !cfg!(styrhous_release_build) {
        return (UpdateStatus::LocalBuild, None);
    }

    match option_env!("STYRHOUS_UPDATER_PUBLIC_KEY").filter(|key| !key.trim().is_empty()) {
        Some(public_key) => (UpdateStatus::Checking, Some(public_key)),
        None => (
            UpdateStatus::Failed {
                message: "this release was built without an updater public key".into(),
            },
            None,
        ),
    }
}

#[cfg(feature = "self-updater")]
mod implementation;

#[cfg(feature = "self-updater")]
fn download_latest_update(
    public_key: &'static str,
    status_sender: &std::sync::mpsc::Sender<UpdateStatus>,
) -> UpdateStatus {
    implementation::download_latest_update(public_key, status_sender)
}

#[cfg(feature = "self-updater")]
fn apply_staged_update_inner(public_key: &'static str) -> anyhow::Result<()> {
    implementation::apply_staged_update(public_key)
}

#[cfg(test)]
mod tests;
