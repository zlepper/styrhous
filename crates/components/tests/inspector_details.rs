use components::colors::WHITE;
use components::test_support::{UiHarnessSnapshot, setup_egui};
use components::{
    DetailCell, DetailColumn, DetailRow, DetailTableCell, DetailTableRow, DetailTone, DetailValue,
    InspectorDetails, WorkspaceCard,
};
use egui_kittest::{Harness, kittest::Queryable};

const UID: &str = "d1f2c3a4-b5e6-47f8-9a0b-1c2d3e4f5a6b";

fn properties() -> Vec<DetailRow<'static>> {
    vec![
        DetailRow::new([
            DetailCell::new("Namespace", "Cluster-wide"),
            DetailCell::new("Age", "132d"),
            DetailCell::status("Scheduling", "Schedulable", DetailTone::Success),
        ]),
        DetailRow::new([
            DetailCell::new("Provider ID", "kind://docker/kind/kind-control-plane").copyable(),
            DetailCell::new("Pod CIDRs", "10.244.0.0/24, fd00:10:244::/56"),
        ])
        .framed(),
        DetailRow::new([DetailCell::new("UID", UID).copyable()]).framed(),
        DetailRow::new([DetailCell::new(
            "Description",
            "This node runs the control-plane components and is reserved for cluster infrastructure workloads.",
        )])
        .framed(),
        DetailRow::new([
            DetailCell::unavailable("External ID"),
            DetailCell::new("Architecture", "amd64"),
        ])
        .framed(),
    ]
}

fn table_columns() -> Vec<DetailColumn<'static>> {
    vec![
        DetailColumn::new("Type"),
        DetailColumn::new("Reason"),
        DetailColumn::new("Message").weight(2.0),
        DetailColumn::new("Source"),
        DetailColumn::new("Time"),
    ]
}

fn table_rows() -> Vec<DetailTableRow<'static>> {
    vec![
        DetailTableRow::new([
            DetailValue::Status {
                text: "Normal".into(),
                tone: DetailTone::Success,
            },
            DetailValue::Text("Pulled".into()),
            DetailValue::Text("Successfully pulled image".into()),
            DetailValue::Text("kubelet".into()),
            DetailValue::Text("2m ago".into()),
        ]),
        DetailTableRow::new([
            DetailValue::Status {
                text: "Warning".into(),
                tone: DetailTone::Warning,
            },
            DetailValue::Text("BackOff".into()),
            DetailValue::Text("Back-off restarting failed container".into()),
            DetailValue::Text("kubelet".into()),
            DetailValue::Text("1m ago".into()),
        ]),
    ]
}

fn condition_columns() -> Vec<DetailColumn<'static>> {
    vec![
        DetailColumn::new("Type"),
        DetailColumn::new("Status"),
        DetailColumn::new("Reason").weight(1.5),
        DetailColumn::new("Message").weight(2.0),
    ]
}

fn condition_rows() -> Vec<DetailTableRow<'static>> {
    vec![
        DetailTableRow::new([
            DetailValue::Text("Ready".into()),
            DetailValue::Status {
                text: "True".into(),
                tone: DetailTone::Success,
            },
            DetailValue::Text("KubeletReady".into()),
            DetailValue::Text("kubelet is posting ready status".into()),
        ]),
        DetailTableRow::new([
            DetailValue::Text("MemoryPressure".into()),
            DetailValue::Status {
                text: "False".into(),
                tone: DetailTone::Neutral,
            },
            DetailValue::Text("KubeletHasSufficientMemory".into()),
            DetailValue::Text("kubelet has sufficient memory".into()),
        ]),
    ]
}

#[test]
fn inspector_details_showcase() {
    let mut harness = Harness::new_ui(|ui| {
        ui.painter().rect_filled(ui.max_rect(), 0.0, WHITE);
        ui.add_space(24.0);
        ui.horizontal_top(|ui| {
            ui.add_space(24.0);
            ui.allocate_ui_with_layout(
                egui::vec2(694.0, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    WorkspaceCard::new().show(ui, |ui| {
                        ui.heading("Properties");
                        ui.add_space(12.0);
                        InspectorDetails::show_properties(ui, &properties());
                    });
                },
            );
            ui.add_space(24.0);
            ui.allocate_ui_with_layout(
                egui::vec2(694.0, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    WorkspaceCard::new().show(ui, |ui| {
                        ui.heading("Events & conditions");
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new("Events").strong());
                        InspectorDetails::show_table(ui, &table_columns(), &table_rows());
                        ui.add_space(16.0);
                        ui.label(egui::RichText::new("Conditions").strong());
                        InspectorDetails::show_table(ui, &condition_columns(), &condition_rows());
                    });
                },
            );
        });
    });
    setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("inspector_details/showcase");
}

fn reflow_harness(width: f32) -> Harness<'static> {
    let mut harness = Harness::new_ui(move |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0.0, WHITE);
        ui.add_space(24.0);
        ui.add_space(24.0);
        ui.allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| InspectorDetails::show_properties(ui, &properties()),
        );
    });
    setup_egui(&mut harness);
    harness.run();
    harness
}

#[test]
fn inspector_details_reflows_to_two_columns_without_justifying_text() {
    let mut harness = reflow_harness(420.0);
    harness.ui_harness("inspector_details/medium_layout");
}

#[test]
fn inspector_details_reflows_to_one_column_without_justifying_text() {
    let mut harness = reflow_harness(280.0);
    harness.ui_harness("inspector_details/narrow_layout");
}

#[test]
fn inspector_details_copy_icon_is_revealed_by_hovering_its_value() {
    let mut harness = reflow_harness(420.0);
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Copy UID")
        .hover();
    harness.run();
    harness.ui_harness("inspector_details/copyable_value_hovered");
}

#[test]
fn inspector_details_copy_hover_preserves_narrow_value_layout() {
    let mut harness = reflow_harness(280.0);
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Copy UID")
        .hover();
    harness.run();
    harness.ui_harness("inspector_details/copyable_value_hovered_narrow");
}

#[test]
fn inspector_details_titled_group_keeps_its_label_in_the_border() {
    let mut harness = Harness::new_ui(|ui| {
        ui.painter().rect_filled(ui.max_rect(), 0.0, WHITE);
        ui.add_space(32.0);
        ui.add_space(32.0);
        ui.allocate_ui_with_layout(
            egui::vec2(420.0, 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                InspectorDetails::show_titled_properties(
                    ui,
                    "Spec",
                    &[DetailRow::new([
                        DetailCell::status("Scheduling", "Schedulable", DetailTone::Success),
                        DetailCell::new("Provider ID", "kind://docker/kind/kind-control-plane"),
                        DetailCell::new("Pod CIDRs", "10.244.0.0/24, fd00:10:244::/56"),
                        DetailCell::new("Taints", "None"),
                    ])],
                );
            },
        );
    });
    setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("inspector_details/titled_group");
}

#[test]
fn inspector_details_copy_button_emits_the_full_uid() {
    let mut harness = Harness::new_ui_state(
        |ui, copied| {
            ui.allocate_ui_with_layout(
                egui::vec2(180.0, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    copied.extend(
                        InspectorDetails::show_properties(
                            ui,
                            &[DetailRow::new([DetailCell::new("UID", UID).copyable()])],
                        )
                        .copied,
                    );
                },
            );
        },
        Vec::<String>::new(),
    );
    setup_egui(&mut harness);
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Copy UID")
        .click();
    harness.step();

    assert_eq!(harness.state(), &[UID.to_owned()]);
    assert!(
        harness
            .output()
            .platform_output
            .commands
            .iter()
            .any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == UID))
    );
}

#[test]
fn inspector_details_table_copy_button_emits_the_full_value() {
    let mut harness = Harness::new_ui_state(
        |ui, copied| {
            copied.extend(
                InspectorDetails::show_table(
                    ui,
                    &[DetailColumn::new("Message")],
                    &[DetailTableRow::new([DetailTableCell::new(
                        DetailValue::Text("probe failed: connection refused".into()),
                    )
                    .copyable()])],
                )
                .copied,
            );
        },
        Vec::<String>::new(),
    );

    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Copy Message")
        .click();
    harness.step();
    assert_eq!(
        harness.state(),
        &["probe failed: connection refused".to_owned()]
    );
}

#[test]
fn inspector_details_tables_wrap_at_narrow_width() {
    let mut harness = Harness::new_ui(|ui| {
        ui.painter().rect_filled(ui.max_rect(), 0.0, WHITE);
        ui.add_space(24.0);
        ui.add_space(24.0);
        ui.allocate_ui_with_layout(
            egui::vec2(280.0, 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| InspectorDetails::show_table(ui, &table_columns(), &table_rows()),
        );
    });
    setup_egui(&mut harness);
    harness.run();
    harness.ui_harness("inspector_details/narrow_table_layout");
}
