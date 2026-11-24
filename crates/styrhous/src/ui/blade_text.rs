//! Text and spacing the global blades (Settings, Notifications, …) share: the
//! intro line under the header and the small-caps captions that open each
//! section.

use components::design::theme::ThemeExt;
use components::design::{space, type_scale};

/// Vertical space of exactly `amount` between two items (CSS `margin`): the
/// style's item spacing is part of the gap, not on top of it.
pub(super) fn gap(ui: &mut egui::Ui, amount: f32) {
    let spacing = ui.spacing().item_spacing.y;
    ui.add_space((amount - spacing).max(0.0));
}

/// The blade's opening sentence (`.t-body.c2`), [`space::S24`] above the
/// first section.
pub(super) fn blade_intro(ui: &mut egui::Ui, text: &str) {
    type_scale::BODY.label(ui, text, ui.tokens().text.secondary);
    gap(ui, space::S24);
}

/// A section caption (`.t-cap`): small caps in muted text, [`space::S8`] above
/// the section's content.
pub(super) fn blade_caption(ui: &mut egui::Ui, text: &str) {
    type_scale::CAPTION.label(ui, text, ui.tokens().text.muted);
    gap(ui, space::S8);
}

/// A section's heading (`.sec`): its title in 14/20 semibold with an
/// optional muted hint after it on the same baseline, [`space::S12`] above
/// the section's content.
pub(super) fn blade_section(ui: &mut egui::Ui, title: &str, hint: Option<&str>) {
    let theme = ui.tokens();
    let title_role = type_scale::BODY_STRONG;
    let hint_role = type_scale::META;
    let row_height = title_role.line_height();
    let baseline = title_role.centred_baseline(row_height, ui.pixels_per_point());
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), row_height),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.x = space::S8;
            title_role.label_on_baseline(ui, title, theme.text.primary, baseline);
            if let Some(hint) = hint {
                hint_role.label_on_baseline(ui, hint, theme.text.muted, baseline);
            }
        },
    );
    gap(ui, space::S12);
}

/// The "More actions" trigger blades show before their close button. The
/// menus behind it are not built yet, so it is drawn as designed and says so.
pub(super) fn blade_more_actions(ui: &mut egui::Ui) -> egui::Response {
    components::Button::icon(components::icons::Icon::EllipsisHorizontal, "More actions")
        .variant(components::ButtonVariant::Ghost)
        .size(components::ButtonSize::Sm)
        .not_implemented()
        .show(ui)
}

/// A [`blade_caption`] with `gap` under it instead of [`space::S8`].
pub(super) fn blade_caption_with_gap(ui: &mut egui::Ui, text: &str, amount: f32) {
    type_scale::CAPTION.label(ui, text, ui.tokens().text.muted);
    gap(ui, amount);
}
