use super::*;
use components::design::theme::Tone;

pub(super) struct SettingsDestination<'a> {
    pub(super) title: &'a str,
    pub(super) label: &'a str,
    pub(super) description: &'a str,
    /// The glyph drawn on the destination's accent tile.
    pub(super) icon: icons::Icon,
    pub(super) detail: SettingsDetail<'a>,
}

/// What a destination row shows before its chevron.
#[derive(Clone, Copy)]
pub(super) enum SettingsDetail<'a> {
    None,
    /// Muted meta text: "4 contexts".
    Meta(&'a str),
    /// A status pill: "Active".
    Status(&'a str, Tone),
}

impl<'a> SettingsDetail<'a> {
    fn text(self) -> Option<&'a str> {
        match self {
            Self::None => None,
            Self::Meta(text) | Self::Status(text, _) => Some(text),
        }
    }
}

/// A captioned card of settings destinations (C13 navigation list rows).
pub(super) fn settings_destination_group<R>(
    ui: &mut egui::Ui,
    heading: &str,
    contents: impl FnOnce(&mut components::ListGroup<'_>) -> R,
) -> R {
    crate::ui::blade_text::blade_caption(ui, heading);
    let result = components::list_group(ui, contents);
    crate::ui::blade_text::gap(ui, space::S24);
    result
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
    let label = match destination.detail.text() {
        None => format!("{}: {}", destination.label, destination.description),
        Some(detail) => format!(
            "{}: {} ({detail})",
            destination.label, destination.description
        ),
    };
    let mut row = components::ListRow::new(destination.icon, destination.title)
        .description(destination.description)
        .accessibility_label(label);
    row = match destination.detail {
        SettingsDetail::None => row,
        SettingsDetail::Meta(text) => row.trailing(components::ListRowTrailing::Meta(text)),
        SettingsDetail::Status(text, tone) => {
            row.trailing(components::ListRowTrailing::Status(text, tone))
        }
    };
    group.row(row).clicked()
}
