use components::design::spacing;
use components::test_support::{UiHarnessSnapshot, setup_egui};
use components::{ChecklistStep, ChecklistStepState, ProgressChecklist};
use egui_kittest::{Harness, kittest::Queryable};

fn step(label: &str, state: ChecklistStepState, detail: Option<&str>) -> ChecklistStep {
    ChecklistStep {
        label: label.to_owned(),
        state,
        detail: detail.map(str::to_owned),
    }
}

#[test]
fn every_step_state_has_an_icon_label_and_detail() {
    let steps = [
        step("Authenticate", ChecklistStepState::Done, Some("0.4 s")),
        step(
            "Contact API server",
            ChecklistStepState::InProgress,
            Some("3 s"),
        ),
        step("Load namespaces", ChecklistStepState::Pending, None),
        step("Open workspace", ChecklistStepState::Failed, None),
    ];
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 220.0))
        .build_ui(move |ui| {
            ui.add_space(spacing::LG);
            ui.horizontal(|ui| {
                ui.add_space(spacing::LG);
                ProgressChecklist::new("Connecting to dev", &steps).show(ui);
            });
        });
    setup_egui(&mut harness);
    harness.set_size(egui::vec2(400.0, 220.0));
    // The in-progress spinner keeps repainting.
    harness.run_steps(2);

    harness.get_by_label("Authenticate: done, 0.4 s");
    harness.get_by_label("Contact API server: in progress, 3 s");
    harness.get_by_label("Load namespaces: pending");
    harness.get_by_label("Open workspace: failed");
    harness.ui_harness("progress_checklist/every_step_state");
}
