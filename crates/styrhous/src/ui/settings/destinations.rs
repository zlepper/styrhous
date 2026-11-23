use super::*;

pub(super) struct SettingsDestination<'a> {
    pub(super) title: &'a str,
    pub(super) label: &'a str,
    pub(super) description: &'a str,
    /// The glyph drawn on the destination's accent tile.
    pub(super) icon: egui::Image<'static>,
    pub(super) detail: &'a str,
}

pub(super) fn settings_destination_group<R>(
    ui: &mut egui::Ui,
    heading: &str,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(heading)
            .font(type_scale::BODY_STRONG.font_id(ui.ctx()))
            .color(theme.text.muted),
    );
    ui.add_space(space::S12);
    WorkspaceCard::new().margin(0).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        contents(ui)
    })
}

pub(super) fn settings_destination_divider(ui: &mut egui::Ui) {
    let theme = ui.tokens();
    let y = ui.next_widget_position().y - 0.5;
    ui.painter()
        .hline(ui.max_rect().x_range(), y, stroke::divider(&theme));
}

pub(super) fn settings_destination_card(
    ui: &mut egui::Ui,
    destination: SettingsDestination<'_>,
) -> bool {
    let theme = ui.tokens();
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
    // At rest the row is the card it sits on, so its square corners never
    // show past the card's rounded ones.
    visuals.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.hovered.weak_bg_fill = theme.components.row_bg_hover;
    visuals.hovered.bg_fill = theme.components.row_bg_hover;
    visuals.active.weak_bg_fill = theme.item.highlight;
    visuals.active.bg_fill = theme.item.highlight;
    ui.spacing_mut().button_padding = egui::vec2(space::S16, space::S12);
    // The right-hand atoms get their own width; the text atom can shrink, never overlap them.
    let trailing_width = if destination.detail.is_empty() {
        24.0
    } else {
        144.0
    };
    let text_width = (ui.available_width()
        - SETTINGS_DESTINATION_ICON_TILE_SIZE
        - space::S16 * 2.0
        - space::S24 * 2.0
        - trailing_width)
        .max(20.0);
    let mut text = egui::text::LayoutJob::default();
    text.append(
        destination.title,
        0.0,
        egui::TextFormat {
            font_id: type_scale::TITLE.font_id(ui.ctx()),
            color: theme.text.primary,
            ..Default::default()
        },
    );
    text.append("\n", 0.0, Default::default());
    text.append(
        destination.description,
        0.0,
        egui::TextFormat {
            font_id: type_scale::BODY.font_id(ui.ctx()),
            color: theme.text.secondary,
            ..Default::default()
        },
    );
    text.wrap.max_width = text_width;
    text.wrap.max_rows = 2;
    let text = egui::WidgetText::from(text).atom_shrink(true);
    let tile_id = ui
        .id()
        .with(("settings-destination-tile", destination.label));
    let response = egui::Button::new((
        egui::Atom::custom(
            tile_id,
            egui::Vec2::splat(SETTINGS_DESTINATION_ICON_TILE_SIZE),
        ),
        text,
    ))
    .gap(space::S24)
    .right_text((
        egui::RichText::new(destination.detail)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.muted),
        icons::chevron_right_icon()
            .fit_to_exact_size(egui::Vec2::splat(SETTINGS_DESTINATION_CHEVRON_SIZE))
            .tint(theme.text.secondary),
    ))
    .min_size(egui::vec2(
        ui.available_width(),
        SETTINGS_DESTINATION_CONTENT_HEIGHT,
    ))
    .corner_radius(egui::CornerRadius::ZERO)
    .atom_ui(ui);
    if let Some(tile) = response.rect(tile_id) {
        icons::paint_tile(
            ui,
            ui.painter(),
            tile,
            destination.icon,
            components::design::theme::Tone::Accent,
            radius::ROW,
        );
    }
    let response = response.response;
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
