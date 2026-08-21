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

    assert!(tree.starts_with("viewport: width=800.0 height=600.0 points, pixels_per_point=1.0\n"));
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
        complete.matches("GenericContainer").count() > semantic.matches("GenericContainer").count()
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
            egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "Validation error annotation")
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

    let result =
        harness.try_ui_harness(HarnessSnapshotOptions::new("example").output_path(&output_path));

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
