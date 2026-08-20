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

impl<State> UiHarnessSnapshot for Harness<'_, State> {
    fn try_ui_harness(
        &mut self,
        options: impl Into<HarnessSnapshotOptions>,
    ) -> Result<(), UiHarnessSnapshotError> {
        let options = options.into();
        let pixel = self.try_snapshot_options(&options.name, &options.pixel);
        let accessibility =
            self.try_accessibility_snapshot_with_options(&options.name, &options.accessibility);
        let nodes = current_accessibility_nodes(self);
        let labels = find_unlabeled_interactive_nodes(&nodes);
        let overlaps = if options.accessibility.check_illegal_overlaps {
            find_illegal_overlaps(&nodes)
        } else {
            Vec::new()
        };

        let error = UiHarnessSnapshotError {
            pixel: pixel.err().map(Box::new),
            accessibility: accessibility.err().map(Box::new),
            labels,
            overlaps,
        };
        if error.pixel.is_none()
            && error.accessibility.is_none()
            && error.labels.is_empty()
            && error.overlaps.is_empty()
        {
            Ok(())
        } else {
            Err(error)
        }
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

const MINIMUM_OVERLAP_SIZE: f32 = 1.0;

#[derive(Clone, Debug, PartialEq)]
struct AccessibilityNodeDescription {
    role: String,
    name: Option<String>,
    value: Option<String>,
    rect: Rect,
}

impl Display for AccessibilityNodeDescription {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.role)?;
        if let Some(name) = &self.name {
            write!(formatter, " name={name:?}")?;
        }
        if let Some(value) = &self.value {
            write!(formatter, " value={value:?}")?;
        }
        write!(formatter, " {}", format_rect(self.rect))
    }
}

#[derive(Clone, Debug)]
struct AccessibilityNodeInfo {
    description: AccessibilityNodeDescription,
    actions: Vec<Action>,
    child_count: usize,
    parent: Option<usize>,
    layer: Option<usize>,
    hidden: bool,
}

fn collect_accessibility_nodes(
    root: &Node<'_>,
    pixels_per_point: f32,
    viewport: Rect,
    nodes: &mut Vec<AccessibilityNodeInfo>,
) {
    let root_index =
        collect_accessibility_node(root, pixels_per_point, viewport, None, None, nodes);
    for (layer, child) in root.children().enumerate() {
        collect_accessibility_branch(
            &child,
            pixels_per_point,
            viewport,
            Some(root_index),
            Some(layer),
            nodes,
        );
    }
}

fn collect_accessibility_branch(
    node: &Node<'_>,
    pixels_per_point: f32,
    visible_rect: Rect,
    parent: Option<usize>,
    layer: Option<usize>,
    nodes: &mut Vec<AccessibilityNodeInfo>,
) {
    let index =
        collect_accessibility_node(node, pixels_per_point, visible_rect, parent, layer, nodes);
    let child_visible_rect = clip_rect_for_scrollbars(node, pixels_per_point, visible_rect);
    for child in node.children() {
        collect_accessibility_branch(
            &child,
            pixels_per_point,
            child_visible_rect,
            Some(index),
            layer,
            nodes,
        );
    }
}

fn collect_accessibility_node(
    node: &Node<'_>,
    pixels_per_point: f32,
    visible_rect: Rect,
    parent: Option<usize>,
    layer: Option<usize>,
    nodes: &mut Vec<AccessibilityNodeInfo>,
) -> usize {
    let accesskit_node = node.accesskit_node();
    let child_count = node.children().count();
    let rect = accesskit_rect(accesskit_node.bounding_box(), pixels_per_point)
        .map(|rect| rect.intersect(visible_rect));
    let index = nodes.len();
    if let Some(rect) = rect {
        nodes.push(AccessibilityNodeInfo {
            description: AccessibilityNodeDescription {
                role: format!("{:?}", accesskit_node.role()),
                name: accesskit_node.label(),
                value: accesskit_node.value(),
                rect,
            },
            actions: label_required_actions(&accesskit_node),
            child_count,
            parent,
            layer,
            hidden: accesskit_node.is_hidden() || !rect.is_positive(),
        });
    } else {
        nodes.push(AccessibilityNodeInfo {
            description: AccessibilityNodeDescription {
                role: format!("{:?}", accesskit_node.role()),
                name: accesskit_node.label(),
                value: accesskit_node.value(),
                rect: Rect::NOTHING,
            },
            actions: label_required_actions(&accesskit_node),
            child_count,
            parent,
            layer,
            hidden: true,
        });
    }

    index
}

fn label_required_actions(node: &egui_kittest::kittest::AccessKitNode<'_>) -> Vec<Action> {
    const ACTIONS: [Action; 11] = [
        Action::Click,
        Action::Focus,
        Action::Collapse,
        Action::Expand,
        Action::CustomAction,
        Action::Decrement,
        Action::Increment,
        Action::ReplaceSelectedText,
        Action::SetTextSelection,
        Action::SetValue,
        Action::ShowContextMenu,
    ];

    ACTIONS
        .iter()
        .copied()
        .filter(|action| node.data().supports_action(*action))
        .collect()
}

fn current_accessibility_nodes<State>(harness: &Harness<'_, State>) -> Vec<AccessibilityNodeInfo> {
    let mut nodes = Vec::new();
    collect_accessibility_nodes(
        &harness.root(),
        harness.ctx.pixels_per_point(),
        harness.ctx.viewport_rect(),
        &mut nodes,
    );
    nodes
}

fn clip_rect_for_scrollbars(
    node: &Node<'_>,
    pixels_per_point: f32,
    mut visible_rect: Rect,
) -> Rect {
    for child in node.children() {
        let child_node = child.accesskit_node();
        if child_node.role() != Role::ScrollBar {
            continue;
        }
        let Some(scrollbar_rect) = accesskit_rect(child_node.bounding_box(), pixels_per_point)
        else {
            continue;
        };

        if scrollbar_rect.height() > scrollbar_rect.width() {
            visible_rect.min.y = visible_rect.min.y.max(scrollbar_rect.min.y);
            visible_rect.max.y = visible_rect.max.y.min(scrollbar_rect.max.y);
        } else {
            visible_rect.min.x = visible_rect.min.x.max(scrollbar_rect.min.x);
            visible_rect.max.x = visible_rect.max.x.min(scrollbar_rect.max.x);
        }
    }
    visible_rect
}

fn accesskit_rect(rect: Option<egui::accesskit::Rect>, pixels_per_point: f32) -> Option<Rect> {
    rect.map(|rect| {
        Rect::from_min_max(
            Pos2::new(
                rect.x0 as f32 / pixels_per_point,
                rect.y0 as f32 / pixels_per_point,
            ),
            Pos2::new(
                rect.x1 as f32 / pixels_per_point,
                rect.y1 as f32 / pixels_per_point,
            ),
        )
    })
}

fn find_illegal_overlaps(nodes: &[AccessibilityNodeInfo]) -> Vec<AccessibilityOverlap> {
    let mut overlaps = Vec::new();
    for (first_index, first) in nodes.iter().enumerate() {
        if !is_overlap_candidate(first_index, nodes) {
            continue;
        }
        for (second_index, second) in nodes.iter().enumerate().skip(first_index + 1) {
            if !is_overlap_candidate(second_index, nodes)
                || first.layer != second.layer
                || nodes_are_related(first_index, second_index, nodes)
                || is_composite_control_content(first, second)
                || is_composite_control_content(second, first)
            {
                continue;
            }

            let intersection = first.description.rect.intersect(second.description.rect);
            if intersection.width() >= MINIMUM_OVERLAP_SIZE
                && intersection.height() >= MINIMUM_OVERLAP_SIZE
            {
                overlaps.push(AccessibilityOverlap {
                    first: first.description.clone(),
                    second: second.description.clone(),
                    intersection,
                });
            }
        }
    }
    overlaps
}

fn find_unlabeled_interactive_nodes(
    nodes: &[AccessibilityNodeInfo],
) -> Vec<AccessibilityLabelViolation> {
    nodes
        .iter()
        .filter(|node| {
            !node.hidden
                && !node.actions.is_empty()
                // egui labels expose Click for text selection even when they are not controls.
                && !matches!(
                    node.description.role.as_str(),
                    "Label" | "ScrollBar" | "TextRun"
                )
                // egui represents structural surfaces such as menus, tooltips, and scroll
                // containers as action-bearing Unknown/GenericContainer parents. Their child
                // controls are checked independently; a leaf of either role comes from a
                // direct `ui.interact` and must itself have a name.
                && (!matches!(
                    node.description.role.as_str(),
                    "GenericContainer" | "Unknown"
                ) || node.child_count == 0)
                && node
                    .description
                    .name
                    .as_deref()
                    .is_none_or(|name| name.trim().is_empty())
        })
        .map(|node| AccessibilityLabelViolation {
            description: node.description.clone(),
            actions: node.actions.clone(),
        })
        .collect()
}

fn is_overlap_candidate(index: usize, nodes: &[AccessibilityNodeInfo]) -> bool {
    let node = &nodes[index];
    !node.hidden
        && node.description.rect.is_positive()
        && !matches!(
            node.description.role.as_str(),
            "Window" | "Unknown" | "GenericContainer" | "Image" | "ScrollBar"
        )
        && (node.description.role != "Label" || has_descendant_role(index, "TextRun", nodes))
}

fn is_composite_control_content(
    outer: &AccessibilityNodeInfo,
    inner: &AccessibilityNodeInfo,
) -> bool {
    let contains_inner = outer.description.rect.contains_rect(inner.description.rect);
    (outer.description.role == "ComboBox"
        && matches!(inner.description.role.as_str(), "TextInput" | "TextRun")
        && contains_inner)
        || (outer.description.role == "Button"
            && matches!(inner.description.role.as_str(), "Label" | "TextRun")
            && contains_inner
            && inner
                .description
                .name
                .as_deref()
                .or(inner.description.value.as_deref())
                .is_some_and(|text| {
                    outer
                        .description
                        .name
                        .as_deref()
                        .is_some_and(|name| name.contains(text))
                }))
}

fn has_descendant_role(index: usize, role: &str, nodes: &[AccessibilityNodeInfo]) -> bool {
    nodes.iter().enumerate().any(|(candidate, node)| {
        node.description.role == role && is_ancestor(index, candidate, nodes)
    })
}

fn nodes_are_related(first: usize, second: usize, nodes: &[AccessibilityNodeInfo]) -> bool {
    is_ancestor(first, second, nodes) || is_ancestor(second, first, nodes)
}

fn is_ancestor(ancestor: usize, mut descendant: usize, nodes: &[AccessibilityNodeInfo]) -> bool {
    while let Some(parent) = nodes[descendant].parent {
        if parent == ancestor {
            return true;
        }
        descendant = parent;
    }
    false
}

fn illegal_overlaps_message(overlaps: &[AccessibilityOverlap]) -> String {
    let overlaps = overlaps
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    format!("Illegal accessibility overlaps:\n{overlaps}")
}

fn missing_labels_message(labels: &[AccessibilityLabelViolation]) -> String {
    let labels = labels
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    format!("Interactive accessibility nodes without labels:\n{labels}")
}

fn format_rect(rect: Rect) -> String {
    let width = rect.width();
    let height = rect.height();
    format!(
        "rect=(x={} y={} width={} height={} center_x={} center_y={})",
        coordinate(rect.min.x),
        coordinate(rect.min.y),
        coordinate(width),
        coordinate(height),
        coordinate(rect.center().x),
        coordinate(rect.center().y),
    )
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
            "styrhous-accessibility-snapshot-{}-{counter}",
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
    fn label_detection_rejects_unnamed_and_whitespace_named_interactive_controls() {
        let mut text = String::new();
        let harness = Harness::new_ui(|ui| {
            ui.add(egui::TextEdit::singleline(&mut text));
            let _ = ui.button(" ");
            let (_, custom_rect) = ui.allocate_space(egui::vec2(80.0, 24.0));
            ui.interact(
                custom_rect,
                egui::Id::new("unnamed-custom-control"),
                egui::Sense::click(),
            );
            let (_, image_rect) = ui.allocate_space(egui::vec2(80.0, 24.0));
            let image = ui.interact(
                image_rect,
                egui::Id::new("unnamed-clickable-image"),
                egui::Sense::click(),
            );
            image.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, ""));
            let _ = ui.button("Save changes");
            ui.label("Passive description");
        });

        let violations =
            harness.unlabeled_interactive_accessibility_nodes(&AccessibilityTreeOptions::default());

        assert_eq!(
            violations.len(),
            4,
            "{}",
            missing_labels_message(&violations)
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.description.role == "TextInput")
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.description.role == "Button")
        );
        assert!(violations.iter().any(|violation| {
            matches!(
                violation.description.role.as_str(),
                "GenericContainer" | "Unknown"
            )
        }));
        assert!(
            violations
                .iter()
                .any(|violation| violation.description.role == "Image")
        );
        assert!(violations.iter().all(|violation| {
            violation
                .description
                .name
                .as_deref()
                .is_none_or(|name| name.trim().is_empty())
        }));
        assert!(missing_labels_message(&violations).contains("actions:"));
    }

    #[test]
    fn illegal_overlap_detection_reports_a_text_run_colliding_with_a_button() {
        let harness = Harness::new_ui(|ui| {
            let rect = Rect::from_min_size(ui.min_rect().min, egui::vec2(180.0, 28.0));
            ui.put(rect, egui::Label::new("Overlapping text"));
            ui.put(rect, egui::Button::new("Colliding button"));
        });

        let overlaps = harness.illegal_accessibility_overlaps(&AccessibilityTreeOptions::default());
        let message = illegal_overlaps_message(&overlaps);

        assert!(
            !overlaps.is_empty(),
            "the deliberately bad UI must be rejected"
        );
        assert!(message.contains("TextRun value=\"Overlapping text\""));
        assert!(message.contains("Button name=\"Colliding button\""));
        assert!(message.contains("overlaps"));
    }

    #[test]
    fn overlap_detection_allows_related_text_and_edge_touching_widgets() {
        let harness = Harness::new_ui(|ui| {
            ui.label("A label owns this text run");
            let left = Rect::from_min_size(
                ui.min_rect().min + egui::vec2(0.0, 40.0),
                egui::vec2(80.0, 28.0),
            );
            let right = Rect::from_min_size(left.right_top(), egui::vec2(80.0, 28.0));
            ui.put(left, egui::Button::new("Left"));
            ui.put(right, egui::Button::new("Right"));
        });

        assert!(
            harness
                .illegal_accessibility_overlaps(&AccessibilityTreeOptions::default())
                .is_empty()
        );
    }

    #[test]
    fn overlap_detection_automatically_ignores_a_foreground_area() {
        let harness = Harness::new_ui(|ui| {
            let rect = Rect::from_min_size(ui.min_rect().min, egui::vec2(180.0, 28.0));
            ui.put(rect, egui::Button::new("Underlying button"));
            egui::Area::new(egui::Id::new("overlap-test-area"))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.min)
                .show(ui.ctx(), |ui| {
                    ui.add_sized(rect.size(), egui::Button::new("Foreground button"));
                });
        });

        assert!(
            harness
                .illegal_accessibility_overlaps(&AccessibilityTreeOptions::default())
                .is_empty()
        );
    }

    #[test]
    fn overlap_detection_ignores_nonvisual_semantic_annotations() {
        let harness = Harness::new_ui(|ui| {
            let rect = Rect::from_min_size(ui.min_rect().min, egui::vec2(180.0, 28.0));
            ui.put(rect, egui::Label::new("Visible text"));
            let annotation = ui.interact(
                rect,
                egui::Id::new("nonvisual-semantic-annotation"),
                egui::Sense::hover(),
            );
            annotation.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Label,
                    true,
                    "Validation error annotation",
                )
            });
        });

        assert!(
            harness
                .illegal_accessibility_overlaps(&AccessibilityTreeOptions::default())
                .is_empty()
        );
    }

    #[test]
    fn overlap_detection_respects_scrollbar_clip_bounds() {
        let harness = Harness::new_ui(|ui| {
            egui::ScrollArea::vertical()
                .max_height(48.0)
                .show(ui, |ui| {
                    for index in 0..10 {
                        ui.add_sized(
                            egui::vec2(160.0, 20.0),
                            egui::Button::new(format!("Scrollable item {index}")),
                        );
                    }
                });
            ui.label("Footer below the scroll area");
        });

        assert!(
            harness
                .illegal_accessibility_overlaps(&AccessibilityTreeOptions::default())
                .is_empty()
        );
    }

    #[test]
    fn overlap_detection_can_be_disabled_for_an_exceptional_test() {
        let harness = Harness::new_ui(|ui| {
            let rect = Rect::from_min_size(ui.min_rect().min, egui::vec2(180.0, 28.0));
            ui.put(rect, egui::Label::new("Overlapping text"));
            ui.put(rect, egui::Button::new("Colliding button"));
        });

        assert!(
            harness
                .illegal_accessibility_overlaps(
                    &AccessibilityTreeOptions::new().check_illegal_overlaps(false),
                )
                .is_empty()
        );
    }

    #[test]
    fn snapshot_paths_keep_text_fixtures_distinct_from_image_snapshots() {
        let paths = snapshot_paths(
            Path::new("tests/snapshots"),
            "buttons/test_buttons/variants",
        );

        assert_eq!(
            paths.snapshot_path,
            PathBuf::from("tests/snapshots/buttons/test_buttons/variants.accessibility.txt")
        );
        assert_eq!(
            paths.new_path,
            PathBuf::from("tests/snapshots/buttons/test_buttons/variants.accessibility.new.txt")
        );
        assert_eq!(
            paths.old_path,
            PathBuf::from("tests/snapshots/buttons/test_buttons/variants.accessibility.old.txt")
        );
    }

    #[test]
    fn harness_snapshot_options_keep_pixel_and_accessibility_output_together() {
        let options = HarnessSnapshotOptions::from("example")
            .output_path("custom-snapshots")
            .include_structural_nodes(false);

        assert_eq!(options.name, "example");
        assert_eq!(options.pixel.threshold, DEFAULT_PIXEL_THRESHOLD);
        assert_eq!(options.pixel.max_failed_pixels, 0);
        assert_eq!(options.pixel.output_path, PathBuf::from("custom-snapshots"));
        assert_eq!(
            options.accessibility.output_path,
            PathBuf::from("custom-snapshots")
        );
        assert!(!options.accessibility.include_structural_nodes);
        assert!(options.accessibility.check_illegal_overlaps);

        let strict = HarnessSnapshotOptions::strict("strict");
        assert_eq!(strict.pixel.threshold, SnapshotOptions::new().threshold);
        assert_eq!(strict.pixel.max_failed_pixels, 0);

        let one_pixel = HarnessSnapshotOptions::one_pixel("one-pixel");
        assert_eq!(one_pixel.pixel.threshold, SnapshotOptions::new().threshold);
        assert_eq!(one_pixel.pixel.max_failed_pixels, 1);
    }

    #[test]
    fn ui_harness_writes_both_candidates_when_both_fixtures_are_missing() {
        let output_path = test_directory();
        let mut harness = Harness::new_ui(|ui| {
            ui.label("Snapshot me");
        });

        let result = harness
            .try_ui_harness(HarnessSnapshotOptions::new("example").output_path(&output_path));

        if SnapshotMode::from_env().is_update() {
            result.expect("update mode should create both fixtures");
            assert!(output_path.join("example.png").exists());
            assert!(output_path.join("example.accessibility.txt").exists());
        } else {
            let error = result.expect_err("missing fixtures must fail");
            assert!(error.pixel.is_some());
            assert!(error.accessibility.is_some());
            assert!(error.overlaps.is_empty());
            assert!(output_path.join("example.new.png").exists());
            assert!(output_path.join("example.accessibility.new.txt").exists());
        }

        std::fs::remove_dir_all(output_path).unwrap();
    }

    #[test]
    fn ui_harness_reports_missing_interactive_labels_with_snapshot_failures() {
        let output_path = test_directory();
        let mut text = String::new();
        let mut harness = Harness::new_ui(|ui| {
            ui.add(egui::TextEdit::singleline(&mut text));
        });

        let error = harness
            .try_ui_harness(HarnessSnapshotOptions::new("unlabeled").output_path(&output_path))
            .expect_err("an unnamed interactive control must fail the combined snapshot");

        assert_eq!(error.labels.len(), 1);
        assert!(
            error
                .to_string()
                .contains("Interactive accessibility nodes without labels")
        );
        if SnapshotMode::from_env().is_update() {
            assert!(error.pixel.is_none());
            assert!(error.accessibility.is_none());
            assert!(output_path.join("unlabeled.png").exists());
            assert!(output_path.join("unlabeled.accessibility.txt").exists());
        } else {
            assert!(error.pixel.is_some());
            assert!(error.accessibility.is_some());
            assert!(output_path.join("unlabeled.new.png").exists());
            assert!(output_path.join("unlabeled.accessibility.new.txt").exists());
        }
        std::fs::remove_dir_all(output_path).unwrap();
    }

    #[test]
    fn ui_harness_keeps_label_validation_when_overlap_checks_are_disabled() {
        let output_path = test_directory();
        let mut text = String::new();
        let mut harness = Harness::new_ui(|ui| {
            ui.add(egui::TextEdit::singleline(&mut text));
        });

        let error = harness
            .try_ui_harness(
                HarnessSnapshotOptions::new("unlabeled")
                    .check_illegal_overlaps(false)
                    .output_path(&output_path),
            )
            .expect_err("disabling overlap checks must not disable label validation");

        assert_eq!(error.labels.len(), 1);
        assert!(error.overlaps.is_empty());
        std::fs::remove_dir_all(output_path).unwrap();
    }

    #[test]
    fn ui_harness_rejects_illegal_overlaps_even_when_updating_snapshots() {
        let output_path = test_directory();
        let mut harness = Harness::new_ui(|ui| {
            let rect = Rect::from_min_size(ui.min_rect().min, egui::vec2(180.0, 28.0));
            ui.put(rect, egui::Label::new("Overlapping text"));
            ui.put(rect, egui::Button::new("Colliding button"));
        });

        let error = harness
            .try_ui_harness(HarnessSnapshotOptions::new("overlap").output_path(&output_path))
            .expect_err("an illegal overlap must always fail the combined snapshot");

        assert!(!error.overlaps.is_empty());
        if SnapshotMode::from_env().is_update() {
            assert!(error.pixel.is_none());
            assert!(error.accessibility.is_none());
            assert!(output_path.join("overlap.png").exists());
            assert!(output_path.join("overlap.accessibility.txt").exists());
        } else {
            assert!(error.pixel.is_some());
            assert!(error.accessibility.is_some());
            assert!(output_path.join("overlap.new.png").exists());
            assert!(output_path.join("overlap.accessibility.new.txt").exists());
        }

        std::fs::remove_dir_all(output_path).unwrap();
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
