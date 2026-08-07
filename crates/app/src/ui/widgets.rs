use crate::resource_table::{ContainerIndicator, StatusTone};
use components::colors::{SUCCESS, gray};
use components::{TableRowBuilder, TailwindButton, WorkspaceEmptyState};

const STATE_MARGIN: i8 = 32;

pub(super) fn display_resource_title(resource_name: &str) -> String {
    let mut characters = resource_name.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => "Resources".to_owned(),
    }
}

pub(super) fn resource_status(ui: &mut egui::Ui, status: &str, tone: StatusTone) {
    let color = status_color(tone);
    ui.horizontal(|ui| {
        ui.add_space(-13.0);
        let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(dot_rect.center(), 4.0, color);
        ui.add_space(5.0);
        TableRowBuilder::text(ui, status, false);
    });
}

pub(super) fn container_indicators(ui: &mut egui::Ui, indicators: &[ContainerIndicator]) {
    if indicators.is_empty() {
        TableRowBuilder::text(ui, "-", false);
        return;
    }

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        for indicator in indicators {
            let label = format!("{}: {}", indicator.kind.label(), indicator.name);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(2),
                status_color(indicator.tone),
            );
            response
                .on_hover_text(container_indicator_tooltip(indicator))
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Image, true, label.clone())
                });
        }
    });
}

fn container_indicator_tooltip(indicator: &ContainerIndicator) -> String {
    let mut lines = vec![
        format!("{}: {}", indicator.kind.label(), indicator.name),
        format!("State: {}", indicator.state),
        format!("Ready: {}", if indicator.ready { "Yes" } else { "No" }),
        format!("Restarts: {}", indicator.restart_count),
    ];
    if let Some(reason) = &indicator.reason {
        lines.push(format!("Reason: {reason}"));
    }
    if let Some(message) = &indicator.message {
        lines.push(format!("Message: {message}"));
    }
    lines.join("\n")
}

fn status_color(tone: StatusTone) -> egui::Color32 {
    match tone {
        StatusTone::Success => SUCCESS,
        StatusTone::Warning => egui::Color32::from_rgb(202, 138, 4),
        StatusTone::Danger => egui::Color32::from_rgb(220, 38, 38),
        StatusTone::Neutral => gray::_400,
    }
}

pub(super) fn workspace_empty_state(ui: &mut egui::Ui, title: &str, message: &str) {
    WorkspaceEmptyState::new(title, message).show(ui);
}

pub(super) fn workspace_search_error_state(ui: &mut egui::Ui, message: &str) {
    workspace_error_details(ui, "Invalid regular expression", message, 0.0, |_| ());
}

pub(super) fn workspace_loading_state(ui: &mut egui::Ui, title: &str, message: &str) {
    show_state_area(ui, 74.0, |ui| {
        ui.add(egui::Spinner::new().size(22.0));
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(title)
                .size(18.0)
                .strong()
                .color(gray::_800),
        );
        ui.add_space(6.0);
        ui.label(egui::RichText::new(message).size(13.0).color(gray::_500));
    });
}

pub(super) fn workspace_error_state(ui: &mut egui::Ui, title: &str, message: &str) -> bool {
    workspace_error_details(ui, title, message, 42.0, |ui| {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| TailwindButton::primary("Retry").show(ui).clicked())
            .inner
    })
}

fn workspace_error_details<R>(
    ui: &mut egui::Ui,
    title: &str,
    message: &str,
    footer_height: f32,
    show_footer: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let code_line_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let code_height = 16.0 + code_line_height * message.lines().count().max(1) as f32;
    let content_height = 22.0 + 6.0 + 16.0 + 4.0 + code_height + footer_height;

    show_state_area(ui, content_height, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .size(18.0)
                        .strong()
                        .color(gray::_800),
                );
            });
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Error details")
                    .size(13.0)
                    .color(gray::_500),
            );
            ui.add_space(4.0);
            let code_width = ui.available_width();
            let code_background = ui.visuals().code_bg_color;
            egui::Frame::new()
                .fill(code_background)
                .stroke(egui::Stroke::new(1.0, gray::_200))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_min_width((code_width - 16.0).max(0.0));
                    ui.code(message);
                });
            show_footer(ui)
        })
        .inner
    })
}

fn show_state_area<R>(
    ui: &mut egui::Ui,
    content_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(STATE_MARGIN))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), content_height),
                    egui::Layout::top_down(egui::Align::Center),
                    add_contents,
                )
                .inner
            })
            .inner
        })
        .inner
}
