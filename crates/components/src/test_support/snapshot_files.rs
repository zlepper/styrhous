use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SnapshotMode {
    Test,
    UpdateFailing,
    UpdateAll,
}

impl SnapshotMode {
    pub(super) fn from_env() -> Self {
        match std::env::var("UPDATE_SNAPSHOTS") {
            Err(_) => Self::Test,
            Ok(value) if matches!(value.as_str(), "false" | "0" | "no" | "off") => Self::Test,
            Ok(value) if matches!(value.as_str(), "true" | "1" | "yes" | "on") => {
                Self::UpdateFailing
            }
            Ok(value) if value == "force" => Self::UpdateAll,
            Ok(value) => panic!("Unsupported value for UPDATE_SNAPSHOTS: {value:?}"),
        }
    }

    pub(super) fn is_update(self) -> bool {
        self != Self::Test
    }
}

pub(super) struct SnapshotPaths {
    pub(super) snapshot_path: PathBuf,
    pub(super) new_path: PathBuf,
    pub(super) old_path: PathBuf,
}

pub(super) fn snapshot_paths(output_path: &Path, name: &str) -> SnapshotPaths {
    SnapshotPaths {
        snapshot_path: output_path.join(format!("{name}.accessibility.txt")),
        new_path: output_path.join(format!("{name}.accessibility.new.txt")),
        old_path: output_path.join(format!("{name}.accessibility.old.txt")),
    }
}

pub(super) fn create_parent_directory(path: &Path) -> Result<(), AccessibilitySnapshotError> {
    let parent = path.parent().expect("snapshot path always has a parent");
    std::fs::create_dir_all(parent).map_err(|source| AccessibilitySnapshotError::Write {
        path: parent.to_owned(),
        source,
    })
}

pub(super) fn write_file(path: &Path, contents: &str) -> Result<(), AccessibilitySnapshotError> {
    std::fs::write(path, contents).map_err(|source| AccessibilitySnapshotError::Write {
        path: path.to_owned(),
        source,
    })
}

pub(super) fn update_snapshot(
    paths: &SnapshotPaths,
    current: &str,
) -> Result<(), AccessibilitySnapshotError> {
    remove_diagnostic_files(paths)?;
    match std::fs::rename(&paths.snapshot_path, &paths.old_path) {
        Ok(()) => {}
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(AccessibilitySnapshotError::Write {
                path: paths.old_path.clone(),
                source,
            });
        }
    }
    write_file(&paths.snapshot_path, current)
}

pub(super) fn remove_diagnostic_files(
    paths: &SnapshotPaths,
) -> Result<(), AccessibilitySnapshotError> {
    remove_file_if_present(&paths.new_path)?;
    remove_file_if_present(&paths.old_path)
}

pub(super) fn remove_file_if_present(path: &Path) -> Result<(), AccessibilitySnapshotError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(AccessibilitySnapshotError::Write {
            path: path.to_owned(),
            source,
        }),
    }
}
