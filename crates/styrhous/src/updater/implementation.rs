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
        Err(error) => Err(error).with_context(|| format!("could not remove {}", path.display())),
    }
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<()> {
    static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let parent = destination
        .parent()
        .expect("update cache files have a parent directory");
    fs::create_dir_all(parent).with_context(|| format!("could not create {}", parent.display()))?;
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
    let public_key = PublicKey::decode(public_key).context("could not parse updater public key")?;
    let encoded_signature = base64::engine::general_purpose::STANDARD
        .decode(signature)
        .context("could not decode updater signature")?;
    let signature =
        std::str::from_utf8(&encoded_signature).context("updater signature is not UTF-8")?;
    let signature = Signature::decode(signature).context("could not parse updater signature")?;
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
mod tests;
