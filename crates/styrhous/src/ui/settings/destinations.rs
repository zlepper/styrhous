use super::*;

pub(super) struct SettingsDestination<'a> {
    pub(super) label: &'a str,
    pub(super) description: &'a str,
    pub(super) icon: egui::Image<'static>,
}

impl SettingsDestination<'_> {
    fn text(&self) -> egui::text::LayoutJob {
        let mut text = egui::text::LayoutJob::default();
        text.append(
            self.label,
            0.0,
            egui::TextFormat {
                font_id: typography::section_heading(),
                color: gray::_900,
                ..Default::default()
            },
        );
        text.append("\n", 0.0, Default::default());
        text.append(
            self.description,
            0.0,
            egui::TextFormat {
                font_id: typography::body(),
                color: gray::_600,
                ..Default::default()
            },
        );
        text
    }
}

pub(super) fn settings_destination_card(
    ui: &mut egui::Ui,
    destination: SettingsDestination<'_>,
) -> bool {
    let saved_widgets = ui.visuals().widgets.clone();
    let saved_button_padding = ui.spacing().button_padding;
    let visuals = &mut ui.visuals_mut().widgets;
    for widget_visuals in [
        &mut visuals.inactive,
        &mut visuals.hovered,
        &mut visuals.active,
    ] {
        widget_visuals.bg_stroke = surface::muted_border();
        widget_visuals.corner_radius = radius::surface();
    }
    visuals.inactive.weak_bg_fill = WHITE;
    visuals.inactive.bg_fill = WHITE;
    visuals.hovered.weak_bg_fill = gray::_50;
    visuals.hovered.bg_fill = gray::_50;
    visuals.active.weak_bg_fill = gray::_100;
    visuals.active.bg_fill = gray::_100;
    ui.spacing_mut().button_padding = egui::vec2(spacing::LG, spacing::MD);
    let accessible_label = format!("{}: {}", destination.label, destination.description);
    let text = egui::WidgetText::from(destination.text()).atom_shrink(true);
    let response = ui.add(
        egui::Button::new((
            destination
                .icon
                .fit_to_exact_size(egui::Vec2::splat(SETTINGS_DESTINATION_ICON_TILE_SIZE)),
            text,
        ))
        .gap(spacing::XL)
        .right_text(
            icons::chevron_right_icon()
                .fit_to_exact_size(egui::Vec2::splat(SETTINGS_DESTINATION_CHEVRON_SIZE))
                .tint(gray::_700),
        )
        .min_size(egui::vec2(
            ui.available_width(),
            SETTINGS_DESTINATION_CONTENT_HEIGHT,
        ))
        .corner_radius(radius::surface()),
    );

    ui.visuals_mut().widgets = saved_widgets;
    ui.spacing_mut().button_padding = saved_button_padding;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            accessible_label.clone(),
        )
    });
    response.clicked()
}
