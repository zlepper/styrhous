use super::*;

pub(super) fn show_pod_usage(
    ui: &mut egui::Ui,
    usage: PodUsageDisplay<'_>,
    requests: PodResourceThresholds,
    limits: PodResourceThresholds,
) {
    Card::new()
        .padding(egui::Margin::symmetric(
            space::S16 as i8,
            space::S12 as i8 + 2,
        ))
        .show(ui, |ui| {
            if usage.metrics_api_unavailable {
                show_metrics_api_unavailable(ui, requests, limits);
                return;
            }
            let displayed_usage = displayed_usage_values(
                usage
                    .usage
                    .map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
                usage.error,
            );
            show_usage(
                ui,
                "Pod",
                displayed_usage,
                usage
                    .history
                    .iter()
                    .map(|sample| (sample.timestamp, sample.cpu_nanocores, sample.memory_bytes))
                    .collect(),
                [
                    usage_references(
                        requests.cpu_nanocores,
                        limits.cpu_nanocores,
                        ["Request", "Limit"],
                    ),
                    usage_references(
                        requests.memory_bytes,
                        limits.memory_bytes,
                        ["Request", "Limit"],
                    ),
                ],
                usage.error.is_some(),
            );
        });
}

/// The current CPU and memory usage: as charts when there is history or a
/// reference line to draw, otherwise as plain values.
fn show_usage(
    ui: &mut egui::Ui,
    subject: &str,
    usage: Option<(i64, i64)>,
    history: Vec<(time::OffsetDateTime, i64, i64)>,
    [cpu_references, memory_references]: [UsageReferences; 2],
    stale: bool,
) {
    if history.is_empty()
        && !has_usage_references(&cpu_references)
        && !has_usage_references(&memory_references)
    {
        show_usage_value_grid(ui, usage);
        return;
    }
    usage_chart_pair(
        ui,
        UsageSeries {
            name: format!("{subject} CPU usage chart"),
            samples: history
                .iter()
                .map(|(timestamp, cpu, _)| (*timestamp, *cpu))
                .collect(),
            value: usage.map(|(cpu, _)| cpu),
            references: cpu_references,
        },
        UsageSeries {
            name: format!("{subject} memory usage chart"),
            samples: history
                .iter()
                .map(|(timestamp, _, memory)| (*timestamp, *memory))
                .collect(),
            value: usage.map(|(_, memory)| memory),
            references: memory_references,
        },
        stale,
    );
}

pub(super) fn show_node_usage(
    ui: &mut egui::Ui,
    usage: NodeUsageDisplay<'_>,
    allocatable: PodResourceThresholds,
) {
    Card::new()
        .padding(egui::Margin::symmetric(
            space::S16 as i8,
            space::S12 as i8 + 2,
        ))
        .show(ui, |ui| {
            if usage.metrics_api_unavailable {
                show_node_metrics_api_unavailable(ui);
                ui.add_space(10.0);
                show_node_pod_resources(
                    ui,
                    allocatable,
                    usage.pod_resources,
                    usage.pod_resources_error,
                );
                return;
            }
            show_node_pod_resources(
                ui,
                allocatable,
                usage.pod_resources,
                usage.pod_resources_error,
            );
            ui.add_space(10.0);
            show_usage(
                ui,
                "Node",
                displayed_usage_values(
                    usage
                        .usage
                        .map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
                    usage.error,
                ),
                usage
                    .history
                    .iter()
                    .map(|sample| (sample.timestamp, sample.cpu_nanocores, sample.memory_bytes))
                    .collect(),
                [
                    usage_references(allocatable.cpu_nanocores, None, ["Allocatable", ""]),
                    usage_references(allocatable.memory_bytes, None, ["Allocatable", ""]),
                ],
                usage.error.is_some(),
            );
        });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn show_container_usage(
    ui: &mut egui::Ui,
    name: &str,
    usage: Option<&ContainerUsage>,
    history: &[PodUsage],
    requests: PodResourceThresholds,
    limits: PodResourceThresholds,
    _missing: bool,
    metrics_api_unavailable: bool,
    error: Option<&str>,
) {
    ui.add_space(10.0);
    if metrics_api_unavailable {
        show_metrics_api_unavailable(ui, requests, limits);
        return;
    }
    show_usage(
        ui,
        name,
        displayed_usage_values(
            usage.map(|usage| (usage.cpu_nanocores, usage.memory_bytes)),
            error,
        ),
        history
            .iter()
            .filter_map(|sample| {
                sample
                    .containers
                    .get(name)
                    .map(|usage| (sample.timestamp, usage.cpu_nanocores, usage.memory_bytes))
            })
            .collect(),
        [
            usage_references(
                requests.cpu_nanocores,
                limits.cpu_nanocores,
                ["Request", "Limit"],
            ),
            usage_references(
                requests.memory_bytes,
                limits.memory_bytes,
                ["Request", "Limit"],
            ),
        ],
        error.is_some(),
    );
}
