//! Usage charts (C13): a resource's CPU or memory use over the last ten
//! minutes against its limit and request.
//!
//! A chart is a header (caption and the limit), the current value at 16/24
//! semibold, a [`size::CHART_PLOT`] plot and a time axis. The limit is a
//! dashed `chart-limit` line at the top of the plot and the request a
//! dashed `chart-request` line at its height; the series is a line over an
//! area at 16 % of its colour. Hovering the plot shows a vertical guide and
//! the nearest sample's time and value.
//!
//! When metrics are unavailable the chart shows "—" and "Metrics
//! unavailable" (with the reason as a tooltip) and no axes.

use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::{Align, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

use crate::design::theme::ThemeExt;
use crate::design::type_scale::{self, TextRole, Weight};
use crate::design::{size, space};
use crate::tooltip::{TooltipContent, TooltipExt};

/// The current value (`.w6.tnum`, 16/24 · 600).
const VALUE: TextRole = type_scale::UI
    .with_size(16.0, 24.0)
    .with_weight(Weight::SemiBold)
    .tabular();
/// Gap between the rows of a chart.
const ROW_GAP: f32 = space::S4;
/// Opacity of the area under the series (`fill-opacity: .16`).
const AREA_OPACITY: f32 = 0.16;
/// The time window a chart covers, in minutes.
const WINDOW_MINUTES: f32 = 10.0;
/// What the value reads when there are no metrics.
const NO_VALUE: &str = "—";
/// The start of the time axis by default.
const DEFAULT_WINDOW: &str = "10 min ago";

/// Which resource a chart plots, choosing its `chart-*` colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageResource {
    /// CPU (`chart-cpu`).
    Cpu,
    /// Memory (`chart-memory`).
    Memory,
}

/// A reference line on a usage chart: a limit, a request or what a node
/// can allocate.
#[derive(Clone, Copy, Debug)]
struct Reference<'a> {
    value: f32,
    name: &'a str,
}

/// A usage chart filling the available width.
#[must_use = "call `show` to add the chart"]
pub struct UsageChart<'a> {
    label: &'a str,
    resource: UsageResource,
    value: &'a str,
    limit: Option<Reference<'a>>,
    request: Option<Reference<'a>>,
    samples: &'a [[f32; 2]],
    unavailable: Option<&'a str>,
    empty_text: &'a str,
    stale: bool,
    window: &'a str,
    format: Option<&'a dyn Fn(f32) -> String>,
    tooltip: Option<&'a dyn Fn(usize) -> String>,
    accessibility_label: Option<String>,
}

impl<'a> UsageChart<'a> {
    /// A chart captioned `label` reading `value` now.
    pub fn new(label: &'a str, resource: UsageResource, value: &'a str) -> Self {
        Self {
            label,
            resource,
            value,
            limit: None,
            request: None,
            samples: &[],
            unavailable: None,
            empty_text: "Collecting…",
            stale: false,
            window: DEFAULT_WINDOW,
            format: None,
            tooltip: None,
            accessibility_label: None,
        }
    }

    /// The samples, oldest first: `[t, value]` where `t` runs from 0 (the
    /// start of the window) to 1 (now) and `value` is in the chart's unit.
    pub fn samples(mut self, samples: &'a [[f32; 2]]) -> Self {
        self.samples = samples;
        self
    }

    /// The limit, a dashed `chart-limit` line, named `name` ("Limit") in the
    /// header caption ("limit 100m") and the hover tooltip.
    pub fn limit(mut self, value: f32, name: &'a str) -> Self {
        self.limit = Some(Reference { value, name });
        self
    }

    /// The request (or what a node can allocate), a dashed `chart-request`
    /// line named `name` ("Request", "Allocatable").
    pub fn request(mut self, value: f32, name: &'a str) -> Self {
        self.request = Some(Reference { value, name });
        self
    }

    /// How values read in the caption and the hover tooltip ("15m").
    pub fn format(mut self, format: &'a dyn Fn(f32) -> String) -> Self {
        self.format = Some(format);
        self
    }

    /// The hover tooltip of the sample at an index, instead of "5 min ago ·
    /// 15m" (for example with its exact time).
    pub fn tooltip(mut self, tooltip: &'a dyn Fn(usize) -> String) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    /// What the plot says while it has fewer than two samples (default
    /// "Collecting…").
    pub fn empty_text(mut self, text: &'a str) -> Self {
        self.empty_text = text;
        self
    }

    /// Draw the series in `status-neutral`: the samples are history that
    /// is no longer being updated.
    pub fn stale(mut self, stale: bool) -> Self {
        self.stale = stale;
        self
    }

    /// The start of the time axis (default "10 min ago").
    pub fn window(mut self, window: &'a str) -> Self {
        self.window = window;
        self
    }

    /// The chart's accessible description, instead of "CPU over the last
    /// 10 minutes".
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Show that metrics are unavailable, for `reason`: "—" and "Metrics
    /// unavailable" with the reason as a tooltip, and no axes.
    pub fn unavailable(mut self, reason: &'a str) -> Self {
        self.unavailable = Some(reason);
        self
    }

    fn format_value(&self, value: f32) -> String {
        self.format
            .map_or_else(|| value.to_string(), |format| format(value))
    }

    /// The header caption: the reference lines ("request 50m · limit 100m"),
    /// or the top of the scale when there are none ("max 120m"), so the
    /// plot's scale is always readable.
    fn caption(&self) -> Option<String> {
        let references: Vec<String> = [self.request, self.limit]
            .into_iter()
            .flatten()
            .map(|reference| {
                format!(
                    "{} {}",
                    reference.name.to_lowercase(),
                    self.format_value(reference.value)
                )
            })
            .collect();
        if !references.is_empty() {
            Some(references.join(" · "))
        } else if self.samples.len() >= 2 {
            Some(format!("max {}", self.format_value(self.scale())))
        } else {
            None
        }
    }

    /// The value at the top of the plot: the largest of the samples and
    /// the reference lines, so none of them is cut off.
    fn scale(&self) -> f32 {
        let largest = self
            .samples
            .iter()
            .map(|[_, value]| *value)
            .chain(self.limit.map(|reference| reference.value))
            .chain(self.request.map(|reference| reference.value))
            .fold(0.0_f32, f32::max);
        if largest > 0.0 { largest } else { 1.0 }
    }

    /// The hover tooltip of sample `index`.
    fn sample_tooltip(&self, index: usize) -> String {
        if let Some(tooltip) = self.tooltip {
            return tooltip(index);
        }
        let [t, value] = self.samples[index];
        let minutes = ((1.0 - t) * WINDOW_MINUTES).round();
        let when = if minutes < 1.0 {
            "now".to_owned()
        } else {
            format!("{minutes} min ago")
        };
        format!("{when} · {}", self.format_value(value))
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            self.show_rows(ui)
        })
        .inner
    }

    fn show_rows(self, ui: &mut Ui) -> Response {
        let theme = ui.tokens();
        let width = ui.available_width();
        let muted = theme.text.muted;

        // Header: caption and limit on one 16 point row.
        let header_height = type_scale::CAPTION.line_height();
        let (header, _) = ui.allocate_exact_size(vec2(width, header_height), Sense::hover());
        let label_width = row_text(
            ui,
            type_scale::CAPTION,
            self.label,
            header,
            Align::Min,
            width,
        );
        if let Some(caption) = self.caption().filter(|_| self.unavailable.is_none()) {
            // The caption gives way to the label in a narrow chart.
            let room = width - label_width - space::S8;
            row_text(ui, type_scale::META, &caption, header, Align::Max, room);
        }
        ui.add_space(ROW_GAP);

        if let Some(reason) = self.unavailable {
            VALUE.label(ui, NO_VALUE, theme.text.primary);
            ui.add_space(ROW_GAP);
            return type_scale::META
                .label(ui, "Metrics unavailable", muted)
                .tooltip(reason);
        }

        VALUE.label(ui, self.value, theme.text.primary);
        ui.add_space(ROW_GAP);

        let (plot, response) =
            ui.allocate_exact_size(vec2(width, size::CHART_PLOT), Sense::hover());
        let name = self
            .accessibility_label
            .clone()
            .unwrap_or_else(|| format!("{} over the last 10 minutes", self.label));
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, &name));
        let color = match (self.stale, self.resource) {
            (true, _) => theme.status.neutral,
            (false, UsageResource::Cpu) => theme.components.chart_cpu,
            (false, UsageResource::Memory) => theme.components.chart_memory,
        };
        let geometry = PlotGeometry::new(plot, self.scale());
        let points: Vec<Pos2> = self
            .samples
            .iter()
            .map(|&[t, value]| geometry.point(t, value))
            .collect();
        if ui.is_rect_visible(plot) {
            let painter = ui.painter();
            for (reference, color) in [
                (self.limit, theme.components.chart_limit),
                (self.request, theme.components.chart_request),
            ] {
                if let Some(reference) = reference {
                    dashed_line(painter, plot, geometry.y(reference.value), color);
                }
            }
            let baseline = geometry.baseline;
            hairline(
                painter,
                plot.left(),
                plot.right(),
                baseline,
                theme.components.chart_grid,
            );
            if points.len() >= 2 {
                painter.add(area(&points, baseline, color.gamma_multiply(AREA_OPACITY)));
                painter.add(PathShape::line(
                    points.clone(),
                    PathStroke::new(size::CHART_LINE, color),
                ));
            } else {
                let galley =
                    type_scale::META.galley(ui.ctx(), self.empty_text, muted, f32::INFINITY);
                let pos = plot.center() - galley.rect.size() / 2.0;
                type_scale::paint_galley(painter, pos, galley, muted);
            }
        }
        let response = match response
            .hover_pos()
            .and_then(|pointer| nearest(&points, pointer.x))
        {
            Some(index) if points.len() >= 2 => {
                ui.painter().vline(
                    points[index].x,
                    plot.y_range(),
                    Stroke::new(size::HAIRLINE, theme.divider.strong),
                );
                response.show_tooltip(TooltipContent::Text(&self.sample_tooltip(index)));
                response
            }
            _ => response,
        };
        ui.add_space(ROW_GAP);

        let (axis, _) =
            ui.allocate_exact_size(vec2(width, type_scale::META.line_height()), Sense::hover());
        let now_width = row_text(ui, type_scale::META, "now", axis, Align::Max, width);
        row_text(
            ui,
            type_scale::META,
            self.window,
            axis,
            Align::Min,
            width - now_width - space::S8,
        );
        response
    }
}

/// Maps samples into a plot rect.
struct PlotGeometry {
    plot: Rect,
    top: f32,
    baseline: f32,
    scale: f32,
}

impl PlotGeometry {
    fn new(plot: Rect, scale: f32) -> Self {
        Self {
            plot,
            top: plot.top() + size::CHART_LIMIT_INSET,
            baseline: plot.bottom() - size::CHART_BASELINE_INSET,
            scale,
        }
    }

    fn y(&self, value: f32) -> f32 {
        let fraction = (value / self.scale).clamp(0.0, 1.0);
        self.baseline - fraction * (self.baseline - self.top)
    }

    fn point(&self, t: f32, value: f32) -> Pos2 {
        pos2(
            self.plot.left() + t.clamp(0.0, 1.0) * self.plot.width(),
            self.y(value),
        )
    }
}

/// One muted line of `role` at the left (`Align::Min`) or right end of
/// `row`, in space already allocated, elided to `max_width` with the full
/// text as a tooltip; returns its width.
fn row_text(ui: &Ui, role: TextRole, text: &str, row: Rect, align: Align, max_width: f32) -> f32 {
    let color = ui.tokens().text.muted;
    let galley = role.galley_elided(ui.ctx(), text, color, max_width.max(0.0), 1);
    let width = galley.rect.width();
    let elided = galley.elided;
    let x = match align {
        Align::Max => row.right() - width,
        _ => row.left(),
    };
    let id = ui.id().with(("usage-chart", text));
    let response = type_scale::text_at(ui, id, pos2(x, row.top()), galley, text, color);
    if elided {
        response.tooltip(text);
    }
    width
}

/// A horizontal hairline from `left` to `right` centred on `y`, unsnapped:
/// a chart line between two device pixels covers half of each, as in an
/// SVG plot. (egui draws thin rectangles as lines snapped to pixel centres,
/// so each device-pixel row is painted with its coverage.)
fn hairline(painter: &egui::Painter, left: f32, right: f32, y: f32, color: Color32) {
    let pixels_per_point = painter.pixels_per_point();
    let half = size::HAIRLINE / 2.0;
    let top = (y - half) * pixels_per_point;
    let bottom = (y + half) * pixels_per_point;
    let mut mesh = Mesh::default();
    let mut row = top.floor();
    while row < bottom {
        let coverage = bottom.min(row + 1.0) - top.max(row);
        let rect = Rect::from_x_y_ranges(
            left..=right,
            row / pixels_per_point..=(row + 1.0) / pixels_per_point,
        );
        mesh.add_colored_rect(rect, color.gamma_multiply(coverage));
        row += 1.0;
    }
    painter.add(mesh);
}

/// A horizontal dashed line across `plot` at `y`. The dashes stretch with
/// the plot: [`size::CHART_DASH`] at [`size::CHART_DASH_SCALE_WIDTH`].
fn dashed_line(painter: &egui::Painter, plot: Rect, y: f32, color: Color32) {
    let dash = size::CHART_DASH * plot.width() / size::CHART_DASH_SCALE_WIDTH;
    let mut left = plot.left();
    while left < plot.right() {
        hairline(painter, left, (left + dash).min(plot.right()), y, color);
        left += 2.0 * dash;
    }
}

/// The area between `points` and `baseline`, one quad per segment.
fn area(points: &[Pos2], baseline: f32, color: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    for pair in points.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        let first = mesh.vertices.len() as u32;
        for pos in [a, b, pos2(b.x, baseline), pos2(a.x, baseline)] {
            mesh.colored_vertex(pos, color);
        }
        mesh.add_triangle(first, first + 1, first + 2);
        mesh.add_triangle(first, first + 2, first + 3);
    }
    mesh
}

/// The index of the point nearest `x`.
fn nearest(points: &[Pos2], x: f32) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_scale_between_the_baseline_and_the_limit() {
        let plot = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, size::CHART_PLOT));
        let geometry = PlotGeometry::new(plot, 100.0);
        assert_eq!(geometry.y(100.0), size::CHART_LIMIT_INSET);
        assert_eq!(
            geometry.y(0.0),
            size::CHART_PLOT - size::CHART_BASELINE_INSET
        );
        assert_eq!(
            geometry.y(250.0),
            size::CHART_LIMIT_INSET,
            "clamped to the limit"
        );
        assert_eq!(geometry.point(1.0, 50.0).x, 300.0);
    }

    #[test]
    fn the_plot_scales_to_its_largest_sample_or_reference() {
        let samples = [[0.0, 10.0], [1.0, 20.0]];
        let chart = UsageChart::new("CPU", UsageResource::Cpu, "20m").samples(&samples);
        assert_eq!(chart.scale(), 20.0);
        let chart = chart.limit(100.0, "Limit");
        assert_eq!(chart.scale(), 100.0);
        let over = [[0.0, 120.0], [1.0, 20.0]];
        assert_eq!(
            chart.samples(&over).scale(),
            120.0,
            "a sample past the limit stays visible"
        );
    }

    #[test]
    fn the_caption_names_the_references_else_the_scale() {
        let format = |value: f32| format!("{value}m");
        let chart = UsageChart::new("CPU", UsageResource::Cpu, "20m").format(&format);
        assert_eq!(chart.caption(), None);
        let chart = chart.request(50.0, "Allocatable");
        assert_eq!(chart.caption().as_deref(), Some("allocatable 50m"));
        let chart = chart.limit(100.0, "Limit");
        assert_eq!(
            chart.caption().as_deref(),
            Some("allocatable 50m · limit 100m")
        );
        let samples = [[0.0, 10.0], [1.0, 20.0]];
        let chart = UsageChart::new("CPU", UsageResource::Cpu, "20m")
            .format(&format)
            .samples(&samples);
        assert_eq!(chart.caption().as_deref(), Some("max 20m"));
    }

    #[test]
    fn the_area_tessellates_each_non_monotonic_segment_on_its_own() {
        let points = [
            pos2(0.0, 6.0),
            pos2(10.0, 30.0),
            pos2(20.0, 8.0),
            pos2(30.0, 24.0),
        ];
        let baseline = 40.0;
        let mesh = area(
            &points,
            baseline,
            crate::design::theme::Theme::default().components.chart_cpu,
        );
        assert_eq!(mesh.vertices.len(), 4 * (points.len() - 1));
        assert_eq!(mesh.indices.len(), 6 * (points.len() - 1));
        for (segment, vertices) in points.windows(2).zip(mesh.vertices.chunks_exact(4)) {
            assert_eq!(vertices[0].pos, segment[0]);
            assert_eq!(vertices[1].pos, segment[1]);
            assert_eq!(vertices[2].pos, pos2(segment[1].x, baseline));
            assert_eq!(vertices[3].pos, pos2(segment[0].x, baseline));
        }
    }

    #[test]
    fn hovering_the_plot_shows_the_nearest_sample() {
        use egui_kittest::kittest::Queryable;
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            ui.set_width(300.0);
            let samples = [[0.0, 20.0], [0.5, 45.0], [1.0, 30.0]];
            let format = |value: f32| format!("{value}m");
            UsageChart::new("CPU", UsageResource::Cpu, "30m")
                .samples(&samples)
                .format(&format)
                .show(ui);
        });
        crate::test_support::setup_egui(&mut harness);
        harness.run();
        let plot = harness.get_by_label("CPU over the last 10 minutes").rect();
        harness.hover_at(pos2(plot.center().x + 2.0, plot.center().y));
        harness.run();
        assert!(harness.query_by_label("5 min ago · 45m").is_some());
        harness.hover_at(pos2(plot.right() - 1.0, plot.center().y));
        harness.run();
        assert!(harness.query_by_label("now · 30m").is_some());
    }
}
