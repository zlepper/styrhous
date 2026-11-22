//! A titled list of steps that are pending, in progress, done or failed.

use egui::{Color32, Response, Sense, Stroke, Ui, Vec2};

use crate::colors::gray;
use crate::design::{spacing, status, typography};

/// The default column width, enough for a step label and its duration.
const DEFAULT_WIDTH: f32 = 320.0;
const ROW_HEIGHT: f32 = spacing::XXL;
const ICON_SIZE: f32 = spacing::LG;
const ICON_STROKE_WIDTH: f32 = 1.5;
const PENDING_DOT_RADIUS: f32 = 3.0;
const TITLE_GAP: f32 = spacing::MD;

/// How far one step has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecklistStepState {
    Pending,
    InProgress,
    Done,
    Failed,
}

impl ChecklistStepState {
    fn description(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in progress",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

/// One row: its label, state and an optional trailing detail such as a duration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistStep {
    pub label: String,
    pub state: ChecklistStepState,
    pub detail: Option<String>,
}

impl ChecklistStep {
    pub fn new(
        label: impl Into<String>,
        state: ChecklistStepState,
        detail: Option<String>,
    ) -> Self {
        Self {
            label: label.into(),
            state,
            detail,
        }
    }
}

/// A titled checklist of steps, for showing progress through work that happens in stages.
pub struct ProgressChecklist<'a> {
    title: &'a str,
    steps: &'a [ChecklistStep],
    width: f32,
}

impl<'a> ProgressChecklist<'a> {
    pub fn new(title: &'a str, steps: &'a [ChecklistStep]) -> Self {
        Self {
            title,
            steps,
            width: DEFAULT_WIDTH,
        }
    }

    /// The width of the column the title and steps share; durations align to its right.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// The height `show` allocates, so callers can center it.
    pub fn height(&self, ui: &Ui) -> f32 {
        let title_height = ui
            .ctx()
            .fonts_mut(|fonts| fonts.row_height(&typography::section_heading()));
        title_height + TITLE_GAP + ROW_HEIGHT * self.steps.len() as f32
    }

    pub fn show(self, ui: &mut Ui) {
        let height = self.height(ui);
        ui.allocate_ui_with_layout(
            Vec2::new(self.width, height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_width(self.width);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(
                    egui::RichText::new(self.title)
                        .font(typography::section_heading())
                        .color(gray::_800),
                );
                ui.add_space(TITLE_GAP);
                for step in self.steps {
                    show_step(ui, step, self.width);
                }
            },
        );
    }
}

fn show_step(ui: &mut Ui, step: &ChecklistStep, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::hover());
    let painter = ui.painter();
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + ICON_SIZE / 2.0, rect.center().y),
        Vec2::splat(ICON_SIZE),
    );
    match step.state {
        ChecklistStepState::Pending => {
            painter.circle_stroke(
                icon_rect.center(),
                PENDING_DOT_RADIUS,
                Stroke::new(ICON_STROKE_WIDTH, gray::_300),
            );
        }
        ChecklistStepState::InProgress => {
            egui::Spinner::new()
                .size(ICON_SIZE)
                .color(gray::_500)
                .paint_at(ui, icon_rect);
        }
        ChecklistStepState::Done => paint_done(painter, icon_rect),
        ChecklistStepState::Failed => paint_failed(painter, icon_rect),
    }

    let text_color = match step.state {
        ChecklistStepState::Pending => gray::_400,
        ChecklistStepState::InProgress => gray::_800,
        ChecklistStepState::Done => gray::_600,
        ChecklistStepState::Failed => status::DANGER,
    };
    painter.text(
        egui::pos2(icon_rect.right() + spacing::MD, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &step.label,
        typography::body(),
        text_color,
    );
    if let Some(detail) = &step.detail {
        painter.text(
            egui::pos2(rect.right(), rect.center().y),
            egui::Align2::RIGHT_CENTER,
            detail,
            typography::metadata(),
            gray::_500,
        );
    }

    let description = match &step.detail {
        Some(detail) => format!("{}: {}, {detail}", step.label, step.state.description()),
        None => format!("{}: {}", step.label, step.state.description()),
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &description));
    response
}

fn paint_done(painter: &egui::Painter, rect: egui::Rect) {
    paint_status_circle(painter, rect, status::SUCCESS);
    crate::icons::paint_check_mark(
        painter,
        rect.center(),
        ICON_SIZE * 0.5,
        Stroke::new(ICON_STROKE_WIDTH, status::SUCCESS),
    );
}

fn paint_failed(painter: &egui::Painter, rect: egui::Rect) {
    paint_status_circle(painter, rect, status::DANGER);
    let size = ICON_SIZE * 0.2;
    let center = rect.center();
    let stroke = Stroke::new(ICON_STROKE_WIDTH, status::DANGER);
    for direction in [egui::vec2(1.0, 1.0), egui::vec2(1.0, -1.0)] {
        painter.line_segment(
            [center - direction * size, center + direction * size],
            stroke,
        );
    }
}

fn paint_status_circle(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    painter.circle_stroke(
        rect.center(),
        (ICON_SIZE - ICON_STROKE_WIDTH) / 2.0,
        Stroke::new(ICON_STROKE_WIDTH, color),
    );
}
