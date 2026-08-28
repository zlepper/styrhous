use super::*;
use crate::ui::metadata_fields::{show_metadata_source_options, source_label};
use crate::ui::namespace_selector::{
    NamespaceIdentityTemplate, NamespaceMetadataField, NamespaceSelectorSettings, presentation,
};

#[derive(Debug)]
pub(crate) struct NamespaceSelectorSettingsBlade {
    draft: NamespaceSelectorSettings,
    editing_field: Option<usize>,
    field_draft: NamespaceMetadataField,
    error: Option<String>,
}

impl NamespaceSelectorSettingsBlade {
    pub(crate) fn new(draft: NamespaceSelectorSettings) -> Self {
        Self {
            draft,
            editing_field: None,
            field_draft: NamespaceMetadataField::default(),
            error: None,
        }
    }

    fn open_new_field(&mut self) {
        self.editing_field = Some(self.draft.fields.len());
        self.field_draft = NamespaceMetadataField::default();
    }

    fn open_existing_field(&mut self, index: usize) {
        self.editing_field = Some(index);
        self.field_draft = self.draft.fields[index].clone();
    }

    fn save_field(&mut self) -> Result<(), String> {
        let field = NamespaceMetadataField {
            alias: self.field_draft.alias.trim().to_owned(),
            source: self.field_draft.source,
            key: self.field_draft.key.trim().to_owned(),
        };
        let index = self.editing_field.expect("field form must be open");
        let mut candidate = self.draft.clone();
        if index == candidate.fields.len() {
            candidate.fields.push(field);
        } else {
            candidate.fields[index] = field;
        }
        candidate.validate_fields()?;
        self.draft = candidate;
        self.editing_field = None;
        Ok(())
    }
}

impl GlobalBladeContent for NamespaceSelectorSettingsBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        ui.label(
            egui::RichText::new("Namespace selector")
                .font(typography::page_title())
                .color(gray::_900),
        );
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let namespaces = context.namespaces();
        let mut save = false;
        let mut reset = false;
        ui.label(
            egui::RichText::new("NAMESPACE SELECTOR")
                .font(typography::metadata())
                .color(gray::_500),
        );
        ui.add_space(spacing::XS);
        ui.label(
            egui::RichText::new(
                "Create meaningful namespace identities from labels and annotations. These settings apply to every cluster on this device.",
            )
            .font(typography::body())
            .color(gray::_600),
        );
        ui.add_space(spacing::XL);
        ui.separator();
        ui.add_space(if self.editing_field.is_some() {
            12.0
        } else {
            30.0
        });
        let content_height = (ui.available_height() - FOOTER_HEIGHT).max(120.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), content_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                components::scroll::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        show_fields(ui, self);
                        ui.add_space(if self.editing_field.is_some() {
                            0.0
                        } else {
                            28.0
                        });
                        ui.separator();
                        ui.add_space(if self.editing_field.is_some() {
                            14.0
                        } else {
                            spacing::XL
                        });
                        show_templates(ui, &mut self.draft);
                        if let Some(namespace) = namespaces.first() {
                            ui.add_space(spacing::SM);
                            show_preview(ui, namespace, &self.draft);
                        }
                        if let Some(error) = &self.error {
                            ui.add_space(spacing::LG);
                            show_validation_error(
                                ui,
                                "Namespace selector settings need attention",
                                error,
                            );
                        }
                    });
            },
        );
        ui.separator();
        ui.add_space(spacing::SM);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            save |= TailwindButton::new("Save changes").show(ui).clicked();
            reset |= TailwindButton::secondary("Reset")
                .size(ButtonSize::Md)
                .show(ui)
                .clicked();
        });
        if reset {
            self.draft = NamespaceSelectorSettings::default();
            self.editing_field = None;
            self.error = None;
        }
        if save {
            match self.draft.validate() {
                Ok(()) => {
                    *context.namespace_selector_settings() = self.draft.clone();
                    return GlobalBladeRenderResult {
                        close: true,
                        ..Default::default()
                    };
                }
                Err(error) => self.error = Some(error),
            }
        }
        GlobalBladeRenderResult::default()
    }
}

fn show_fields(ui: &mut egui::Ui, blade: &mut NamespaceSelectorSettingsBlade) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Metadata fields")
                .font(typography::section_heading())
                .color(gray::_900),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if TailwindButton::primary("Add field")
                .size(ButtonSize::Sm)
                .show(ui)
                .clicked()
            {
                blade.open_new_field();
                blade.error = None;
            }
        });
    });
    ui.add_space(spacing::SM);
    if blade.editing_field.is_none() {
        ui.add_space(spacing::SM);
    }
    let mut remove = None;
    let mut edit = None;
    let mut close_editor = false;
    let table_width = ui.available_width();
    egui::Frame::new()
        .fill(WHITE)
        .stroke(surface::muted_border())
        .corner_radius(radius::surface())
        .show(ui, |ui| {
            let original_item_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.y = 0.0;
            if blade.editing_field.is_some() {
                for index in 0..blade.draft.fields.len() {
                    let is_editing = blade.editing_field == Some(index);
                    let row_actions = {
                        let field = &mut blade.draft.fields[index];
                        show_metadata_field_row(ui, field, index, is_editing, None, 64.0)
                    };
                    remove = remove.or(row_actions.remove);
                    edit = edit.or(row_actions.edit);
                    close_editor |= row_actions.close_editor;
                    if is_editing {
                        show_field_form(ui, blade);
                    }
                }
                if blade.editing_field == Some(blade.draft.fields.len()) {
                    show_field_form(ui, blade);
                }
            } else {
                ReorderableTable::new("namespace-metadata-fields", 84.0).show(
                    ui,
                    &mut blade.draft.fields,
                    table_width,
                    |ui, field, index, handle| {
                        let row_actions =
                            show_metadata_field_row(ui, field, index, false, Some(handle), 84.0);
                        remove = remove.or(row_actions.remove);
                        edit = edit.or(row_actions.edit);
                    },
                    |ui, field| {
                        ui.label(&field.alias);
                    },
                );
            }
            ui.spacing_mut().item_spacing = original_item_spacing;
        });
    if close_editor {
        blade.editing_field = None;
    }
    if let Some(index) = remove {
        let alias = blade.draft.fields[index].alias.clone();
        if blade
            .draft
            .templates
            .iter()
            .any(|template| template.template.contains(&format!("{{{{{alias}}}}}")))
        {
            blade.error = Some(format!(
                "Remove or update templates that use {{{{{alias}}}}} before removing this field."
            ));
        } else {
            blade.draft.fields.remove(index);
            blade.editing_field = None;
        }
    }
    if let Some(index) = edit {
        blade.open_existing_field(index);
    }
}

#[derive(Default)]
struct MetadataFieldRowActions {
    remove: Option<usize>,
    edit: Option<usize>,
    close_editor: bool,
}

fn show_metadata_field_row(
    ui: &mut egui::Ui,
    field: &mut NamespaceMetadataField,
    index: usize,
    is_editing: bool,
    handle: Option<&components::ReorderHandle>,
    row_height: f32,
) -> MetadataFieldRowActions {
    let mut actions = MetadataFieldRowActions::default();
    let (row_rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_height),
        egui::Sense::hover(),
    );
    if index > 0 {
        ui.painter()
            .hline(row_rect.x_range(), row_rect.top(), surface::muted_border());
    }
    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row_ui.add_space(18.0);
    let (handle_rect, response) = row_ui.allocate_exact_size(
        egui::Vec2::splat(32.0),
        if handle.is_some() {
            egui::Sense::drag()
        } else {
            egui::Sense::hover()
        },
    );
    if let Some(handle) = handle {
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                row_ui.is_enabled(),
                format!("Reorder {} field", field.alias),
            )
        });
        handle.register(&response.with_pointing_hand());
    }
    let icon_rect = egui::Rect::from_center_size(handle_rect.center(), egui::Vec2::splat(16.0));
    let mut handle_ui = row_ui.new_child(
        egui::UiBuilder::new()
            .max_rect(icon_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    icons::bars_3(&mut handle_ui, 16.0, gray::_500);
    row_ui.add_space(11.0);
    row_ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{}: {}", source_label(field.source), field.key))
                .font(typography::body())
                .color(gray::_600),
        );
        ui.add_space(spacing::MD);
        metadata_key_pill(ui, &field.alias);
    });
    row_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add_space(30.0);
        if TailwindButton::icon(icons::trash_icon().fit_to_exact_size(egui::Vec2::splat(16.0)))
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Md)
            .accessibility_label(format!("Remove {} field", field.alias))
            .show(ui)
            .clicked()
        {
            actions.remove = Some(index);
        }
        ui.add_space(spacing::SM);
        let (icon, label) = if is_editing {
            (
                icons::arrow_up_icon(),
                format!("Close {} field editor", field.alias),
            )
        } else {
            (icons::pencil_icon(), format!("Edit {} field", field.alias))
        };
        if TailwindButton::icon(icon.fit_to_exact_size(egui::Vec2::splat(16.0)))
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Md)
            .accessibility_label(label)
            .show(ui)
            .clicked()
        {
            if is_editing {
                actions.close_editor = true;
            } else {
                actions.edit = Some(index);
            }
        }
    });
    actions
}

fn show_field_form(ui: &mut egui::Ui, blade: &mut NamespaceSelectorSettingsBlade) {
    ui.add_space(spacing::XS);
    egui::Frame::new()
        .fill(WHITE)
        .stroke(surface::muted_border())
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::same(spacing::LG as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if blade.editing_field == Some(blade.draft.fields.len()) {
                        "Add metadata field"
                    } else {
                        "Edit metadata field"
                    })
                    .font(typography::section_heading())
                    .color(gray::_900),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if TailwindButton::primary("Save field")
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                    {
                        blade.error = blade.save_field().err();
                    }
                    if TailwindButton::secondary("Cancel")
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                    {
                        blade.editing_field = None;
                    }
                });
            });
            ui.add_space(spacing::MD);
            show_metadata_source_options(ui, &mut blade.field_draft.source, "Source");
            ui.add_space(spacing::MD);
            labeled_input(
                ui,
                "Selector",
                &mut blade.field_draft.key,
                "company.example/customer",
            );
            ui.add_space(spacing::SM);
            labeled_input(ui, "Template key", &mut blade.field_draft.alias, "customer");
            ui.add_space(spacing::LG);
        });
    ui.add_space(spacing::XS);
}

fn show_templates(ui: &mut egui::Ui, settings: &mut NamespaceSelectorSettings) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Identity templates")
                .font(typography::section_heading())
                .color(gray::_900),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if TailwindButton::primary("Add template")
                .size(ButtonSize::Sm)
                .show(ui)
                .clicked()
            {
                settings.templates.push(NamespaceIdentityTemplate {
                    template: String::new(),
                });
            }
        });
    });
    ui.add_space(spacing::SM);
    ui.label(
        egui::RichText::new("The first template with all its values available is used.")
            .font(typography::body())
            .color(gray::_600),
    );
    ui.add_space(spacing::SM);
    let mut remove = None;
    let table_width = ui.available_width();
    ui.add_space(3.0);
    egui::Frame::new()
        .fill(WHITE)
        .stroke(surface::muted_border())
        .corner_radius(radius::surface())
        .show(ui, |ui| {
            let original_item_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.y = 0.0;
            let reordered = ReorderableTable::new("namespace-identity-templates", 72.0).show(
                ui,
                &mut settings.templates,
                table_width,
                |ui, template, index, handle| {
                    let (row_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 72.0),
                        egui::Sense::hover(),
                    );
                    if index > 0 {
                        ui.painter().hline(
                            row_rect.x_range(),
                            row_rect.top(),
                            surface::muted_border(),
                        );
                    }
                    let center_y = row_rect.center().y;
                    let handle_rect = egui::Rect::from_center_size(
                        egui::pos2(row_rect.left() + 35.0, center_y),
                        egui::Vec2::splat(32.0),
                    );
                    let remove_rect = egui::Rect::from_center_size(
                        egui::pos2(row_rect.right() - 43.0, center_y),
                        egui::Vec2::splat(36.0),
                    );
                    let input_rect = egui::Rect::from_min_max(
                        egui::pos2(row_rect.left() + 67.0, center_y - 14.0),
                        egui::pos2(remove_rect.left() - spacing::LG, center_y + 14.0),
                    );

                    let response = ui.interact(
                        handle_rect,
                        ui.id().with(("reorder-template", index)),
                        egui::Sense::drag(),
                    );
                    let icon_rect =
                        egui::Rect::from_center_size(handle_rect.center(), egui::Vec2::splat(16.0));
                    let mut handle_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(icon_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            handle_ui.is_enabled(),
                            format!("Reorder identity template {}", index + 1),
                        )
                    });
                    icons::bars_3(&mut handle_ui, 16.0, gray::_500);
                    handle.register(&response.with_pointing_hand());

                    let mut input_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(input_rect).layout(
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        ));
                    TailwindTextInput::new(&mut template.template)
                        .id_salt(("namespace-identity-template", index))
                        .hint_text("Customer: {{customer}}")
                        .accessibility_label(format!("Identity template {}", index + 1))
                        .show(&mut input_ui);

                    let mut remove_ui =
                        ui.new_child(egui::UiBuilder::new().max_rect(remove_rect).layout(
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        ));
                    if TailwindButton::icon(
                        icons::trash_icon().fit_to_exact_size(egui::Vec2::splat(16.0)),
                    )
                    .variant(ButtonVariant::Secondary)
                    .size(ButtonSize::Md)
                    .accessibility_label(format!("Remove identity template {}", index + 1))
                    .show(&mut remove_ui)
                    .clicked()
                    {
                        remove = Some(index);
                    }
                },
                |ui, template| {
                    ui.label(&template.template);
                },
            );
            if reordered {
                ui.ctx().memory_mut(|memory| {
                    for index in 0..settings.templates.len() {
                        memory
                            .surrender_focus(egui::Id::new(("namespace-identity-template", index)));
                    }
                });
            }
            show_namespace_name_fallback(ui, table_width);
            ui.spacing_mut().item_spacing = original_item_spacing;
        });
    if let Some(index) = remove {
        settings.templates.remove(index);
    }
}

fn show_preview(
    ui: &mut egui::Ui,
    namespace: &crate::minimal_namespace::MinimalNamespace,
    settings: &NamespaceSelectorSettings,
) {
    let display = presentation(namespace, settings);
    let preview_width = ui.available_width();
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, indigo::_200))
        .corner_radius(radius::surface())
        .inner_margin(egui::Margin::same(spacing::LG as i8))
        .show(ui, |ui| {
            ui.set_min_width(preview_width - spacing::LG * 2.0);
            ui.set_min_height(96.0);
            ui.label(
                egui::RichText::new("Preview")
                    .font(typography::section_heading())
                    .color(indigo::_600),
            );
            ui.add_space(spacing::MD);
            ui.label(
                egui::RichText::new(display.primary)
                    .font(typography::section_heading())
                    .color(gray::_900),
            );
            ui.add_space(spacing::XS);
            ui.label(
                egui::RichText::new(display.secondary)
                    .font(typography::body())
                    .color(gray::_600),
            );
        });
}

fn metadata_key_pill(ui: &mut egui::Ui, key: &str) {
    egui::Frame::new()
        .fill(gray::_100)
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            spacing::SM as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(key)
                    .font(typography::metadata())
                    .color(gray::_600),
            );
        });
}

fn show_namespace_name_fallback(ui: &mut egui::Ui, table_width: f32) {
    let (row_rect, _) = ui.allocate_exact_size(egui::vec2(table_width, 72.0), egui::Sense::hover());
    ui.painter()
        .hline(row_rect.x_range(), row_rect.top(), surface::muted_border());
    let input_rect = egui::Rect::from_min_max(
        egui::pos2(row_rect.left() + 67.0, row_rect.center().y - 14.0),
        egui::pos2(row_rect.right() - spacing::LG, row_rect.center().y + 14.0),
    );
    let mut fallback_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(input_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    egui::Frame::new()
        .fill(gray::_50)
        .stroke(surface::control_border())
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            spacing::SM as i8,
            spacing::XS as i8,
        ))
        .show(&mut fallback_ui, |ui| {
            ui.label(
                egui::RichText::new("Namespace name")
                    .font(typography::body())
                    .color(gray::_500),
            );
        });
}

fn labeled_input(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.label(
        egui::RichText::new(label)
            .font(typography::body())
            .color(gray::_800),
    );
    ui.add_space(spacing::XS);
    TailwindTextInput::new(value)
        .hint_text(hint)
        .accessibility_label(label)
        .show(ui);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_a_field_does_not_require_unfinished_templates_to_be_valid() {
        let mut blade = NamespaceSelectorSettingsBlade::new(NamespaceSelectorSettings {
            fields: Vec::new(),
            templates: vec![NamespaceIdentityTemplate {
                template: String::new(),
            }],
        });
        blade.open_new_field();
        blade.field_draft.alias = "customer".into();
        blade.field_draft.key = "company.example/customer".into();

        assert!(blade.save_field().is_ok());
        assert_eq!(blade.draft.fields.len(), 1);
    }
}
