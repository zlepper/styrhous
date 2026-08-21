//! Shared setup for component UI tests.
//!
//! Keeping this setup in the component crate lets both unit tests and the
//! public-API showcase snapshots render with the same deterministic theme and
//! image loaders.

use std::fmt::{Display, Write as _};
use std::io;
use std::path::{Path, PathBuf};

use egui::accesskit::{Action, Role};
use egui::{Pos2, Rect, Vec2};
use egui_kittest::kittest::NodeT;
use egui_kittest::{Harness, Node, SnapshotError, SnapshotOptions};

/// The deterministic viewport used by all egui tests and snapshots.
pub const EGUI_TEST_SIZE: Vec2 = Vec2::new(1536.0, 1024.0);

/// The project-wide per-pixel color tolerance for UI screenshots.
///
/// This accepts the measured WGPU anti-aliasing variance at transformed shadow edges while still
/// allowing no pixels to exceed the threshold.
pub const DEFAULT_PIXEL_THRESHOLD: f32 = 2.1;

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
    /// Reject overlapping, unrelated visible nodes in the same egui layer.
    pub check_illegal_overlaps: bool,
    /// Directory containing the committed text fixtures and diagnostic dumps.
    pub output_path: PathBuf,
}

impl Default for AccessibilityTreeOptions {
    fn default() -> Self {
        Self {
            include_structural_nodes: true,
            check_illegal_overlaps: true,
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

    /// Enable or disable automatic same-layer collision detection.
    ///
    /// Disabling this should be reserved for tests that intentionally construct overlapping
    /// content in a single egui layer.
    pub fn check_illegal_overlaps(mut self, check_illegal_overlaps: bool) -> Self {
        self.check_illegal_overlaps = check_illegal_overlaps;
        self
    }

    /// Write fixtures and dumps under a custom directory.
    pub fn output_path(mut self, output_path: impl Into<PathBuf>) -> Self {
        self.output_path = output_path.into();
        self
    }
}

/// Options for a complete UI snapshot: rendered pixels plus the AccessKit tree.
#[derive(Clone, Debug)]
pub struct HarnessSnapshotOptions {
    name: String,
    pixel: SnapshotOptions,
    accessibility: AccessibilityTreeOptions,
}

impl HarnessSnapshotOptions {
    /// Create a snapshot using the project-wide pixel tolerance.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            pixel: SnapshotOptions::new().threshold(DEFAULT_PIXEL_THRESHOLD),
            accessibility: AccessibilityTreeOptions::default(),
        }
    }

    /// Create a snapshot using egui_kittest's strict default tolerance.
    pub fn strict(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            pixel: SnapshotOptions::new(),
            accessibility: AccessibilityTreeOptions::default(),
        }
    }

    /// Create a strict snapshot that permits one pixel above its color threshold.
    pub fn one_pixel(name: impl Into<String>) -> Self {
        Self::strict(name).max_failed_pixels(1)
    }

    /// Override the per-pixel color tolerance.
    pub fn threshold(mut self, threshold: f32) -> Self {
        self.pixel.threshold = threshold;
        self
    }

    /// Override the maximum number of pixels that may exceed the color tolerance.
    pub fn max_failed_pixels(mut self, max_failed_pixels: usize) -> Self {
        self.pixel.max_failed_pixels = max_failed_pixels;
        self
    }

    /// Write both image and accessibility fixtures under a custom directory.
    pub fn output_path(mut self, output_path: impl Into<PathBuf>) -> Self {
        let output_path = output_path.into();
        self.pixel.output_path = output_path.clone();
        self.accessibility.output_path = output_path;
        self
    }

    /// Include or omit unlabeled structural nodes from the accessibility fixture.
    pub fn include_structural_nodes(mut self, include_structural_nodes: bool) -> Self {
        self.accessibility.include_structural_nodes = include_structural_nodes;
        self
    }

    /// Enable or disable automatic same-layer collision detection.
    pub fn check_illegal_overlaps(mut self, check_illegal_overlaps: bool) -> Self {
        self.accessibility.check_illegal_overlaps = check_illegal_overlaps;
        self
    }
}

impl From<&str> for HarnessSnapshotOptions {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for HarnessSnapshotOptions {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}

impl From<&HarnessSnapshotOptions> for HarnessSnapshotOptions {
    fn from(options: &HarnessSnapshotOptions) -> Self {
        options.clone()
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

/// A visible interactive accessibility node without a usable accessible name.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityLabelViolation {
    description: AccessibilityNodeDescription,
    actions: Vec<Action>,
}

impl Display for AccessibilityLabelViolation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} has no non-blank accessible name (actions: {})",
            self.description,
            self.actions
                .iter()
                .map(|action| format!("{action:?}"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

/// Two unrelated visible accessibility nodes that collide in the same egui layer.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityOverlap {
    first: AccessibilityNodeDescription,
    second: AccessibilityNodeDescription,
    intersection: Rect,
}

impl Display for AccessibilityOverlap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} overlaps {} at {}",
            self.first,
            self.second,
            format_rect(self.intersection),
        )
    }
}

/// A failure from either artifact produced by [`UiHarnessSnapshot::ui_harness`].
#[derive(Debug)]
pub struct UiHarnessSnapshotError {
    pixel: Option<Box<SnapshotError>>,
    accessibility: Option<Box<AccessibilitySnapshotError>>,
    labels: Vec<AccessibilityLabelViolation>,
    overlaps: Vec<AccessibilityOverlap>,
}

impl Display for UiHarnessSnapshotError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sections = Vec::new();
        if let Some(error) = &self.pixel {
            sections.push(format!("Pixel snapshot failed:\n{error}"));
        }
        if let Some(error) = &self.accessibility {
            sections.push(format!("Accessibility snapshot failed:\n{error}"));
        }
        if !self.labels.is_empty() {
            sections.push(missing_labels_message(&self.labels));
        }
        if !self.overlaps.is_empty() {
            let overlaps = self
                .overlaps
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!("Illegal accessibility overlaps:\n{overlaps}"));
        }
        formatter.write_str(&sections.join("\n\n"))
    }
}

impl std::error::Error for UiHarnessSnapshotError {}

/// Adds deterministic AccessKit tree snapshots to an egui test harness.
pub trait AccessibilitySnapshot {
    /// Return the current AccessKit tree as agent-readable text.
    fn accessibility_tree(&self, options: &AccessibilityTreeOptions) -> String;

    /// Return unrelated visible nodes that overlap within the same egui layer.
    fn illegal_accessibility_overlaps(
        &self,
        options: &AccessibilityTreeOptions,
    ) -> Vec<AccessibilityOverlap>;

    /// Return visible interactive nodes whose accessible names are absent or blank.
    fn unlabeled_interactive_accessibility_nodes(
        &self,
        options: &AccessibilityTreeOptions,
    ) -> Vec<AccessibilityLabelViolation>;

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
        let labels = self.unlabeled_interactive_accessibility_nodes(options);
        let overlaps = self.illegal_accessibility_overlaps(options);
        if !labels.is_empty() {
            panic!("{}", missing_labels_message(&labels));
        }
        if !overlaps.is_empty() {
            panic!("{}", illegal_overlaps_message(&overlaps));
        }
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

/// Adds complete pixel and accessibility snapshots to an egui test harness.
pub trait UiHarnessSnapshot {
    /// Compare the rendered pixels and AccessKit tree for one named UI state.
    #[track_caller]
    fn ui_harness(&mut self, options: impl Into<HarnessSnapshotOptions>) {
        if let Err(error) = self.try_ui_harness(options) {
            panic!("{error}");
        }
    }

    /// Fallibly compare both artifacts for one named UI state.
    fn try_ui_harness(
        &mut self,
        options: impl Into<HarnessSnapshotOptions>,
    ) -> Result<(), UiHarnessSnapshotError>;
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

    fn illegal_accessibility_overlaps(
        &self,
        options: &AccessibilityTreeOptions,
    ) -> Vec<AccessibilityOverlap> {
        if !options.check_illegal_overlaps {
            return Vec::new();
        }

        let nodes = current_accessibility_nodes(self);
        find_illegal_overlaps(&nodes)
    }

    fn unlabeled_interactive_accessibility_nodes(
        &self,
        _options: &AccessibilityTreeOptions,
    ) -> Vec<AccessibilityLabelViolation> {
        let nodes = current_accessibility_nodes(self);
        find_unlabeled_interactive_nodes(&nodes)
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

mod accessibility;
mod harness;
mod snapshot_files;

use accessibility::*;
use snapshot_files::*;

#[cfg(test)]
mod tests;
