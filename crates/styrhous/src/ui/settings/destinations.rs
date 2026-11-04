use super::*;

pub(super) struct SettingsDestination<'a> {
    pub(super) title: &'a str,
    pub(super) label: &'a str,
    pub(super) description: &'a str,
    pub(super) icon: egui::Image<'static>,
    pub(super) detail: &'a str,
}

pub(super) fn settings_destination_group<R>(
    ui: &mut egui::Ui,
    heading: &str,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.label(
        egui::RichText::new(heading)
            .font(typography::semibold(typography::BODY_SIZE))
            .color(gray::_500),
    );
    ui.add_space(spacing::MD);
    let frame = egui::Frame::new()
        .stroke(surface::muted_border())
        .corner_radius(radius::surface())
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            contents(ui)
        });
    ui.painter().rect_stroke(
        frame.response.rect,
        radius::surface(),
        surface::muted_border(),
        egui::StrokeKind::Inside,
    );
    frame.inner
}

pub(super) fn settings_destination_divider(ui: &mut egui::Ui) {
    let y = ui.next_widget_position().y - 0.5;
    ui.painter()
        .hline(ui.max_rect().x_range(), y, surface::muted_border());
}

pub(super) fn settings_destination_card(
    ui: &mut egui::Ui,
    destination: SettingsDestination<'_>,
) -> bool {
    let saved_widgets = ui.visuals().widgets.clone();
    let saved_button_padding = ui.spacing().button_padding;
    let visuals = &mut ui.visuals_mut().widgets;
    for visual in [
        &mut visuals.inactive,
        &mut visuals.hovered,
        &mut visuals.active,
    ] {
        visual.bg_stroke = egui::Stroke::NONE;
        visual.corner_radius = egui::CornerRadius::ZERO;
    }
    visuals.inactive.weak_bg_fill = WHITE;
    visuals.inactive.bg_fill = WHITE;
    visuals.hovered.weak_bg_fill = gray::_50;
    visuals.hovered.bg_fill = gray::_50;
    visuals.active.weak_bg_fill = gray::_100;
    visuals.active.bg_fill = gray::_100;
    ui.spacing_mut().button_padding = egui::vec2(spacing::LG, spacing::MD);
    // The right-hand atoms get their own width; the text atom can shrink, never overlap them.
    let trailing_width = if destination.detail.is_empty() {
        24.0
    } else {
        144.0
    };
    let text_width = (ui.available_width()
        - SETTINGS_DESTINATION_ICON_TILE_SIZE
        - spacing::LG * 2.0
        - spacing::XL * 2.0
        - trailing_width)
        .max(20.0);
    let mut text = egui::text::LayoutJob::default();
    text.append(
        destination.title,
        0.0,
        egui::TextFormat {
            font_id: typography::section_heading(),
            color: gray::_900,
            ..Default::default()
        },
    );
    text.append("\n", 0.0, Default::default());
    text.append(
        destination.description,
        0.0,
        egui::TextFormat {
            font_id: typography::body(),
            color: gray::_600,
            ..Default::default()
        },
    );
    text.wrap.max_width = text_width;
    text.wrap.max_rows = 2;
    let text = egui::WidgetText::from(text).atom_shrink(true);
    let response = ui.add(
        egui::Button::new((
            destination
                .icon
                .fit_to_exact_size(egui::Vec2::splat(SETTINGS_DESTINATION_ICON_TILE_SIZE)),
            text,
        ))
        .gap(spacing::XL)
        .right_text((
            egui::RichText::new(destination.detail)
                .font(typography::body())
                .color(gray::_500),
            icons::chevron_right_icon()
                .fit_to_exact_size(egui::Vec2::splat(SETTINGS_DESTINATION_CHEVRON_SIZE))
                .tint(gray::_700),
        ))
        .min_size(egui::vec2(
            ui.available_width(),
            SETTINGS_DESTINATION_CONTENT_HEIGHT,
        ))
        .corner_radius(egui::CornerRadius::ZERO),
    );
    ui.visuals_mut().widgets = saved_widgets;
    ui.spacing_mut().button_padding = saved_button_padding;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            if destination.detail.is_empty() {
                format!("{}: {}", destination.label, destination.description)
            } else {
                format!(
                    "{}: {} ({})",
                    destination.label, destination.description, destination.detail
                )
            },
        )
    });
    response.clicked()
}
