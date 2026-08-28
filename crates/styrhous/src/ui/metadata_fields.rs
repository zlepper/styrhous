use super::table_preferences::MetadataColumnSource;
use components::{
    TailwindCombobox, TailwindTextInput,
    colors::{gray, indigo},
    design::{spacing, typography},
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
    ui.horizontal(|ui| {
        ui.radio_value(source, MetadataColumnSource::Label, "Label");
        ui.radio_value(source, MetadataColumnSource::Annotation, "Annotation");
    });
    ui.add_space(spacing::MD);
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
        TailwindCombobox::from_label("Suggested metadata key")
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
        ui.add_space(spacing::SM);
    }
    ui.label(
        egui::RichText::new("Metadata key")
            .font(typography::body())
            .color(gray::_800),
    );
    TailwindTextInput::new(key)
        .hint_text("Enter an exact metadata key")
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
    ui.label(
        egui::RichText::new(label)
            .font(typography::body())
            .color(gray::_800),
    );
    ui.add_space(spacing::XS);
    ui.horizontal(|ui| {
        metadata_source_radio(ui, source, MetadataColumnSource::Label, "Label");
        ui.add_space(spacing::SM);
        metadata_source_radio(ui, source, MetadataColumnSource::Annotation, "Annotation");
    });
}

fn metadata_source_radio(
    ui: &mut egui::Ui,
    source: &mut MetadataColumnSource,
    value: MetadataColumnSource,
    label: &str,
) {
    ui.horizontal(|ui| {
        let (circle_rect, radio_response) =
            ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::click());
        ui.add_space(spacing::XS);
        let label_response = ui.add(
            egui::Label::new(
                egui::RichText::new(label)
                    .font(typography::body())
                    .color(gray::_800),
            )
            .sense(egui::Sense::click()),
        );
        if radio_response.clicked() || label_response.clicked() {
            *source = value;
        }
        let selected = *source == value;
        radio_response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::RadioButton,
                ui.is_enabled(),
                selected,
                label,
            )
        });
        let painter = ui.painter();
        if selected {
            painter.circle_filled(circle_rect.center(), 9.0, indigo::_600);
            painter.circle_filled(circle_rect.center(), 3.0, gray::_50);
        } else {
            painter.circle_filled(circle_rect.center(), 8.0, gray::_50);
            painter.circle_stroke(
                circle_rect.center(),
                8.0,
                egui::Stroke::new(1.0, gray::_300),
            );
        }
    });
}

pub(super) fn source_label(source: MetadataColumnSource) -> &'static str {
    match source {
        MetadataColumnSource::Label => "Label",
        MetadataColumnSource::Annotation => "Annotation",
    }
}
