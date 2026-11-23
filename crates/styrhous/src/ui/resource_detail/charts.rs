use super::*;

/// One resource of a [`usage_chart_pair`].
pub(super) struct UsageSeries {
    /// The chart's name for assistive technologies ("Pod CPU usage chart").
    pub(super) name: String,
    /// The usage history, oldest first.
    pub(super) samples: Vec<(time::OffsetDateTime, i64)>,
    /// The current usage, when known.
    pub(super) value: Option<i64>,
    pub(super) references: UsageReferences,
}

/// CPU and memory usage charts side by side, each with its current value,
/// its reference lines (request, limit or allocatable) and its history.
/// `stale` charts show history that metrics no longer update.
pub(super) fn usage_chart_pair(
    ui: &mut egui::Ui,
    cpu: UsageSeries,
    memory: UsageSeries,
    stale: bool,
) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = space::S24;
        ui.columns(2, |columns| {
            usage_chart(
                &mut columns[0],
                "CPU",
                components::UsageResource::Cpu,
                cpu,
                format_cpu,
                stale,
            );
            usage_chart(
                &mut columns[1],
                "Memory",
                components::UsageResource::Memory,
                memory,
                format_memory,
                stale,
            );
        });
    });
}

fn usage_chart(
    ui: &mut egui::Ui,
    label: &str,
    resource: components::UsageResource,
    series: UsageSeries,
    format: impl Fn(i64) -> String,
    stale: bool,
) {
    let samples = &series.samples;
    let references = &series.references;
    let max_value = samples
        .iter()
        .map(|(_, value)| *value)
        .chain(references.iter().flatten().map(|reference| reference.value))
        .max()
        .unwrap_or(1)
        .max(1);
    let reference_summary = references
        .iter()
        .flatten()
        .map(|reference| format!("{} {}", reference.label, format(reference.value)))
        .collect::<Vec<_>>()
        .join(", ");
    let chart_summary = format!(
        "{}; {}; {} history; scale from 0 to {}; {}",
        series.name,
        if stale {
            "metrics unavailable; displayed history may be stale"
        } else if samples.len() < 2 {
            "collecting samples"
        } else {
            "usage history available"
        },
        format_history_window(),
        format(max_value),
        if reference_summary.is_empty() {
            "no usage reference configured"
        } else {
            &reference_summary
        }
    );
    // Anchor the window to the newest collected sample. Metrics update on a fixed cadence, so
    // this prevents the plot from subtly drifting between samples while still showing its full
    // retained history.
    let end = samples
        .iter()
        .map(|(timestamp, _)| *timestamp)
        .max()
        .unwrap_or_else(time::OffsetDateTime::now_utc);
    let start = end - POD_USAGE_HISTORY_WINDOW;
    let points: Vec<[f32; 2]> = samples
        .iter()
        .map(|(timestamp, sample)| {
            let fraction = ((*timestamp - start).whole_seconds() as f32
                / POD_USAGE_HISTORY_WINDOW.whole_seconds() as f32)
                .clamp(0.0, 1.0);
            [fraction, *sample as f32]
        })
        .collect();
    let format_value = |value: f32| format(value.round() as i64);
    let tooltip = |index: usize| {
        let (timestamp, sample) = samples[index];
        let mut tooltip = format!(
            "{}\n{}",
            format(sample),
            timestamp
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default()
        );
        for reference in references.iter().flatten() {
            tooltip.push_str(&format!(
                "\n{}: {}",
                reference.label,
                format(reference.value)
            ));
        }
        tooltip
    };
    let value = series
        .value
        .map_or_else(|| "Unavailable".to_owned(), &format);
    let window = history_axis_start();
    let mut chart = components::UsageChart::new(label, resource, &value)
        .samples(&points)
        .format(&format_value)
        .tooltip(&tooltip)
        .accessibility_label(chart_summary)
        .stale(stale)
        .empty_text(if stale {
            "Unavailable"
        } else {
            "Collecting…"
        })
        .window(&window);
    let [request, limit] = *references;
    if let Some(request) = request {
        chart = chart.request(request.value as f32, request.label);
    }
    if let Some(limit) = limit {
        chart = chart.limit(limit.value as f32, limit.label);
    }
    chart.show(ui);
}

#[derive(Clone, Copy)]
pub(super) struct UsageReference {
    label: &'static str,
    value: i64,
}

pub(super) type UsageReferences = [Option<UsageReference>; 2];

pub(super) fn usage_references(
    request: Option<i64>,
    limit: Option<i64>,
    labels: [&'static str; 2],
) -> UsageReferences {
    [
        request.map(|value| UsageReference {
            label: labels[0],
            value,
        }),
        limit.map(|value| UsageReference {
            label: labels[1],
            value,
        }),
    ]
}

pub(super) fn has_usage_references(references: &UsageReferences) -> bool {
    references.iter().any(Option::is_some)
}

/// The usage history window as an amount and a unit: whole minutes when
/// it is a whole number of them, otherwise seconds.
fn history_window() -> (i64, bool) {
    let seconds = POD_USAGE_HISTORY_WINDOW.whole_seconds();
    if seconds % 60 == 0 {
        (seconds / 60, true)
    } else {
        (seconds, false)
    }
}

/// "10-minute", as in "10-minute history".
pub(super) fn format_history_window() -> String {
    match history_window() {
        (minutes, true) => format!("{minutes}-minute"),
        (seconds, false) => format!("{seconds}-second"),
    }
}

/// The start of a usage chart's time axis: "10 min ago".
fn history_axis_start() -> String {
    match history_window() {
        (minutes, true) => format!("{minutes} min ago"),
        (seconds, false) => format!("{seconds} s ago"),
    }
}

pub(super) fn total_resource_thresholds(
    containers: &[PodContainerDetail],
    thresholds: impl Fn(&PodContainerDetail) -> PodResourceThresholds,
) -> PodResourceThresholds {
    PodResourceThresholds {
        cpu_nanocores: sum_resource_quantities(
            containers
                .iter()
                .map(|container| thresholds(container).cpu_nanocores),
        ),
        memory_bytes: sum_resource_quantities(
            containers
                .iter()
                .map(|container| thresholds(container).memory_bytes),
        ),
    }
}

pub(super) fn sum_resource_quantities(values: impl Iterator<Item = Option<i64>>) -> Option<i64> {
    let mut found = false;
    let mut total = 0_i64;
    for value in values.flatten() {
        found = true;
        total = total.checked_add(value)?;
    }
    found.then_some(total)
}
