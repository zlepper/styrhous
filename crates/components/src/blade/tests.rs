use super::*;
use crate::test_support::{AccessibilitySnapshot, UiHarnessSnapshot};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
struct TestBlade {
    id: u64,
    title: &'static str,
}

fn render_test_blade(ui: &mut Ui, blade: &mut TestBlade, layer: BladeLayer) {
    ui.label(format!(
        "{} · {}",
        if layer.is_foreground {
            "Active"
        } else {
            "History"
        },
        blade.title
    ));
    ui.label(format!(
        "back: {} · forward: {}",
        layer.can_go_back, layer.can_go_forward
    ));
}

mod history;
mod mouse;
mod navigation;
mod snapshots;
