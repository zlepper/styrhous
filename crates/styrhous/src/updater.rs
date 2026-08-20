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
mod implementation {
    use super::UpdateStatus;
    use anyhow::{Context, Result, anyhow};
    use base64::Engine;
    use cargo_packager_updater::{
        Config, Update, UpdateFormat, UpdaterBuilder, semver::Version, url::Url,
    };
    use minisign_verify::{PublicKey, Signature};
    use serde::{Deserialize, Serialize};
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    const UPDATE_ENDPOINT: &str = "https://github.com/zlepper/styrhous/releases/latest/download/styrhous-update-{{target}}-{{arch}}.json";
    const UPDATE_STATE_NAME: &str = "pending-update.yaml";
    const MAX_APPLY_ATTEMPTS: u8 = 2;

    #[derive(Debug, Serialize, Deserialize)]
    struct PendingUpdate {
        version: String,
        attempts: u8,
        package_name: String,
        signature: String,
    }

    pub(super) fn download_latest_update(
        public_key: &'static str,
        status_sender: &std::sync::mpsc::Sender<UpdateStatus>,
    ) -> UpdateStatus {
        match download_latest_update_inner(public_key, status_sender) {
            Ok(status) => status,
            Err(error) => UpdateStatus::Failed {
                message: error.to_string(),
            },
        }
    }

    fn download_latest_update_inner(
        public_key: &'static str,
        status_sender: &std::sync::mpsc::Sender<UpdateStatus>,
    ) -> Result<UpdateStatus> {
        let updater = updater(public_key)?;
        let Some(update) = updater.check().context("could not check for updates")? else {
            return Ok(UpdateStatus::UpToDate);
        };
        let version = update.version.clone();
        if has_staged_update(&version)? {
            return Ok(UpdateStatus::Staged { version });
        }
        let _ = status_sender.send(UpdateStatus::Downloading {
            version: version.clone(),
        });
        let package = update
            .download()
            .with_context(|| format!("could not download version {version}"))?;
        stage_update(&update, &package)?;
        Ok(UpdateStatus::Staged { version })
    }

    pub(super) fn apply_staged_update(public_key: &'static str) -> Result<()> {
        let Some(mut pending) = read_pending_update()? else {
            return Ok(());
        };
        if pending.attempts >= MAX_APPLY_ATTEMPTS {
            clear_pending_update()?;
            return Err(anyhow!(
                "discarded version {} after {} failed installation attempts",
                pending.version,
                pending.attempts
            ));
        }

        let package = fs::read(package_path(&pending)?).context("could not read staged update")?;
        if let Err(error) = verify_staged_package(public_key, &pending.signature, &package) {
            clear_pending_update()?;
            return Err(error).context("discarded staged update with an invalid signature");
        }
        let update = staged_update(public_key, &pending)?;

        #[cfg(windows)]
        {
            // NSIS exits this process immediately after spawning its installer, so the pending
            // marker must be gone before calling install or every future launch would rerun it.
            remove_if_exists(&state_path_in(&cache_directory()?))?;
            return update
                .install(package)
                .context("could not install staged update");
        }

        #[cfg(not(windows))]
        {
            if let Err(error) = update.install(package) {
                pending.attempts += 1;
                write_pending_update(&pending)?;
                return Err(error).context("could not install staged update");
            }

            clear_pending_update()?;
            Ok(())
        }
    }

    fn updater(public_key: &'static str) -> Result<cargo_packager_updater::Updater> {
        let endpoint = Url::parse(UPDATE_ENDPOINT).expect("the built-in updater endpoint is valid");
        let config = Config {
            endpoints: vec![endpoint],
            pubkey: public_key.into(),
            windows: None,
        };
        UpdaterBuilder::new(
            Version::parse(env!("CARGO_PKG_VERSION")).expect("package version is valid semver"),
            config,
        )
        .timeout(Duration::from_secs(20))
        .build()
        .context("could not configure updater")
    }

    fn stage_update(update: &Update, package: &[u8]) -> Result<()> {
        let directory = cache_directory()?;
        stage_update_in(&directory, update, package)
    }

    fn stage_update_in(directory: &Path, update: &Update, package: &[u8]) -> Result<()> {
        fs::create_dir_all(directory).context("could not create update cache directory")?;
        let pending = PendingUpdate {
            version: update.version.clone(),
            attempts: 0,
            package_name: package_name(&update.version),
            signature: update.signature.clone(),
        };
        let previous = read_pending_update_in(directory)?;
        atomic_write(&package_path_in(directory, &pending)?, package)?;
        write_pending_update_in(directory, &pending)?;
        if let Some(previous) = previous
            && previous.package_name != pending.package_name
            && let Err(error) = remove_if_exists(&package_path_in(directory, &previous)?)
        {
            tracing::warn!(error = %error, "could not remove superseded staged application update");
        }
        Ok(())
    }

    fn has_staged_update(version: &str) -> Result<bool> {
        has_staged_update_in(&cache_directory()?, version)
    }

    fn has_staged_update_in(directory: &Path, version: &str) -> Result<bool> {
        let Some(pending) = read_pending_update_in(directory)? else {
            return Ok(false);
        };
        Ok(pending.version == version && package_path_in(directory, &pending)?.is_file())
    }

    fn staged_update(public_key: &'static str, pending: &PendingUpdate) -> Result<Update> {
        Ok(Update {
            config: Config {
                endpoints: Vec::new(),
                pubkey: public_key.into(),
                windows: None,
            },
            body: None,
            current_version: env!("CARGO_PKG_VERSION").into(),
            version: pending.version.clone(),
            date: None,
            target: String::new(),
            extract_path: trusted_extract_path()?,
            download_url: Url::parse("https://invalid.example/staged-update")
                .expect("the staged update placeholder URL is valid"),
            signature: pending.signature.clone(),
            timeout: None,
            headers: Default::default(),
            format: trusted_update_format(),
        })
    }

    fn read_pending_update() -> Result<Option<PendingUpdate>> {
        read_pending_update_in(&cache_directory()?)
    }

    fn read_pending_update_in(directory: &Path) -> Result<Option<PendingUpdate>> {
        let state_path = state_path_in(directory);
        match fs::read_to_string(&state_path) {
            Ok(state) => match serde_yaml::from_str(&state) {
                Ok(pending) => {
                    if package_path_in(directory, &pending).is_ok() {
                        Ok(Some(pending))
                    } else {
                        remove_if_exists(&state_path)?;
                        tracing::warn!("discarded staged update with an invalid package path");
                        Ok(None)
                    }
                }
                Err(error) => {
                    remove_if_exists(&state_path)?;
                    tracing::warn!(error = %error, "discarded malformed staged update metadata");
                    Ok(None)
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("could not read staged update metadata"),
        }
    }

    fn write_pending_update(pending: &PendingUpdate) -> Result<()> {
        write_pending_update_in(&cache_directory()?, pending)
    }

    fn write_pending_update_in(directory: &Path, pending: &PendingUpdate) -> Result<()> {
        let encoded =
            serde_yaml::to_string(pending).context("could not encode staged update metadata")?;
        atomic_write(&state_path_in(directory), encoded.as_bytes())
    }

    fn clear_pending_update() -> Result<()> {
        clear_pending_update_in(&cache_directory()?)
    }

    fn clear_pending_update_in(directory: &Path) -> Result<()> {
        let pending = read_pending_update_in(directory)?;
        remove_if_exists(&state_path_in(directory))?;
        if let Some(pending) = pending {
            remove_if_exists(&package_path_in(directory, &pending)?)?;
        }
        Ok(())
    }

    fn remove_if_exists(path: &Path) -> Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("could not remove {}", path.display()))
            }
        }
    }

    fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<()> {
        static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

        let parent = destination
            .parent()
            .expect("update cache files have a parent directory");
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
        let temporary = destination.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&temporary, bytes)
            .with_context(|| format!("could not write {}", temporary.display()))?;
        #[cfg(windows)]
        if destination.exists() {
            fs::remove_file(destination).with_context(|| {
                format!(
                    "could not replace existing staged file {}",
                    destination.display()
                )
            })?;
        }
        let result = fs::rename(&temporary, destination).with_context(|| {
            format!(
                "could not move verified update into place from {} to {}",
                temporary.display(),
                destination.display()
            )
        });
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn cache_directory() -> Result<PathBuf> {
        let base = if cfg!(target_os = "windows") {
            env::var_os("LOCALAPPDATA").ok_or_else(|| anyhow!("LOCALAPPDATA is not set"))?
        } else if cfg!(target_os = "macos") {
            let home = env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
            return Ok(PathBuf::from(home)
                .join("Library")
                .join("Caches")
                .join("Styrhous")
                .join("updater"));
        } else if let Some(cache_home) = env::var_os("XDG_CACHE_HOME") {
            cache_home
        } else {
            let home = env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
            return Ok(PathBuf::from(home)
                .join(".cache")
                .join("styrhous")
                .join("updater"));
        };
        Ok(PathBuf::from(base).join("Styrhous").join("updater"))
    }

    fn package_path(pending: &PendingUpdate) -> Result<PathBuf> {
        package_path_in(&cache_directory()?, pending)
    }

    fn package_path_in(directory: &Path, pending: &PendingUpdate) -> Result<PathBuf> {
        if Path::new(&pending.package_name).components().count() != 1 {
            return Err(anyhow!("staged update package name is invalid"));
        }
        Ok(directory.join(&pending.package_name))
    }

    fn state_path_in(directory: &Path) -> PathBuf {
        directory.join(UPDATE_STATE_NAME)
    }

    fn verify_staged_package(public_key: &str, signature: &str, package: &[u8]) -> Result<()> {
        let encoded_public_key = base64::engine::general_purpose::STANDARD
            .decode(public_key)
            .context("could not decode updater public key")?;
        let public_key =
            std::str::from_utf8(&encoded_public_key).context("updater public key is not UTF-8")?;
        let public_key =
            PublicKey::decode(public_key).context("could not parse updater public key")?;
        let encoded_signature = base64::engine::general_purpose::STANDARD
            .decode(signature)
            .context("could not decode updater signature")?;
        let signature =
            std::str::from_utf8(&encoded_signature).context("updater signature is not UTF-8")?;
        let signature =
            Signature::decode(signature).context("could not parse updater signature")?;
        public_key
            .verify(package, &signature, true)
            .context("staged package signature did not verify")?;
        Ok(())
    }

    fn trusted_extract_path() -> Result<PathBuf> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            if let Some(appimage) = env::var_os("APPIMAGE") {
                return Ok(PathBuf::from(appimage));
            }
        }

        let executable = env::current_exe().context("could not locate current executable")?;
        #[cfg(target_os = "macos")]
        return macos_application_bundle_path(&executable);
        #[cfg(not(target_os = "macos"))]
        Ok(executable)
    }

    #[cfg(any(target_os = "macos", test))]
    fn macos_application_bundle_path(executable: &Path) -> Result<PathBuf> {
        if executable.display().to_string().contains("Contents/MacOS") {
            return executable
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .map(PathBuf::from)
                .ok_or_else(|| anyhow!("could not locate the application bundle"));
        }
        Ok(executable.into())
    }

    fn trusted_update_format() -> UpdateFormat {
        #[cfg(windows)]
        {
            return UpdateFormat::Nsis;
        }
        #[cfg(target_os = "macos")]
        {
            return UpdateFormat::App;
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        UpdateFormat::AppImage
    }

    fn package_name(version: &str) -> String {
        let safe_version: String = version
            .chars()
            .map(|character| match character {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '.' | '-' | '_' => character,
                _ => '_',
            })
            .collect();
        format!("pending-update-{safe_version}.bin")
    }

    #[cfg(test)]
    mod tests {
        use super::{
            Config, Update, UpdateFormat, atomic_write, clear_pending_update_in,
            has_staged_update_in, macos_application_bundle_path, package_path_in,
            read_pending_update_in, stage_update_in, staged_update, verify_staged_package,
        };
        use base64::Engine;
        use cargo_packager_updater::url::Url;
        use std::fs;
        use std::path::{Path, PathBuf};

        fn update(version: &str) -> Update {
            Update {
                config: Config {
                    endpoints: Vec::new(),
                    pubkey: String::new(),
                    windows: None,
                },
                body: None,
                current_version: "0.0.1-alpha.1".into(),
                version: version.into(),
                date: None,
                target: "linux-x86_64".into(),
                extract_path: PathBuf::from("Styrhous.AppImage"),
                download_url: Url::parse("https://invalid.example/update")
                    .expect("fixture URL should be valid"),
                signature: "fixture signature".into(),
                timeout: None,
                headers: Default::default(),
                format: UpdateFormat::AppImage,
            }
        }

        #[test]
        fn atomic_write_replaces_existing_staged_metadata() {
            let directory = std::env::temp_dir().join(format!(
                "styrhous-updater-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("the system clock is after the Unix epoch")
                    .as_nanos()
            ));
            fs::create_dir_all(&directory).expect("temporary test directory should be created");
            let destination = directory.join("pending-update.yaml");

            atomic_write(&destination, b"first").expect("initial staged metadata should write");
            atomic_write(&destination, b"second").expect("staged metadata should be replaceable");

            assert_eq!(
                fs::read(&destination).expect("staged metadata should be readable"),
                b"second"
            );
            fs::remove_dir_all(directory).expect("temporary test directory should be removed");
        }

        #[test]
        fn staged_update_persists_payload_and_replaces_a_superseded_version() {
            let directory = tempfile::tempdir().expect("temporary cache directory should exist");
            let first = update("0.0.1-alpha.2");
            stage_update_in(directory.path(), &first, b"first payload")
                .expect("first update should stage successfully");

            let first_pending = read_pending_update_in(directory.path())
                .expect("staged metadata should be readable")
                .expect("first update should have staged metadata");
            assert_eq!(first_pending.version, "0.0.1-alpha.2");
            assert_eq!(
                fs::read(package_path_in(directory.path(), &first_pending).expect("package path"))
                    .expect("first payload should be readable"),
                b"first payload"
            );
            assert!(
                has_staged_update_in(directory.path(), "0.0.1-alpha.2")
                    .expect("first staged version should be checked")
            );

            let second = update("0.0.1-alpha.3");
            stage_update_in(directory.path(), &second, b"second payload")
                .expect("replacement update should stage successfully");
            assert!(
                !package_path_in(directory.path(), &first_pending)
                    .expect("first package path")
                    .exists(),
                "the superseded payload should be removed"
            );

            let second_pending = read_pending_update_in(directory.path())
                .expect("replacement staged metadata should be readable")
                .expect("replacement update should have staged metadata");
            let reconstructed = staged_update("fixture public key", &second_pending)
                .expect("trusted staged update should reconstruct");
            assert_eq!(reconstructed.version, "0.0.1-alpha.3");
            assert_eq!(reconstructed.config.pubkey, "fixture public key");
            assert!(
                has_staged_update_in(directory.path(), "0.0.1-alpha.3")
                    .expect("replacement staged version should be checked")
            );

            clear_pending_update_in(directory.path())
                .expect("staged update cleanup should succeed");
            assert!(
                read_pending_update_in(directory.path())
                    .expect("staged metadata should be readable after cleanup")
                    .is_none()
            );
            assert!(
                !package_path_in(directory.path(), &second_pending)
                    .expect("second package path")
                    .exists(),
                "cleanup should remove the staged payload"
            );
        }

        #[test]
        fn staged_package_signature_must_match_the_payload() {
            let public_key = concat!(
                "untrusted comment: minisign public key\n",
                "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3"
            );
            let signature = concat!(
                "untrusted comment: signature from minisign secret key\n",
                "RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/",
                "z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n",
                "trusted comment: timestamp:1633700835\tfile:test\tprehashed\n",
                "wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ=="
            );
            let encoded_public_key = base64::engine::general_purpose::STANDARD.encode(public_key);
            let encoded_signature = base64::engine::general_purpose::STANDARD.encode(signature);

            verify_staged_package(&encoded_public_key, &encoded_signature, b"test")
                .expect("the fixture signature should authenticate the fixture payload");
            assert!(
                verify_staged_package(&encoded_public_key, &encoded_signature, b"tampered")
                    .is_err()
            );
        }

        #[test]
        fn macos_staged_updates_target_the_application_bundle() {
            assert_eq!(
                macos_application_bundle_path(Path::new(
                    "/Applications/Styrhous.app/Contents/MacOS/styrhous"
                ))
                .expect("a macOS bundle executable should have an application path"),
                PathBuf::from("/Applications/Styrhous.app")
            );
        }
    }
}

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
mod tests {
    use super::{UpdateStatus, updates_disabled_by_value};

    #[test]
    fn recognizes_package_manager_update_opt_out_values() {
        for value in ["1", "true", "TRUE", " yes "] {
            assert!(
                updates_disabled_by_value(value),
                "{value:?} should disable updates"
            );
        }
        for value in ["", "0", "false", "no", "enabled"] {
            assert!(
                !updates_disabled_by_value(value),
                "{value:?} should not disable updates"
            );
        }
    }

    #[test]
    fn staged_update_summary_mentions_the_next_launch() {
        assert!(
            UpdateStatus::Staged {
                version: "1.2.3".into()
            }
            .summary()
            .contains("next launch")
        );
    }

    #[test]
    fn only_active_update_states_show_a_badge() {
        assert!(UpdateStatus::Checking.shows_badge());
        assert!(
            UpdateStatus::Staged {
                version: "1.2.3".into()
            }
            .shows_badge()
        );
        assert!(!UpdateStatus::LocalBuild.shows_badge());
        assert!(!UpdateStatus::ExternallyManaged.shows_badge());
    }

    #[cfg(feature = "self-updater")]
    #[test]
    fn updater_activation_matches_the_compiled_build_kind() {
        let (status, public_key) = super::updater_activation();
        if cfg!(styrhous_package_managed_build) {
            assert_eq!(status, UpdateStatus::ExternallyManaged);
            assert!(public_key.is_none());
        } else if cfg!(styrhous_release_build) {
            assert_eq!(status, UpdateStatus::Checking);
            assert!(public_key.is_some());
        } else {
            assert_eq!(status, UpdateStatus::LocalBuild);
            assert!(public_key.is_none());
        }
    }
}
