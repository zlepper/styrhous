//! The optional user theme file.
//!
//! `theme.toml` in the application's data directory
//! ([`eframe::storage_dir`]; `~/.local/share/styrhous/theme.toml` on Linux)
//! extends a built-in theme (see `docs/theming.md`). It is read once at
//! startup: a valid file switches the theme, an invalid one keeps the
//! built-in theme and reports why. Edits apply at the next start.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use components::design::theme::Theme;

/// The theme file's name inside the data directory.
const THEME_FILE_NAME: &str = "theme.toml";

/// `theme.toml` in the data directory of the application `app_id`, if the
/// platform has one.
pub(crate) fn path_in_data_dir(app_id: &str) -> Option<PathBuf> {
    eframe::storage_dir(app_id).map(|dir| dir.join(THEME_FILE_NAME))
}

/// The theme in the file at `path`: `None` when there is no file, otherwise
/// the theme or why the file cannot be used.
pub(crate) fn load(path: &Path) -> Option<Result<Arc<Theme>, String>> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => return Some(Err(format!("couldn't read {}: {error}", path.display()))),
    };
    Some(
        Theme::from_file_str(&source)
            .map(Arc::new)
            .map_err(|error| error.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use components::design::theme::EXAMPLE_THEME_FILE;

    /// Write `contents` as a theme file in a new temporary directory.
    fn theme_file(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join(THEME_FILE_NAME);
        std::fs::write(&path, contents).expect("the theme file is writable");
        (dir, path)
    }

    #[test]
    fn a_valid_file_loads_its_theme() {
        let (_dir, path) = theme_file(EXAMPLE_THEME_FILE);
        let theme = load(&path).expect("a file").expect("a valid theme");
        assert_eq!(theme.name(), "Harbour Dark · high contrast");
    }

    #[test]
    fn a_missing_file_is_no_theme() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        assert!(load(&dir.path().join(THEME_FILE_NAME)).is_none());
    }

    #[test]
    fn an_invalid_file_reports_the_parse_error() {
        let invalid = "name = \"Typo\"\n[roles]\ntext-mutted = \"sand-200\"\n";
        let (_dir, path) = theme_file(invalid);
        let message = load(&path).expect("a file").expect_err("an invalid theme");
        assert_eq!(
            message,
            Theme::from_file_str(invalid).unwrap_err().to_string()
        );
        assert!(message.contains("did you mean `text-muted`"), "{message}");
    }

    #[test]
    fn an_unreadable_path_reports_why() {
        // A directory where the file should be.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let message = load(dir.path())
            .expect("something there")
            .expect_err("unreadable");
        assert!(message.starts_with("couldn't read "), "{message}");
    }
}
