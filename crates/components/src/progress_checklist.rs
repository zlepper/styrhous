//! A titled list of steps that are pending, in progress, done or failed.

use egui::{Color32, Response, Sense, Stroke, Ui, Vec2};

use crate::design::theme::{Theme, ThemeExt};
use crate::design::{space, type_scale};

/// The default column width, enough for a step label and its duration.
const DEFAULT_WIDTH: f32 = 320.0;
const ROW_HEIGHT: f32 = space::S32;
const ICON_SIZE: f32 = space::S16;
const ICON_STROKE_WIDTH: f32 = 1.5;
const PENDING_DOT_RADIUS: f32 = 3.0;
const TITLE_GAP: f32 = space::S12;

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
        let title_font = type_scale::TITLE.font_id(ui.ctx());
        let title_height = ui.ctx().fonts_mut(|fonts| fonts.row_height(&title_font));
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
                        .font(type_scale::TITLE.font_id(ui.ctx()))
                        .color(ui.tokens().text.primary),
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
    let tokens = ui.tokens();
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
                Stroke::new(ICON_STROKE_WIDTH, tokens.control.border),
            );
        }
        ChecklistStepState::InProgress => {
            egui::Spinner::new()
                .size(ICON_SIZE)
                .color(tokens.accent.fill)
                .paint_at(ui, icon_rect);
        }
        ChecklistStepState::Done => paint_done(painter, icon_rect, tokens.success.fill),
        ChecklistStepState::Failed => paint_failed(painter, icon_rect, tokens.danger.fill),
    }

    let text_color = step_text_color(&tokens, step.state);
    painter.text(
        egui::pos2(icon_rect.right() + space::S12, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &step.label,
        type_scale::BODY.font_id(ui.ctx()),
        text_color,
    );
    if let Some(detail) = &step.detail {
        painter.text(
            egui::pos2(rect.right(), rect.center().y),
            egui::Align2::RIGHT_CENTER,
            detail,
            type_scale::META.font_id(ui.ctx()),
            tokens.text.muted,
        );
    }

    let description = match &step.detail {
        Some(detail) => format!("{}: {}, {detail}", step.label, step.state.description()),
        None => format!("{}: {}", step.label, step.state.description()),
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &description));
    response
}

fn step_text_color(theme: &Theme, state: ChecklistStepState) -> Color32 {
    match state {
        ChecklistStepState::Pending => theme.text.muted,
        ChecklistStepState::InProgress => theme.text.primary,
        ChecklistStepState::Done => theme.text.secondary,
        ChecklistStepState::Failed => theme.danger.text,
    }
}

fn paint_done(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    paint_status_circle(painter, rect, color);
    crate::icons::paint_check_mark(
        painter,
        rect.center(),
        ICON_SIZE * 0.5,
        Stroke::new(ICON_STROKE_WIDTH, color),
    );
}

fn paint_failed(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    paint_status_circle(painter, rect, color);
    let size = ICON_SIZE * 0.2;
    let center = rect.center();
    let stroke = Stroke::new(ICON_STROKE_WIDTH, color);
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
