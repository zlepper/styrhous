use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn usage_chart(
    ui: &mut egui::Ui,
    accessibility_label: &str,
    samples: Vec<(time::OffsetDateTime, i64)>,
    format: impl Fn(i64) -> String,
    color: egui::Color32,
    metrics_unavailable: bool,
    references: &UsageReferences,
) {
    let max_value = samples
        .iter()
        .map(|(_, value)| *value)
        .chain(references.iter().flatten().map(|reference| reference.value))
        .max()
        .unwrap_or(1)
        .max(1);
    let max = max_value as f32;
    let reference_summary = references
        .iter()
        .flatten()
        .map(|reference| format!("{} {}", reference.label, format(reference.value)))
        .collect::<Vec<_>>()
        .join(", ");
    let chart_summary = format!(
        "{accessibility_label}; {}; {} history; scale from 0 to {}; {}",
        if metrics_unavailable {
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
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), USAGE_CHART_HEIGHT),
        egui::Sense::hover(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Image, true, chart_summary.clone())
    });
    let status_message = if metrics_unavailable {
        "Unavailable"
    } else {
        "Collecting…"
    };
    if samples.len() < 2 && !has_usage_references(references) {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            status_message,
            typography::metadata(),
            gray::_500,
        );
        return;
    }
    let start = time::OffsetDateTime::now_utc() - POD_USAGE_HISTORY_WINDOW;
    let plot = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + USAGE_CHART_LEFT_INSET,
            rect.top() + USAGE_CHART_TOP_INSET,
        ),
        egui::pos2(
            rect.right() - USAGE_CHART_RIGHT_INSET,
            rect.bottom() - USAGE_CHART_BOTTOM_INSET,
        ),
    );
    draw_chart_axes(ui.painter(), plot, &format, max);
    for reference in references.iter().flatten() {
        let y = plot.bottom() - plot.height() * (reference.value as f32 / max);
        dashed_reference_line(
            ui.painter(),
            plot.left(),
            plot.right(),
            y,
            egui::Stroke::new(
                1.0,
                reference
                    .color
                    .gamma_multiply(USAGE_CHART_REFERENCE_OPACITY),
            ),
        );
    }
    let points = samples
        .iter()
        .map(|(timestamp, sample)| {
            let fraction = ((*timestamp - start).whole_seconds() as f32
                / POD_USAGE_HISTORY_WINDOW.whole_seconds() as f32)
                .clamp(0.0, 1.0);
            egui::pos2(
                egui::lerp(plot.left()..=plot.right(), fraction),
                plot.bottom() - plot.height() * (*sample as f32 / max),
            )
        })
        .collect::<Vec<_>>();
    if points.len() >= 2 {
        ui.painter().add(egui::Shape::mesh(usage_area_mesh(
            &points,
            plot.bottom(),
            color.gamma_multiply(USAGE_CHART_AREA_OPACITY),
        )));
        ui.painter().add(egui::Shape::line(
            points.clone(),
            egui::Stroke::new(1.7, color),
        ));
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            status_message,
            typography::metadata(),
            gray::_500,
        );
    }
    if let Some(pointer) = response
        .hover_pos()
        .filter(|pointer| plot.contains(*pointer))
        && let Some((timestamp, sample)) = points
            .iter()
            .zip(&samples)
            .min_by(|(left, _), (right, _)| {
                (pointer.x - left.x)
                    .abs()
                    .total_cmp(&(pointer.x - right.x).abs())
            })
            .map(|(_, sample)| *sample)
    {
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
        response.on_hover_text(tooltip);
    }
}

pub(super) fn usage_area_mesh(
    points: &[egui::Pos2],
    baseline: f32,
    color: egui::Color32,
) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let segment_count = points.len().saturating_sub(1);
    mesh.reserve_vertices(segment_count * 4);
    mesh.reserve_triangles(segment_count * 2);
    for segment in points.windows(2) {
        let base_index = mesh.vertices.len() as u32;
        let [start, end] = segment else {
            unreachable!("windows of two always contain two points")
        };
        mesh.colored_vertex(*start, color);
        mesh.colored_vertex(egui::pos2(start.x, baseline), color);
        mesh.colored_vertex(*end, color);
        mesh.colored_vertex(egui::pos2(end.x, baseline), color);
        mesh.add_triangle(base_index, base_index + 1, base_index + 2);
        mesh.add_triangle(base_index + 2, base_index + 1, base_index + 3);
    }
    mesh
}

#[derive(Clone, Copy)]
pub(super) struct UsageReference {
    label: &'static str,
    value: i64,
    color: egui::Color32,
}

type UsageReferences = [Option<UsageReference>; 2];

pub(super) fn usage_references(
    request: Option<i64>,
    limit: Option<i64>,
    labels: [&'static str; 2],
) -> UsageReferences {
    [
        request.map(|value| UsageReference {
            label: labels[0],
            value,
            color: status::WARNING,
        }),
        limit.map(|value| UsageReference {
            label: labels[1],
            value,
            color: status::CRITICAL,
        }),
    ]
}

pub(super) fn usage_chart_pair_labels(ui: &mut egui::Ui) {
    ui.columns(2, |columns| {
        columns[0].label(egui::RichText::new("CPU").color(gray::_500));
        columns[1].label(egui::RichText::new("Memory").color(gray::_500));
    });
}

pub(super) fn has_usage_references(references: &UsageReferences) -> bool {
    references.iter().any(Option::is_some)
}

pub(super) fn format_history_window() -> String {
    let seconds = POD_USAGE_HISTORY_WINDOW.whole_seconds();
    if seconds % 60 == 0 {
        format!("{}-minute", seconds / 60)
    } else {
        format!("{seconds}-second")
    }
}

pub(super) fn dashed_reference_line(
    painter: &egui::Painter,
    left: f32,
    right: f32,
    y: f32,
    stroke: egui::Stroke,
) {
    draw_dashed_horizontal_line(painter, left, right, y, stroke, 3.0, 3.0);
}

pub(super) fn draw_chart_axes(
    painter: &egui::Painter,
    plot: egui::Rect,
    format: &impl Fn(i64) -> String,
    max: f32,
) {
    let tick_color = gray::_300;
    dashed_grid_line(painter, plot.left(), plot.right(), plot.center().y);
    for fraction in [0.0, 1.0] {
        let y = egui::lerp(plot.bottom()..=plot.top(), fraction);
        painter.line_segment(
            [egui::pos2(plot.left() - 3.0, y), egui::pos2(plot.left(), y)],
            egui::Stroke::new(1.0, tick_color),
        );
        painter.line_segment(
            [
                egui::pos2(plot.right(), y),
                egui::pos2(plot.right() + 3.0, y),
            ],
            egui::Stroke::new(1.0, tick_color),
        );
        let value = (max * fraction).round() as i64;
        painter.text(
            egui::pos2(plot.left() - 6.0, y),
            egui::Align2::RIGHT_CENTER,
            if value == 0 {
                "0".to_owned()
            } else {
                format(value)
            },
            typography::chart_axis(),
            gray::_500,
        );
    }
    let time_labels = history_axis_labels();
    for (fraction, label, align) in [
        (0.0, time_labels[0].as_str(), egui::Align2::LEFT_TOP),
        (1.0, time_labels[1].as_str(), egui::Align2::RIGHT_TOP),
    ] {
        let x = egui::lerp(plot.left()..=plot.right(), fraction);
        painter.line_segment(
            [
                egui::pos2(x, plot.bottom()),
                egui::pos2(x, plot.bottom() + 3.0),
            ],
            egui::Stroke::new(1.0, tick_color),
        );
        painter.text(
            egui::pos2(x, plot.bottom() + 4.0),
            align,
            label,
            typography::chart_axis(),
            gray::_500,
        );
    }
}

pub(super) fn dashed_grid_line(painter: &egui::Painter, left: f32, right: f32, y: f32) {
    draw_dashed_horizontal_line(
        painter,
        left,
        right,
        y,
        egui::Stroke::new(1.0, gray::_200),
        2.0,
        2.0,
    );
}

pub(super) fn draw_dashed_horizontal_line(
    painter: &egui::Painter,
    left: f32,
    right: f32,
    y: f32,
    stroke: egui::Stroke,
    dash_length: f32,
    gap_length: f32,
) {
    let mut start = left;
    while start < right {
        let end = (start + dash_length).min(right);
        painter.line_segment([egui::pos2(start, y), egui::pos2(end, y)], stroke);
        start += dash_length + gap_length;
    }
}

pub(super) fn history_axis_labels() -> [String; 2] {
    let seconds = POD_USAGE_HISTORY_WINDOW.whole_seconds();
    let unit = if seconds % 60 == 0 { "m" } else { "s" };
    let amount = if unit == "m" { seconds / 60 } else { seconds };
    [format!("{amount}{unit} ago"), "now".to_owned()]
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
