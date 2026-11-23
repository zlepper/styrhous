use super::*;

pub(super) fn launcher_choice(
    ui: &mut egui::Ui,
    selected: bool,
    title: &str,
    description: &str,
    template: Option<&mut String>,
    template_invalid: bool,
    template_error: Option<&str>,
) -> bool {
    let theme = ui.tokens();
    let mut card = WorkspaceCard::new().margin(space::S32 as i8);
    if selected {
        card = card
            .fill(theme.item.selected)
            .stroke(stroke::hairline(theme.accent.fill));
    }
    card.show_inner(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let choice_clicked = ui
            .allocate_ui_with_layout(
                egui::vec2(ui.available_width(), CHOICE_CONTENT_MIN_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().icon_width = 20.0;
                    ui.spacing_mut().icon_width_inner = 12.0;
                    let response = ui.radio(selected, "").with_pointing_hand();
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::RadioButton,
                            ui.is_enabled(),
                            selected,
                            title,
                        )
                    });
                    ui.add_space(space::S8);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(title)
                                .font(type_scale::CARD_TITLE.font_id(ui.ctx()))
                                .color(theme.text.primary),
                        );
                        ui.add_space(space::S4);
                        ui.label(
                            egui::RichText::new(description)
                                .font(type_scale::BODY.font_id(ui.ctx()))
                                .color(theme.text.secondary),
                        );
                    });
                    response.clicked()
                },
            )
            .inner;
        if let Some(template) = template {
            ui.add_space(space::S32);
            ui.label(
                egui::RichText::new("Command template")
                    .font(type_scale::BODY.font_id(ui.ctx()))
                    .color(theme.text.primary),
            );
            ui.add_space(space::S8);
            show_command_template_input(ui, template, template_invalid);
            ui.add_space(space::S12);
            egui::Frame::new()
                .fill(theme.info.tint)
                .stroke(stroke::hairline(theme.info.outline))
                .corner_radius(egui::CornerRadius::same(radius::MENU_ITEM))
                .inner_margin(egui::Margin::same(space::S16 as i8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        let (icon_rect, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::hover());
                        ui.painter().circle_stroke(
                            icon_rect.center(),
                            9.0,
                            egui::Stroke::new(1.5, theme.info.text),
                        );
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "i",
                            type_scale::BODY_STRONG.font_id(ui.ctx()),
                            theme.info.text,
                        );
                        ui.add_space(space::S8);
                        ui.label(template_guidance(ui.ctx(), &theme));
                    });
                });
            if let Some(error) = template_error {
                ui.add_space(space::S16);
                show_validation_error(ui, "Command template needs attention", error);
            }
        }
        choice_clicked
    })
    .inner
}

pub(super) fn show_command_template_input(
    ui: &mut egui::Ui,
    template: &mut String,
    invalid: bool,
) -> egui::Response {
    let theme = ui.tokens();
    let id = ui.make_persistent_id("terminal-command-template");
    let focused = ui.memory(|memory| memory.has_focus(id));
    let stroke = if invalid {
        stroke::hairline(theme.danger.fill)
    } else if focused {
        stroke::hairline(theme.accent.fill)
    } else {
        stroke::control(&theme)
    };
    let response = egui::Frame::new()
        .fill(theme.components.field_bg)
        .stroke(stroke)
        .corner_radius(egui::CornerRadius::same(radius::KEY))
        .inner_margin(egui::Margin::symmetric(space::S12 as i8, space::S8 as i8))
        .show(ui, |ui| {
            let full_width = ui.available_width();
            ui.set_min_width(full_width);
            ui.add_sized(
                egui::vec2(full_width, 20.0),
                egui::TextEdit::singleline(template)
                    .id(id)
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 2)))
                    .font(type_scale::MONO.font_id(ui.ctx()))
                    .text_color(theme.text.primary)
                    .hint_text("alacritty -e {command}"),
            )
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::TextEdit,
            ui.is_enabled(),
            "Command template",
        )
    });
    response
}

pub(super) fn template_guidance(ctx: &egui::Context, theme: &Theme) -> egui::text::LayoutJob {
    let mut guidance = egui::text::LayoutJob::default();
    let text = egui::TextFormat {
        font_id: type_scale::BODY.font_id(ctx),
        color: theme.info.text_on_tint,
        ..Default::default()
    };
    guidance.append("Use ", 0.0, text.clone());
    guidance.append(
        "{command}",
        0.0,
        egui::TextFormat {
            font_id: type_scale::MONO.font_id(ctx),
            color: theme.info.text_on_tint,
            ..Default::default()
        },
    );
    guidance.append(
        " as the placeholder.\nIt is replaced with the complete, already quoted kubectl shell command,\nso do not wrap it in quotes.",
        0.0,
        text,
    );
    guidance
}

pub(super) fn show_validation_error(ui: &mut egui::Ui, title: &str, error: &str) {
    let theme = ui.tokens();
    egui::Frame::new()
        .fill(theme.danger.tint)
        .stroke(stroke::hairline(theme.danger.outline))
        .corner_radius(egui::CornerRadius::same(radius::KEY))
        .inner_margin(egui::Margin::same(space::S12 as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .font(type_scale::BODY_STRONG.font_id(ui.ctx()))
                    .color(theme.danger.text_on_tint),
            );
            ui.add_space(space::S4);
            ui.label(
                egui::RichText::new(error)
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(theme.text.secondary),
            );
        });
}
