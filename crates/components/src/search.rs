//! Compact search control shared by resource and log toolbars.

use crate::PointingHand;
use crate::colors::{WHITE, gray};
use crate::design::{radius, spacing, status, typography};
use egui::{Id, Margin, Response, Stroke, Ui, Vec2};

const INPUT_WIDTH: f32 = 150.0;
// The frame adds 4 px above and below plus a 1 px border on each edge, so the
// editor must remain 26 px tall to fit the 36 px toolbar slots used by callers.
const INPUT_HEIGHT: f32 = 26.0;

/// Responses from a [`TailwindSearchInput`].
pub struct SearchInputResponse {
    pub text: Response,
    pub regex: Response,
}

/// The compact search and regex-toggle control used in workspace toolbars.
pub struct TailwindSearchInput<'a> {
    query: &'a mut String,
    regex_mode: &'a mut bool,
    hint_text: String,
    input_id: Id,
    accessibility_label: String,
    invalid: bool,
}

impl<'a> TailwindSearchInput<'a> {
    pub fn new(query: &'a mut String, regex_mode: &'a mut bool) -> Self {
        Self {
            query,
            regex_mode,
            hint_text: "Search…".to_owned(),
            input_id: Id::NULL,
            accessibility_label: "Search".to_owned(),
            invalid: false,
        }
    }

    pub fn hint_text(mut self, hint_text: impl Into<String>) -> Self {
        self.hint_text = hint_text.into();
        self
    }

    pub fn id_salt(mut self, id_salt: impl std::hash::Hash) -> Self {
        self.input_id = Id::new(id_salt);
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = label.into();
        self
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn show(self, ui: &mut Ui) -> SearchInputResponse {
        let accessibility_label = self.accessibility_label;
        let stroke_color = if self.invalid {
            status::DANGER
        } else {
            gray::_300
        };
        egui::Frame::new()
            .fill(WHITE)
            .stroke(Stroke::new(1.0, stroke_color))
            .corner_radius(radius::control())
            .inner_margin(Margin::symmetric(spacing::SM as i8, spacing::XS as i8))
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let text = ui.add_sized(
                        Vec2::new(INPUT_WIDTH, INPUT_HEIGHT),
                        egui::TextEdit::singleline(self.query)
                            .hint_text(self.hint_text)
                            .frame(false)
                            .font(typography::body())
                            .id_salt(self.input_id),
                    );
                    text.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::TextEdit,
                            ui.is_enabled(),
                            accessibility_label.clone(),
                        )
                    });

                    ui.separator();
                    let regex = ui
                        .toggle_value(
                            self.regex_mode,
                            egui::RichText::new(".*").font(typography::body()),
                        )
                        .with_pointing_hand();
                    regex.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Checkbox,
                            ui.is_enabled(),
                            "Use regex search",
                        )
                    });
                    SearchInputResponse { text, regex }
                })
                .inner
            })
            .inner
    }
}
