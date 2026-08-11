//! Shared setup for component UI tests.
//!
//! Keeping this setup in the component crate lets both unit tests and the
//! public-API showcase snapshots render with the same deterministic theme and
//! image loaders.

use std::fmt::{Display, Write as _};
use std::io;
use std::path::{Path, PathBuf};

use egui::Vec2;
use egui::accesskit::Role;
use egui_kittest::kittest::NodeT;
use egui_kittest::{Harness, Node};

/// The deterministic viewport used by all egui tests and snapshots.
pub const EGUI_TEST_SIZE: Vec2 = Vec2::new(1536.0, 1024.0);

/// Configure a test harness for component tests and snapshots.
pub fn setup_egui<State>(harness: &mut Harness<'_, State>) {
    crate::apply_light_theme(&harness.ctx);
    egui_extras::install_image_loaders(&harness.ctx);
    harness.set_size(EGUI_TEST_SIZE);
}

/// Options controlling accessibility-tree text snapshots.
#[derive(Clone, Debug)]
pub struct AccessibilityTreeOptions {
    /// Include unlabeled generic containers so the output preserves the complete AccessKit tree.
    pub include_structural_nodes: bool,
    /// Directory containing the committed text fixtures and diagnostic dumps.
    pub output_path: PathBuf,
}

impl Default for AccessibilityTreeOptions {
    fn default() -> Self {
        Self {
            include_structural_nodes: true,
            output_path: PathBuf::from("tests/snapshots"),
        }
    }
}

impl AccessibilityTreeOptions {
    /// Create options using the project's normal snapshot directory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Include or omit unlabeled `GenericContainer` nodes.
    pub fn include_structural_nodes(mut self, include_structural_nodes: bool) -> Self {
        self.include_structural_nodes = include_structural_nodes;
        self
    }

    /// Write fixtures and dumps under a custom directory.
    pub fn output_path(mut self, output_path: impl Into<PathBuf>) -> Self {
        self.output_path = output_path.into();
        self
    }
}

/// A text-snapshot failure for an accessibility tree.
#[derive(Debug)]
pub enum AccessibilitySnapshotError {
    /// The committed fixture could not be read.
    Read { path: PathBuf, source: io::Error },
    /// The current tree differs from the committed fixture.
    Mismatch {
        snapshot_path: PathBuf,
        new_path: PathBuf,
    },
    /// A fixture or diagnostic artifact could not be written.
    Write { path: PathBuf, source: io::Error },
}

impl Display for AccessibilitySnapshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => write!(
                formatter,
                "Could not read accessibility snapshot at {}: {source}\n\
                 Run `UPDATE_SNAPSHOTS=1 cargo nextest run -p components` to create it.",
                path.display()
            ),
            Self::Mismatch {
                snapshot_path,
                new_path,
            } => write!(
                formatter,
                "Accessibility snapshot did not match: {}\n\
                 Wrote the current tree to: {}\n\
                 Review it, then run with `UPDATE_SNAPSHOTS=1` to accept it.",
                snapshot_path.display(),
                new_path.display()
            ),
            Self::Write { path, source } => {
                write!(
                    formatter,
                    "Could not write accessibility snapshot at {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for AccessibilitySnapshotError {}

/// Adds deterministic AccessKit tree snapshots to an egui test harness.
pub trait AccessibilitySnapshot {
    /// Return the current AccessKit tree as agent-readable text.
    fn accessibility_tree(&self, options: &AccessibilityTreeOptions) -> String;

    /// Compare the current tree with `{name}.accessibility.txt`.
    #[track_caller]
    fn accessibility_snapshot(&self, name: impl AsRef<str>) {
        self.accessibility_snapshot_with_options(name, &AccessibilityTreeOptions::default());
    }

    /// Compare the current tree using custom formatting and output options.
    #[track_caller]
    fn accessibility_snapshot_with_options(
        &self,
        name: impl AsRef<str>,
        options: &AccessibilityTreeOptions,
    ) {
        if let Err(error) = self.try_accessibility_snapshot_with_options(name, options) {
            panic!("{error}");
        }
    }

    /// Fallibly compare the current tree with its committed fixture.
    fn try_accessibility_snapshot_with_options(
        &self,
        name: impl AsRef<str>,
        options: &AccessibilityTreeOptions,
    ) -> Result<(), AccessibilitySnapshotError>;

    /// Write the current tree directly to `{name}.accessibility.txt` without comparing it.
    fn write_accessibility_tree(
        &self,
        name: impl AsRef<str>,
        options: &AccessibilityTreeOptions,
    ) -> Result<PathBuf, AccessibilitySnapshotError>;
}

impl<State> AccessibilitySnapshot for Harness<'_, State> {
    fn accessibility_tree(&self, options: &AccessibilityTreeOptions) -> String {
        let viewport = self.ctx.viewport_rect();
        let pixels_per_point = self.ctx.pixels_per_point();
        let mut output = format!(
            "viewport: width={} height={} points, pixels_per_point={}\n",
            coordinate(viewport.width()),
            coordinate(viewport.height()),
            coordinate(pixels_per_point),
        );
        format_node(
            &mut output,
            &self.root(),
            pixels_per_point,
            0,
            true,
            options,
        );
        output
    }

    fn try_accessibility_snapshot_with_options(
        &self,
        name: impl AsRef<str>,
        options: &AccessibilityTreeOptions,
    ) -> Result<(), AccessibilitySnapshotError> {
        let paths = snapshot_paths(&options.output_path, name.as_ref());
        create_parent_directory(&paths.snapshot_path)?;

        let current = self.accessibility_tree(options);
        let mode = SnapshotMode::from_env();
        let previous = match std::fs::read_to_string(&paths.snapshot_path) {
            Ok(previous) => previous,
            Err(source) if source.kind() == io::ErrorKind::NotFound && mode.is_update() => {
                return update_snapshot(&paths, &current);
            }
            Err(source) => {
                write_file(&paths.new_path, &current)?;
                return Err(AccessibilitySnapshotError::Read {
                    path: paths.snapshot_path,
                    source,
                });
            }
        };

        if previous == current && mode != SnapshotMode::UpdateAll {
            remove_diagnostic_files(&paths)?;
            return Ok(());
        }

        match mode {
            SnapshotMode::Test => {
                remove_file_if_present(&paths.old_path)?;
                write_file(&paths.new_path, &current)?;
                Err(AccessibilitySnapshotError::Mismatch {
                    snapshot_path: paths.snapshot_path,
                    new_path: paths.new_path,
                })
            }
            SnapshotMode::UpdateFailing | SnapshotMode::UpdateAll => {
                update_snapshot(&paths, &current)
            }
        }
    }

    fn write_accessibility_tree(
        &self,
        name: impl AsRef<str>,
        options: &AccessibilityTreeOptions,
    ) -> Result<PathBuf, AccessibilitySnapshotError> {
        let path = snapshot_paths(&options.output_path, name.as_ref()).snapshot_path;
        create_parent_directory(&path)?;
        write_file(&path, &self.accessibility_tree(options))?;
        Ok(path)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapshotMode {
    Test,
    UpdateFailing,
    UpdateAll,
}

impl SnapshotMode {
    fn from_env() -> Self {
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

    fn is_update(self) -> bool {
        self != Self::Test
    }
}

struct SnapshotPaths {
    snapshot_path: PathBuf,
    new_path: PathBuf,
    old_path: PathBuf,
}

fn snapshot_paths(output_path: &Path, name: &str) -> SnapshotPaths {
    SnapshotPaths {
        snapshot_path: output_path.join(format!("{name}.accessibility.txt")),
        new_path: output_path.join(format!("{name}.accessibility.new.txt")),
        old_path: output_path.join(format!("{name}.accessibility.old.txt")),
    }
}

fn create_parent_directory(path: &Path) -> Result<(), AccessibilitySnapshotError> {
    let parent = path.parent().expect("snapshot path always has a parent");
    std::fs::create_dir_all(parent).map_err(|source| AccessibilitySnapshotError::Write {
        path: parent.to_owned(),
        source,
    })
}

fn write_file(path: &Path, contents: &str) -> Result<(), AccessibilitySnapshotError> {
    std::fs::write(path, contents).map_err(|source| AccessibilitySnapshotError::Write {
        path: path.to_owned(),
        source,
    })
}

fn update_snapshot(paths: &SnapshotPaths, current: &str) -> Result<(), AccessibilitySnapshotError> {
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

fn remove_diagnostic_files(paths: &SnapshotPaths) -> Result<(), AccessibilitySnapshotError> {
    remove_file_if_present(&paths.new_path)?;
    remove_file_if_present(&paths.old_path)
}

fn remove_file_if_present(path: &Path) -> Result<(), AccessibilitySnapshotError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(AccessibilitySnapshotError::Write {
            path: path.to_owned(),
            source,
        }),
    }
}

fn format_node(
    output: &mut String,
    node: &Node<'_>,
    pixels_per_point: f32,
    depth: usize,
    is_root: bool,
    options: &AccessibilityTreeOptions,
) {
    let accesskit_node = node.accesskit_node();
    let include = is_root
        || options.include_structural_nodes
        || accesskit_node.role() != Role::GenericContainer
        || accesskit_node.label().is_some();
    let child_depth = depth + usize::from(include);

    if include {
        output.push_str(&"  ".repeat(depth));
        let _ = write!(output, "{:?}", accesskit_node.role());
        if let Some(name) = accesskit_node.label() {
            let _ = write!(output, " name={name:?}");
        }
        if let Some(value) = accesskit_node.value() {
            let _ = write!(output, " value={value:?}");
        }

        let mut states = Vec::new();
        if accesskit_node.is_focused() {
            states.push("focused".to_owned());
        }
        if accesskit_node.is_disabled() {
            states.push("disabled".to_owned());
        }
        if accesskit_node.is_hidden() {
            states.push("hidden".to_owned());
        }
        if accesskit_node.is_selected() == Some(true) {
            states.push("selected".to_owned());
        }
        if let Some(toggled) = accesskit_node.toggled() {
            states.push(format!("toggled={toggled:?}"));
        }
        if !states.is_empty() {
            let _ = write!(output, " state=[{}]", states.join(", "));
        }

        match accesskit_node.bounding_box() {
            Some(rect) => {
                let x = rect.x0 as f32 / pixels_per_point;
                let y = rect.y0 as f32 / pixels_per_point;
                let width = (rect.x1 - rect.x0) as f32 / pixels_per_point;
                let height = (rect.y1 - rect.y0) as f32 / pixels_per_point;
                let center_x = x + width / 2.0;
                let center_y = y + height / 2.0;
                let _ = write!(
                    output,
                    " rect=(x={} y={} width={} height={} center_x={} center_y={})",
                    coordinate(x),
                    coordinate(y),
                    coordinate(width),
                    coordinate(height),
                    coordinate(center_x),
                    coordinate(center_y),
                );
            }
            None => output.push_str(" rect=<none>"),
        }
        output.push('\n');
    }

    for child in node.children() {
        format_node(
            output,
            &child,
            pixels_per_point,
            child_depth,
            false,
            options,
        );
    }
}

fn coordinate(value: f32) -> String {
    let value = if value.abs() < f32::EPSILON {
        0.0
    } else {
        value
    };
    format!("{value:.1}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_DIRECTORY_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn test_directory() -> PathBuf {
        let counter = TEST_DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "kubernetes-dev-ui-accessibility-snapshot-{}-{counter}",
            std::process::id()
        ))
    }

    #[test]
    fn accessibility_tree_reports_semantics_and_point_rectangles() {
        let mut checked = true;
        let harness = Harness::new_ui(|ui| {
            ui.vertical(|ui| {
                ui.label("A \"quoted\" label");
                ui.checkbox(&mut checked, "Enabled");
            });
        });

        let tree = harness.accessibility_tree(&AccessibilityTreeOptions::default());

        assert!(
            tree.starts_with("viewport: width=800.0 height=600.0 points, pixels_per_point=1.0\n")
        );
        assert!(tree.contains("Label value=\"A \\\"quoted\\\" label\" rect=(x="));
        assert!(tree.contains("center_x="));
        assert!(tree.contains("CheckBox name=\"Enabled\" state=[toggled=True] rect=(x="));
    }

    #[test]
    fn structural_node_option_removes_unlabeled_generic_containers() {
        let harness = Harness::new_ui(|ui| {
            ui.vertical(|ui| {
                let _ = ui.button("Visible action");
            });
        });

        let complete = harness.accessibility_tree(&AccessibilityTreeOptions::default());
        let semantic = harness
            .accessibility_tree(&AccessibilityTreeOptions::new().include_structural_nodes(false));

        assert!(
            complete.matches("GenericContainer").count()
                > semantic.matches("GenericContainer").count()
        );
        assert!(semantic.contains("Button name=\"Visible action\""));
    }

    #[test]
    fn snapshot_paths_keep_text_fixtures_distinct_from_image_snapshots() {
        let paths = snapshot_paths(Path::new("tests/snapshots"), "buttons/variants");

        assert_eq!(
            paths.snapshot_path,
            PathBuf::from("tests/snapshots/buttons/variants.accessibility.txt")
        );
        assert_eq!(
            paths.new_path,
            PathBuf::from("tests/snapshots/buttons/variants.accessibility.new.txt")
        );
        assert_eq!(
            paths.old_path,
            PathBuf::from("tests/snapshots/buttons/variants.accessibility.old.txt")
        );
    }

    #[test]
    fn updating_a_snapshot_preserves_the_previous_text_for_review() {
        let output_path = test_directory();
        let paths = snapshot_paths(&output_path, "example");
        create_parent_directory(&paths.snapshot_path).unwrap();
        write_file(&paths.snapshot_path, "previous tree\n").unwrap();
        write_file(&paths.new_path, "stale candidate\n").unwrap();
        write_file(&paths.old_path, "stale backup\n").unwrap();

        update_snapshot(&paths, "current tree\n").unwrap();

        assert_eq!(
            std::fs::read_to_string(&paths.snapshot_path).unwrap(),
            "current tree\n"
        );
        assert_eq!(
            std::fs::read_to_string(&paths.old_path).unwrap(),
            "previous tree\n"
        );
        assert!(!paths.new_path.exists());

        std::fs::remove_dir_all(output_path).unwrap();
    }
}
