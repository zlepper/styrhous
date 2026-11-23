//! The optional user theme file.
//!
//! `theme.toml` in the application's data directory
//! ([`eframe::storage_dir`]; `~/.local/share/styrhous/theme.toml` on Linux)
//! extends a built-in theme (see `docs/theming.md`). It is loaded at startup
//! and polled about once a second, so edits apply live: a valid change
//! switches the theme, an invalid one keeps the current theme and reports
//! why, and deleting the file returns to the built-in theme.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use components::design::theme::{BuiltinTheme, Theme};

/// The theme file's name inside the data directory.
pub(crate) const THEME_FILE_NAME: &str = "theme.toml";

/// How often the file is checked for changes.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// What a check of the theme file found that differs from the last check.
#[derive(Debug)]
pub(crate) enum ThemeFileChange {
    /// The file was created or changed and is valid: switch to this theme.
    Loaded(Arc<Theme>),
    /// The file was deleted: switch back to this built-in theme.
    Removed(Arc<Theme>),
    /// The file was created or changed but cannot be used. Keep the current
    /// theme and show this message.
    Failed(String),
}

/// What the file looked like when it was last checked. A change of either
/// field means the contents may have changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileStamp {
    Missing,
    Present {
        modified: Option<SystemTime>,
        len: u64,
    },
}

impl FileStamp {
    fn of(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(metadata) => Self::Present {
                modified: metadata.modified().ok(),
                len: metadata.len(),
            },
            Err(_) => Self::Missing,
        }
    }
}

/// Polls one theme file for changes.
#[derive(Debug)]
pub(crate) struct ThemeFileWatcher {
    path: PathBuf,
    /// The file at the last check. Starts as missing, which matches the
    /// built-in theme the application starts with.
    last: FileStamp,
    /// When the file is next due to be checked; `None` checks right away.
    next_check: Option<Instant>,
}

impl ThemeFileWatcher {
    /// Watch the theme file at `path`. The first [`Self::poll`] checks it
    /// immediately.
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            last: FileStamp::Missing,
            next_check: None,
        }
    }

    /// Watch `theme.toml` in the data directory of the application `app_id`,
    /// if the platform has one.
    pub(crate) fn in_data_dir(app_id: &str) -> Option<Self> {
        eframe::storage_dir(app_id).map(|dir| Self::new(dir.join(THEME_FILE_NAME)))
    }

    /// The watched file.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Check the file if a check is due at `now`, and return how it changed
    /// since the previous check.
    pub(crate) fn poll(&mut self, now: Instant) -> Option<ThemeFileChange> {
        if self.next_check.is_some_and(|next_check| now < next_check) {
            return None;
        }
        self.next_check = Some(now + POLL_INTERVAL);
        let stamp = FileStamp::of(&self.path);
        if stamp == self.last {
            return None;
        }
        self.last = stamp;
        Some(match stamp {
            FileStamp::Missing => {
                ThemeFileChange::Removed(Arc::new(Theme::builtin(BuiltinTheme::default())))
            }
            FileStamp::Present { .. } => self.load(),
        })
    }

    /// How long after `now` the next check is due.
    pub(crate) fn time_until_next_check(&self, now: Instant) -> Duration {
        self.next_check.map_or(Duration::ZERO, |next_check| {
            next_check.saturating_duration_since(now)
        })
    }

    fn load(&self) -> ThemeFileChange {
        let source = match std::fs::read_to_string(&self.path) {
            Ok(source) => source,
            Err(error) => {
                return ThemeFileChange::Failed(format!(
                    "couldn't read {}: {error}",
                    self.path.display()
                ));
            }
        };
        match Theme::from_file_str(&source) {
            Ok(theme) => ThemeFileChange::Loaded(Arc::new(theme)),
            Err(error) => ThemeFileChange::Failed(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use components::design::theme::EXAMPLE_THEME_FILE;
    use std::fs::File;

    /// A theme file in its own temporary directory.
    struct Fixture {
        _dir: tempfile::TempDir,
        path: PathBuf,
        /// The modification time the next write gets.
        modified: SystemTime,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a temporary directory");
            let path = dir.path().join(THEME_FILE_NAME);
            Self {
                _dir: dir,
                path,
                modified: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            }
        }

        /// Write `contents` with a modification time one second after the
        /// previous write, independent of the file system's timestamp
        /// resolution.
        fn write(&mut self, contents: &str) {
            std::fs::write(&self.path, contents).expect("the theme file is writable");
            self.modified += Duration::from_secs(1);
            File::options()
                .write(true)
                .open(&self.path)
                .and_then(|file| file.set_modified(self.modified))
                .expect("the modification time can be set");
        }
    }

    fn loaded_name(change: Option<ThemeFileChange>) -> String {
        match change {
            Some(ThemeFileChange::Loaded(theme)) => theme.name().to_owned(),
            other => panic!("expected a loaded theme, got {other:?}"),
        }
    }

    const CUSTOM: &str = r##"
        name = "Custom"
        extends = "harbour-dark"
        [palette]
        sand-950 = "#101010"
    "##;

    #[test]
    fn loads_the_file_on_the_first_poll() {
        let mut fixture = Fixture::new();
        fixture.write(EXAMPLE_THEME_FILE);
        let mut watcher = ThemeFileWatcher::new(&fixture.path);
        let now = Instant::now();

        assert_eq!(
            loaded_name(watcher.poll(now)),
            "Harbour Dark · high contrast"
        );
        assert_eq!(watcher.time_until_next_check(now), POLL_INTERVAL);
        assert!(
            watcher.poll(now + POLL_INTERVAL).is_none(),
            "unchanged file"
        );
    }

    #[test]
    fn a_missing_file_keeps_the_builtin_theme() {
        let fixture = Fixture::new();
        let mut watcher = ThemeFileWatcher::new(&fixture.path);
        assert!(watcher.poll(Instant::now()).is_none());
    }

    #[test]
    fn a_change_is_picked_up_at_the_next_poll_after_its_modification() {
        let mut fixture = Fixture::new();
        fixture.write(EXAMPLE_THEME_FILE);
        let mut watcher = ThemeFileWatcher::new(&fixture.path);
        let start = Instant::now();
        loaded_name(watcher.poll(start));

        fixture.write(CUSTOM);
        assert!(
            watcher.poll(start + POLL_INTERVAL / 2).is_none(),
            "the file is only checked once per interval"
        );
        let change = watcher.poll(start + POLL_INTERVAL);
        let Some(ThemeFileChange::Loaded(theme)) = change else {
            panic!("expected the changed theme, got {change:?}");
        };
        assert_eq!(theme.name(), "Custom");
        assert_eq!(
            theme.surface.canvas,
            components::design::palette::parse_hex("#101010").unwrap()
        );
    }

    #[test]
    fn an_invalid_change_reports_the_error_and_keeps_the_current_theme() {
        let mut fixture = Fixture::new();
        fixture.write(CUSTOM);
        let mut watcher = ThemeFileWatcher::new(&fixture.path);
        let start = Instant::now();
        loaded_name(watcher.poll(start));

        let invalid = "name = \"Typo\"\n[roles]\ntext-mutted = \"sand-200\"\n";
        fixture.write(invalid);
        let change = watcher.poll(start + POLL_INTERVAL);
        let Some(ThemeFileChange::Failed(message)) = change else {
            panic!("expected an error, got {change:?}");
        };
        let expected = Theme::from_file_str(invalid).unwrap_err().to_string();
        assert_eq!(message, expected);
        assert!(message.contains("did you mean `text-muted`"), "{message}");
        // The error is reported once, not on every poll.
        assert!(watcher.poll(start + 2 * POLL_INTERVAL).is_none());

        // Fixing the file loads it.
        fixture.write(CUSTOM);
        assert_eq!(
            loaded_name(watcher.poll(start + 3 * POLL_INTERVAL)),
            "Custom"
        );
    }

    #[test]
    fn deleting_the_file_reverts_to_the_builtin_theme() {
        let mut fixture = Fixture::new();
        fixture.write(CUSTOM);
        let mut watcher = ThemeFileWatcher::new(&fixture.path);
        let start = Instant::now();
        loaded_name(watcher.poll(start));

        std::fs::remove_file(&fixture.path).expect("the theme file can be deleted");
        let change = watcher.poll(start + POLL_INTERVAL);
        let Some(ThemeFileChange::Removed(theme)) = change else {
            panic!("expected the built-in theme, got {change:?}");
        };
        assert_eq!(theme.name(), BuiltinTheme::default().display_name());
        assert!(watcher.poll(start + 2 * POLL_INTERVAL).is_none());
    }
}
