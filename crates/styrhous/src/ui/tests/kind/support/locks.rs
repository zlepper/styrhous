//! Cross-process locks shared by Kind test processes and `scripts/ensure-kind-cluster.sh`.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

const KIND_DISCOVERY_LOCK_FILE: &str = "styrhous-kind-api-discovery.lock";
const NAMESPACE_LEASE_DIR: &str = "styrhous-kind-namespaces";

pub(in crate::ui::tests::kind) struct KindDiscoveryLock {
    _file: File,
}

impl KindDiscoveryLock {
    pub(in crate::ui::tests::kind) fn acquire() -> Self {
        Self::acquire_at(&kind_lock_dir().join(KIND_DISCOVERY_LOCK_FILE))
    }

    fn acquire_at(path: &Path) -> Self {
        let file = open_lock_file(path, "Kind API discovery lock");
        file.lock().unwrap_or_else(|error| {
            panic!(
                "Failed to acquire Kind API discovery lock {}: {error}",
                path.display()
            )
        });
        Self { _file: file }
    }
}

/// Directory for the locks shared by every Kind test process and `scripts/ensure-kind-cluster.sh`
/// (`kind_lock_dir` in `scripts/lib/kind-namespace-leases.sh`; a test checks that both agree).
/// It derives from the user id rather than `XDG_RUNTIME_DIR` or `TMPDIR`, which differ between
/// shells, services and `nix develop` sessions: runs that resolved different directories would
/// not see each other's leases and would delete each other's namespaces.
#[cfg(unix)]
pub(super) fn kind_lock_dir() -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    // `/proc/self` is owned by the process's effective user id, which is what `id -u` prints.
    let uid = std::fs::metadata("/proc/self")
        .expect("Kind tests run on Linux, which has /proc/self")
        .uid();
    let runtime_dir = PathBuf::from(format!("/run/user/{uid}"));
    if runtime_dir.is_dir() {
        runtime_dir
    } else {
        PathBuf::from(format!("/tmp/styrhous-kind-{uid}"))
    }
}

/// Kind tests only run on Linux; other platforms just need this to compile.
#[cfg(not(unix))]
pub(super) fn kind_lock_dir() -> PathBuf {
    std::env::temp_dir().join("styrhous-kind")
}

fn open_lock_file(path: &Path, what: &str) -> File {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|error| {
            panic!(
                "Failed to create the directory of {what} {}: {error}",
                path.display()
            )
        });
    }
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .unwrap_or_else(|error| panic!("Failed to open {what} {}: {error}", path.display()))
}

/// Marks a test namespace as owned by a running test.
///
/// The test holds an exclusive `flock` on `<kind lock dir>/styrhous-kind-namespaces/<namespace>.lock`
/// from before it creates the namespace until after it has deleted it.
/// `scripts/ensure-kind-cluster.sh` removes only leftover namespaces whose lease it can take, so
/// concurrent test runs leave each other's namespaces alone. The kernel releases the lock when the
/// owning process exits, so namespaces of crashed or interrupted tests are still cleaned up by the
/// next run.
pub(super) struct NamespaceLease {
    pub(super) path: PathBuf,
    _file: File,
}

impl NamespaceLease {
    pub(super) fn acquire(namespace: &str) -> Self {
        Self::acquire_in(&kind_lock_dir().join(NAMESPACE_LEASE_DIR), namespace)
    }

    fn acquire_in(dir: &Path, namespace: &str) -> Self {
        let path = dir.join(format!("{namespace}.lock"));
        let file = open_lock_file(&path, "namespace lease");
        file.try_lock().unwrap_or_else(|error| {
            panic!(
                "Failed to take namespace lease {} (namespace names are unique, so nothing \
                 else should hold it): {error}",
                path.display()
            )
        });
        Self { path, _file: file }
    }
}

impl Drop for NamespaceLease {
    fn drop(&mut self) {
        // Unlink while still holding the lock; the lock itself is released when `_file` closes.
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const LEASE_SCRIPT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/lib/kind-namespace-leases.sh"
    );

    /// Whether `flock --nonblock`, as `scripts/ensure-kind-cluster.sh` uses it, can take the lock.
    fn flock_can_take(path: &Path) -> bool {
        Command::new("flock")
            .arg("--nonblock")
            .arg(path)
            .arg("true")
            .status()
            .expect("flock (util-linux) should run")
            .success()
    }

    /// Runs `script` in bash after sourcing the lease helpers, with `args` as `$@`.
    fn run_lease_script(script: &str, args: &[&std::ffi::OsStr]) -> String {
        let output = Command::new("bash")
            .arg("-c")
            .arg(format!("set -euo pipefail; source \"$0\"; {script}"))
            .arg(LEASE_SCRIPT)
            .args(args)
            .output()
            .expect("bash should run");
        assert!(
            output.status.success(),
            "the lease script failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("the lease script prints UTF-8")
    }

    #[test]
    fn kind_discovery_lock_is_exclusive_and_released_with_its_guard() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("discovery.lock");
        let guard = KindDiscoveryLock::acquire_at(&path);
        let competing_file = File::open(&path).expect("the competing lock file should open");

        assert!(
            competing_file.try_lock().is_err(),
            "a second Kind discovery should not acquire the lock while the guard is alive"
        );

        drop(guard);
        competing_file
            .try_lock()
            .expect("dropping the guard should release the discovery lock");
    }

    #[test]
    fn ensure_kind_cluster_cannot_take_a_namespace_lease_until_its_test_releases_it() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let namespace = "styrhous-it-lease-example";
        let path = dir.path().join(format!("{namespace}.lock"));

        let lease = NamespaceLease::acquire_in(dir.path(), namespace);
        assert!(
            !flock_can_take(&path),
            "cleanup must not take the lease of a namespace whose test is still running"
        );
        let leftover_handle =
            File::open(&path).expect("the lease file should exist while the lease is held");

        drop(lease);
        assert!(!path.exists(), "releasing the lease should remove its file");
        leftover_handle
            .try_lock()
            .expect("releasing the lease should release its lock");
    }

    #[test]
    fn ensure_kind_cluster_selects_only_namespaces_without_a_running_test() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let _running = NamespaceLease::acquire_in(dir.path(), "styrhous-it-running");

        let output = run_lease_script(
            r#"lease_dir="$1"; shift
take_leftover_namespace_leases "${lease_dir}" "$@"
printf 'leftover %s\n' "${test_namespaces[@]}"
printf 'lease file %s\n' "${lease_files[@]}"
# The taken leases stay locked in this shell until it exits.
for lease_file in "${lease_files[@]}"; do
    if flock --nonblock "${lease_file}" true; then echo "unlocked ${lease_file}"; fi
done"#,
            &[
                dir.path().as_os_str(),
                "styrhous-it-running".as_ref(),
                "styrhous-it-finished".as_ref(),
            ],
        );

        let finished_lease = dir.path().join("styrhous-it-finished.lock");
        assert_eq!(
            output,
            format!(
                "Keeping test namespace 'styrhous-it-running': a running test or another cleanup owns it.\n\
                 leftover styrhous-it-finished\n\
                 lease file {}\n",
                finished_lease.display()
            )
        );
    }

    #[test]
    fn ensure_kind_cluster_resolves_the_same_lock_directory_as_the_tests() {
        let output = run_lease_script("kind_lock_dir", &[]);
        assert_eq!(output.trim_end(), kind_lock_dir().to_str().unwrap());
    }
}
