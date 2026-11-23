//! The user theme file switching the theme of the running application.

use super::*;
use components::design::theme::{EXAMPLE_THEME_FILE, ThemeExt};
use components::test_support::{AccessibilitySnapshot, AccessibilityTreeOptions};

/// Write `contents` as a theme file in a new temporary directory.
fn theme_file(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("theme.toml");
    std::fs::write(&path, contents).expect("the theme file is writable");
    (dir, path)
}

#[test]
fn theme_switch_recolours_workspace_and_blade() {
    let mut harness = application_harness::<MockWorker>();
    show_apps_resource_table(&mut harness);
    open_settings(&mut harness);
    let tree_options = AccessibilityTreeOptions::default();
    let before = harness.accessibility_tree(&tree_options);
    assert_eq!(harness.ctx.tokens().name(), "Harbour Dark");

    let (_dir, path) = theme_file(EXAMPLE_THEME_FILE);
    harness.state_mut().watch_theme_file(path);
    harness.run();

    assert_eq!(harness.ctx.tokens().name(), "Harbour Dark · high contrast");
    assert_eq!(
        harness.ctx.global_style().visuals.selection.bg_fill,
        harness.ctx.tokens().accent.tint,
        "egui's own visuals follow the switch"
    );
    assert_eq!(
        harness.accessibility_tree(&tree_options),
        before,
        "a theme changes colours only, never layout"
    );
    harness.ui_harness("theme/harbour_dark_hc_workspace");
}

#[test]
fn an_invalid_theme_file_keeps_the_theme_and_shows_why() {
    let mut harness = application_harness::<MockWorker>();
    harness.state_mut().ui_state = oracle_resource_table_state();
    harness.run();

    let (_dir, path) = theme_file("name = \"Typo\"\n[roles]\ntext-mutted = \"sand-200\"\n");
    harness.state_mut().watch_theme_file(path);
    harness.run();

    assert_eq!(harness.ctx.tokens().name(), "Harbour Dark");
    harness.get_by_label("Couldn’t apply the theme file");
    harness.get_by_label_contains("did you mean `text-muted`");
}
