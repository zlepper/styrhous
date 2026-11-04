//! Circular text avatars shared by navigation, settings, and previews.

use crate::colors::{gray, indigo};
use crate::design::typography;
use egui::{Align2, Color32, FontId, Pos2, Response, Sense, Ui, Vec2};

pub struct TextAvatar<'a> {
    text: &'a str,
    size: f32,
    fill: Color32,
    text_color: Color32,
    font: FontId,
}

impl<'a> TextAvatar<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            size: 28.0,
            fill: gray::_200,
            text_color: gray::_600,
            font: typography::body(),
        }
    }

    pub fn dark(mut self) -> Self {
        self.fill = gray::_700;
        self.text_color = gray::_100;
        self
    }

    pub fn accent(mut self) -> Self {
        self.fill = indigo::_50;
        self.text_color = crate::design::button::PRIMARY;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
    pub fn font(mut self, font: FontId) -> Self {
        self.font = font;
        self
    }

    pub fn paint(&self, ui: &Ui, center: Pos2) {
        let mut font = self.font.clone();
        let width = ui
            .painter()
            .layout_no_wrap(self.text.to_owned(), font.clone(), self.text_color)
            .size()
            .x;
        // Wide pairs such as WW must fit within the same circle as single initials.
        let available_width = self.size * 0.75;
        if width > available_width {
            font.size *= available_width / width;
        }
        ui.painter()
            .circle_filled(center, self.size / 2.0, self.fill);
        ui.painter().text(
            center,
            Align2::CENTER_CENTER,
            self.text,
            font,
            self.text_color,
        );
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), Sense::hover());
        self.paint(ui, rect.center());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), self.text)
        });
        response
    }
}
