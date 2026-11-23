use super::table_preferences::MetadataColumnSource;
use components::{
    Combobox, Radio, TextInput,
    design::theme::ThemeExt,
    design::{space, type_scale},
};

/// Metadata keys observed in the current UI data, grouped by Kubernetes source.
#[derive(Debug, Clone, Default)]
pub(super) struct MetadataKeySuggestions {
    pub(super) labels: Vec<String>,
    pub(super) annotations: Vec<String>,
}

/// Render the shared source, suggestion, and exact-key controls used by metadata features.
/// Returns a selected suggestion so callers may apply feature-specific defaults.
pub(super) fn show_metadata_key_picker(
    ui: &mut egui::Ui,
    source: &mut MetadataColumnSource,
    key: &mut String,
    suggestions: &MetadataKeySuggestions,
    suggestion_placeholder: &str,
) -> Option<String> {
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        source_radios(ui, source);
    });
    ui.add_space(space::S12);
    let source_keys = match source {
        MetadataColumnSource::Label => &suggestions.labels,
        MetadataColumnSource::Annotation => &suggestions.annotations,
    };
    let mut selected = None;
    if !source_keys.is_empty() {
        let selected_text = if key.is_empty() {
            suggestion_placeholder.to_owned()
        } else {
            key.clone()
        };
        Combobox::from_label("Suggested metadata key")
            .placeholder("Search keys...")
            .search_accessibility_label("Search metadata keys")
            .selected_text(selected_text)
            .width(ui.available_width())
            .filter_by(|value: &String| value)
            .show_items(ui, source_keys, |combobox, value| {
                if combobox.item(value, *value == *key).clicked() {
                    *key = value.clone();
                    selected = Some(value.clone());
                }
            });
        ui.add_space(space::S8);
    }
    ui.label(
        egui::RichText::new("Metadata key")
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.primary),
    );
    TextInput::new(key)
        .placeholder("Enter an exact metadata key")
        .accessibility_label("Metadata key")
        .show(ui);
    selected
}

/// Render a label and the shared label/annotation source radio controls.
pub(super) fn show_metadata_source_options(
    ui: &mut egui::Ui,
    source: &mut MetadataColumnSource,
    label: &str,
) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(label)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.primary),
    );
    ui.add_space(space::S4);
    ui.horizontal(|ui| source_radios(ui, source));
}

/// The label / annotation source radios, side by side.
fn source_radios(ui: &mut egui::Ui, source: &mut MetadataColumnSource) {
    ui.spacing_mut().item_spacing.x = space::S16;
    for (value, label) in [
        (MetadataColumnSource::Label, "Label"),
        (MetadataColumnSource::Annotation, "Annotation"),
    ] {
        Radio::new(&mut *source, value, label).show(ui);
    }
}

pub(super) fn source_label(source: MetadataColumnSource) -> &'static str {
    match source {
        MetadataColumnSource::Label => "Label",
        MetadataColumnSource::Annotation => "Annotation",
    }
}
