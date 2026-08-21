use super::*;

pub(super) fn show_usage_value_grid(ui: &mut egui::Ui, usage: Option<(i64, i64)>) {
    let cells = match usage {
        Some((cpu, memory)) => [
            DetailCell::new("CPU", format_cpu(cpu)),
            DetailCell::new("Memory", format_memory(memory)),
        ],
        None => [
            DetailCell::unavailable("CPU"),
            DetailCell::unavailable("Memory"),
        ],
    };
    InspectorDetails::show_properties(ui, &[DetailRow::new(cells)]);
}

pub(super) fn displayed_usage_values(
    usage: Option<(i64, i64)>,
    metrics_error: Option<&str>,
) -> Option<(i64, i64)> {
    metrics_error.is_none().then_some(usage).flatten()
}

pub(super) fn show_metrics_api_unavailable(
    ui: &mut egui::Ui,
    requests: PodResourceThresholds,
    limits: PodResourceThresholds,
) {
    ui.label(
        egui::RichText::new("Metrics API unavailable")
            .font(typography::body())
            .color(gray::_700),
    );
    ui.label(
        egui::RichText::new("Live CPU and memory usage requires the Kubernetes Metrics API.")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(8.0);
    InspectorDetails::show_properties(
        ui,
        &[
            DetailRow::new([
                DetailCell::new(
                    "CPU request",
                    requests
                        .cpu_nanocores
                        .map(format_cpu)
                        .unwrap_or_else(|| "Not set".into()),
                ),
                DetailCell::new(
                    "CPU limit",
                    limits
                        .cpu_nanocores
                        .map(format_cpu)
                        .unwrap_or_else(|| "Not set".into()),
                ),
            ]),
            DetailRow::new([
                DetailCell::new(
                    "Memory request",
                    requests
                        .memory_bytes
                        .map(format_memory)
                        .unwrap_or_else(|| "Not set".into()),
                ),
                DetailCell::new(
                    "Memory limit",
                    limits
                        .memory_bytes
                        .map(format_memory)
                        .unwrap_or_else(|| "Not set".into()),
                ),
            ]),
        ],
    );
}

pub(super) fn show_node_metrics_api_unavailable(
    ui: &mut egui::Ui,
    allocatable: PodResourceThresholds,
) {
    ui.label(
        egui::RichText::new("Metrics API unavailable")
            .font(typography::body())
            .color(gray::_700),
    );
    ui.label(
        egui::RichText::new("Live CPU and memory usage requires the Kubernetes Metrics API.")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(8.0);
    InspectorDetails::show_properties(
        ui,
        &[DetailRow::new([
            DetailCell::new(
                "CPU allocatable",
                allocatable
                    .cpu_nanocores
                    .map(format_cpu)
                    .unwrap_or_else(|| "Not reported".into()),
            ),
            DetailCell::new(
                "Memory allocatable",
                allocatable
                    .memory_bytes
                    .map(format_memory)
                    .unwrap_or_else(|| "Not reported".into()),
            ),
        ])],
    );
}

pub(super) fn chip_row(ui: &mut egui::Ui, label: &str, values: &[String]) {
    ui.label(
        egui::RichText::new(label)
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.with_layout(
        egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true),
        |ui| {
            for value in values {
                let chip_width = (value.chars().count() as f32 * 6.7 + 28.0).clamp(54.0, 320.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(chip_width, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        egui::Frame::new()
                            .fill(gray::_100)
                            .stroke(egui::Stroke::new(1.0, gray::_200))
                            .corner_radius(radius::subtle())
                            .inner_margin(egui::Margin::symmetric((spacing::SM - 3.0) as i8, 0))
                            .show(ui, |ui| {
                                ui.set_max_width(chip_width - 10.0);
                                ui.horizontal_top(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(value)
                                                .monospace()
                                                .font(typography::monospace())
                                                .color(gray::_800),
                                        )
                                        .wrap(),
                                    );
                                    let (icon_rect, copy_action) = ui.allocate_exact_size(
                                        egui::vec2(14.0, 14.0),
                                        egui::Sense::click(),
                                    );
                                    copy_action.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            copy_action.enabled(),
                                            format!("Copy {label}"),
                                        )
                                    });
                                    if copy_action.hovered() {
                                        components::icons::document_duplicate_icon()
                                            .fit_to_exact_size(egui::vec2(14.0, 14.0))
                                            .tint(gray::_700)
                                            .paint_at(ui, icon_rect);
                                    }
                                    if copy_action.clicked() {
                                        ui.ctx().copy_text(value.clone());
                                    }
                                });
                            });
                    },
                );
            }
        },
    );
}

pub(super) fn volume_detail_row(
    ui: &mut egui::Ui,
    volume: &crate::resource_detail::PodVolumeDetail,
) {
    InspectorDetails::show_properties(
        ui,
        &[
            DetailRow::new([
                DetailCell::new("Type", volume.kind.as_str()),
                DetailCell::new("Source", volume.source.as_str()),
                DetailCell::new("Read-only", if volume.read_only { "true" } else { "false" }),
            ]),
            DetailRow::new([DetailCell::new(
                "Mount path",
                volume.mount_path.as_deref().unwrap_or("-"),
            )]),
        ],
    );
}

pub(super) fn error_card(ui: &mut egui::Ui, title: &str, error: &str) {
    ui.label(egui::RichText::new(title).strong().color(status::DANGER));
    ui.label(
        egui::RichText::new(error)
            .font(typography::metadata())
            .color(gray::_600),
    );
}
