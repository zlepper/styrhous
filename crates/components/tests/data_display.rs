//! Focused snapshots of the C13 data display components, so a regression
//! shows up next to the component instead of only on the whole C13 board
//! (`c13_data_display.rs`).

use components::design::space;
use components::design::theme::Tone;
use components::test_support::{UiHarnessSnapshot, canvas_harness};
use components::{
    Card, Chip, Disclosure, Property, PropertyGrid, UsageChart, UsageResource, chip_flow,
};
use egui::{Id, Ui};
use egui_kittest::Harness;

/// A harness `width` × `height` points showing `scene` on the canvas,
/// padded by 16 points.
fn harness(width: f32, height: f32, scene: impl Fn(&mut Ui) + 'static) -> Harness<'static> {
    canvas_harness(width, height, Some(space::S16), scene)
}

/// Two columns between 400 and 600 points; long values elide, missing
/// values read "Unavailable", references are links.
#[test]
fn property_grid_in_two_columns() {
    let mut harness = harness(480.0, 200.0, |ui| {
        Card::new().show(ui, |ui| {
            let properties = [
                Property::text("Name", "coredns-66bc5c9577-ffw2s-with-a-very-long-suffix"),
                Property::link("Node", "kind-control-plane", Id::new("node")),
                Property::unavailable("Pod IP"),
                Property::status("Status", "Pending", Tone::Warning),
                Property::mono("Image", "registry.k8s.io/coredns/coredns:v1.11.1").copyable(),
            ];
            let _ = PropertyGrid::new(&properties).show(ui);
        });
    });
    harness.ui_harness("data_display/property_grid_two_columns");
}

/// A chip longer than its maximum width truncates; chips wrap in rows.
#[test]
fn chips_wrap_and_truncate() {
    let mut harness = harness(480.0, 180.0, |ui| {
        Disclosure::new("chips", "Annotations")
            .meta("3 annotations")
            .force_open(true)
            .show(ui, |ui| {
                chip_flow(ui, |ui| {
                    Chip::new("true")
                        .key("kubernetes.io/cluster-service=")
                        .show(ui);
                    Chip::new("a-value-long-enough-to-run-past-the-chip-maximum-width")
                        .key("example.com/annotation=")
                        .show(ui);
                    Chip::new("53/UDP").show(ui);
                });
            });
    });
    harness.ui_harness("data_display/chips_wrap_and_truncate");
}

/// A chart with a request line, and one without metrics.
#[test]
fn usage_chart_request_and_unavailable() {
    fn millicores(value: f32) -> String {
        format!("{value}m")
    }
    let mut harness = harness(360.0, 300.0, |ui| {
        let samples = [[0.0, 20.0], [0.5, 45.0], [1.0, 30.0]];
        UsageChart::new("CPU", UsageResource::Cpu, "30m")
            .limit(100.0, "Limit")
            .request(50.0, "Request")
            .format(&millicores)
            .samples(&samples)
            .show(ui);
        UsageChart::new("Memory", UsageResource::Memory, "")
            .unavailable("metrics-server is not installed")
            .show(ui);
    });
    harness.ui_harness("data_display/usage_chart_request_and_unavailable");
}
