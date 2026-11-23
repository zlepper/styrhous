use super::*;

pub(super) struct SettingsDestination<'a> {
    pub(super) title: &'a str,
    pub(super) label: &'a str,
    pub(super) description: &'a str,
    /// The glyph drawn on the destination's accent tile.
    pub(super) icon: icons::Icon,
    pub(super) detail: &'a str,
}

/// A captioned card of settings destinations (C13 navigation list rows).
pub(super) fn settings_destination_group<R>(
    ui: &mut egui::Ui,
    heading: &str,
    contents: impl FnOnce(&mut components::ListGroup<'_>) -> R,
) -> R {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(heading)
            .font(type_scale::BODY_STRONG.font_id(ui.ctx()))
            .color(theme.text.muted),
    );
    ui.add_space(space::S12);
    components::list_group(ui, contents)
}

pub(super) fn settings_destination_divider(ui: &mut egui::Ui) {
    let theme = ui.tokens();
    let y = ui.next_widget_position().y - 0.5;
    ui.painter()
        .hline(ui.max_rect().x_range(), y, stroke::divider(&theme));
}

/// One destination row; returns whether it was activated.
pub(super) fn settings_destination_card(
    group: &mut components::ListGroup<'_>,
    destination: SettingsDestination<'_>,
) -> bool {
    let label = if destination.detail.is_empty() {
        format!("{}: {}", destination.label, destination.description)
    } else {
        format!(
            "{}: {} ({})",
            destination.label, destination.description, destination.detail
        )
    };
    let mut row = components::ListRow::new(destination.icon, destination.title)
        .description(destination.description)
        .accessibility_label(label);
    if !destination.detail.is_empty() {
        row = row.trailing(components::ListRowTrailing::Meta(destination.detail));
    }
    group.row(row).clicked()
}
