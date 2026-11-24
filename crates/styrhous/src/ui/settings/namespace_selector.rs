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
    fn description(&self) -> components::BladeDescription {
        components::BladeDescription::named("Namespace selector")
    }

    fn sidebar_destination(&self) -> Option<crate::ui::global_blade::SidebarDestination> {
        Some(crate::ui::global_blade::SidebarDestination::Settings)
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        components::blade_title(ui, None, "Namespace selector");
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let theme = ui.tokens();
        let namespaces = context.namespaces();
        let mut save = false;
        let mut reset = false;
        ui.label(
            egui::RichText::new("NAMESPACE SELECTOR")
                .font(type_scale::META.font_id(ui.ctx()))
                .color(theme.text.muted),
        );
        ui.add_space(space::S4);
        ui.label(
            egui::RichText::new(
                "Create meaningful namespace identities from labels and annotations. These settings apply to every cluster on this device.",
            )
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.secondary),
        );
        ui.add_space(space::S24);
        ui.separator();
        ui.add_space(if self.editing_field.is_some() {
            12.0
        } else {
            30.0
        });
        // The form scrolls in whatever room the blade footer leaves.
        let content_height = (ui.available_height() - FOOTER_HEIGHT).max(0.0);
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
                            space::S24
                        });
                        show_templates(ui, &mut self.draft);
                        if let Some(namespace) = namespaces.first() {
                            ui.add_space(space::S8);
                            show_preview(ui, namespace, &self.draft);
                        }
                        if let Some(error) = &self.error {
                            ui.add_space(space::S16);
                            show_validation_error(
                                ui,
                                "Namespace selector settings need attention",
                                error,
                            );
                        }
                    });
            },
        );
        components::blade_body_footer(ui, |ui| {
            save |= Button::primary("Save changes").show(ui).clicked();
            reset |= Button::secondary("Reset").show(ui).clicked();
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
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Metadata fields")
                .font(type_scale::TITLE.font_id(ui.ctx()))
                .color(theme.text.primary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if Button::soft("Add field")
                .size(ButtonSize::Sm)
                .show(ui)
                .clicked()
            {
                blade.open_new_field();
                blade.error = None;
            }
        });
    });
    ui.add_space(space::S8);
    if blade.editing_field.is_none() {
        ui.add_space(space::S8);
    }
    let mut remove = None;
    let mut edit = None;
    let mut close_editor = false;
    Card::new().padding(0).show_inner(ui, |ui| {
        // Measured inside the frame so its stroke is already excluded.
        let table_width = ui.available_width();
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
            ReorderableTable::new("namespace-metadata-fields").show_rows(
                ui,
                &mut blade.draft.fields,
                84.0,
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
    let theme = ui.tokens();
    let mut actions = MetadataFieldRowActions::default();
    let (row_rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_height),
        egui::Sense::hover(),
    );
    if index > 0 {
        ui.painter()
            .hline(row_rect.x_range(), row_rect.top(), stroke::divider(&theme));
    }
    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row_ui.add_space(18.0);
    let (handle_rect, _) =
        row_ui.allocate_exact_size(egui::Vec2::splat(32.0), egui::Sense::hover());
    match handle {
        Some(handle) => {
            handle.grip(
                &mut row_ui,
                handle_rect,
                &format!("Reorder {} field", field.alias),
            );
        }
        // Being edited: the grip shows, but the row does not move.
        None => components::ReorderHandle::paint_still(&row_ui, handle_rect),
    }
    row_ui.add_space(11.0);
    // The row's own centred layout, so the taller chip centres in the row too.
    row_ui.scope(|ui| {
        ui.label(
            egui::RichText::new(format!("{}: {}", source_label(field.source), field.key))
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(theme.text.secondary),
        );
        ui.add_space(space::S12);
        components::Chip::new(&field.alias).show(ui);
    });
    row_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.add_space(30.0);
        if Button::icon(icons::Icon::Trash, format!("Remove {} field", field.alias))
            .variant(ButtonVariant::DangerQuiet)
            .show(ui)
            .clicked()
        {
            actions.remove = Some(index);
        }
        ui.add_space(space::S8);
        let (icon, label) = if is_editing {
            (
                icons::Icon::ArrowUp,
                format!("Close {} field editor", field.alias),
            )
        } else {
            (icons::Icon::Pencil, format!("Edit {} field", field.alias))
        };
        if Button::icon(icon, label).show(ui).clicked() {
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
    let theme = ui.tokens();
    ui.add_space(space::S4);
    Card::new().show_inner(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(if blade.editing_field == Some(blade.draft.fields.len()) {
                    "Add metadata field"
                } else {
                    "Edit metadata field"
                })
                .font(type_scale::TITLE.font_id(ui.ctx()))
                .color(theme.text.primary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Button::primary("Save field")
                    .size(ButtonSize::Sm)
                    .show(ui)
                    .clicked()
                {
                    blade.error = blade.save_field().err();
                }
                if Button::secondary("Cancel")
                    .size(ButtonSize::Sm)
                    .show(ui)
                    .clicked()
                {
                    blade.editing_field = None;
                }
            });
        });
        ui.add_space(space::S12);
        show_metadata_source_options(ui, &mut blade.field_draft.source, "Source");
        ui.add_space(space::S12);
        labeled_input(
            ui,
            "Selector",
            &mut blade.field_draft.key,
            "company.example/customer",
        );
        ui.add_space(space::S8);
        labeled_input(ui, "Template key", &mut blade.field_draft.alias, "customer");
        ui.add_space(space::S16);
    });
    ui.add_space(space::S4);
}

fn show_templates(ui: &mut egui::Ui, settings: &mut NamespaceSelectorSettings) {
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Identity templates")
                .font(type_scale::TITLE.font_id(ui.ctx()))
                .color(theme.text.primary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if Button::soft("Add template")
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
    ui.add_space(space::S8);
    ui.label(
        egui::RichText::new("The first template with all its values available is used.")
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.secondary),
    );
    ui.add_space(space::S8);
    let mut remove = None;
    ui.add_space(3.0);
    Card::new().padding(0).show_inner(ui, |ui| {
        // Measured inside the frame so its stroke is already excluded.
        let table_width = ui.available_width();
        let original_item_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.y = 0.0;
        let reordered = ReorderableTable::new("namespace-identity-templates").show_rows(
            ui,
            &mut settings.templates,
            72.0,
            table_width,
            |ui, template, index, handle| {
                let (row_rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 72.0),
                    egui::Sense::hover(),
                );
                if index > 0 {
                    ui.painter()
                        .hline(row_rect.x_range(), row_rect.top(), stroke::divider(&theme));
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
                    egui::pos2(
                        row_rect.left() + 67.0,
                        center_y - components::FieldSize::Md.height() / 2.0,
                    ),
                    egui::pos2(
                        remove_rect.left() - space::S16,
                        center_y + components::FieldSize::Md.height() / 2.0,
                    ),
                );

                handle.grip(
                    ui,
                    handle_rect,
                    &format!("Reorder identity template {}", index + 1),
                );

                let mut input_ui =
                    ui.new_child(egui::UiBuilder::new().max_rect(input_rect).layout(
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                    ));
                TextInput::new(&mut template.template)
                    .id_salt(("namespace-identity-template", index))
                    .placeholder("Customer: {{customer}}")
                    .accessibility_label(format!("Identity template {}", index + 1))
                    .show(&mut input_ui);

                // Centred, not justified: the icon button stays square.
                let mut remove_ui = ui.new_child(
                    egui::UiBuilder::new().max_rect(remove_rect).layout(
                        egui::Layout::left_to_right(egui::Align::Center)
                            .with_main_align(egui::Align::Center),
                    ),
                );
                if Button::icon(
                    icons::Icon::Trash,
                    format!("Remove identity template {}", index + 1),
                )
                .variant(ButtonVariant::DangerQuiet)
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
                    memory.surrender_focus(egui::Id::new(("namespace-identity-template", index)));
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
    let theme = ui.tokens();
    let display = presentation(namespace, settings);
    Card::new()
        .stroke(stroke::hairline(theme.accent.outline))
        .show_inner(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(96.0);
            ui.label(
                egui::RichText::new("Preview")
                    .font(type_scale::TITLE.font_id(ui.ctx()))
                    .color(theme.accent.text),
            );
            ui.add_space(space::S12);
            ui.label(
                egui::RichText::new(display.primary)
                    .font(type_scale::TITLE.font_id(ui.ctx()))
                    .color(theme.text.primary),
            );
            ui.add_space(space::S4);
            ui.label(
                egui::RichText::new(display.secondary)
                    .font(type_scale::BODY.font_id(ui.ctx()))
                    .color(theme.text.secondary),
            );
        });
}

fn show_namespace_name_fallback(ui: &mut egui::Ui, table_width: f32) {
    let theme = ui.tokens();
    let (row_rect, _) = ui.allocate_exact_size(egui::vec2(table_width, 72.0), egui::Sense::hover());
    ui.painter()
        .hline(row_rect.x_range(), row_rect.top(), stroke::divider(&theme));
    let input_rect = egui::Rect::from_min_max(
        egui::pos2(row_rect.left() + 67.0, row_rect.center().y - 14.0),
        egui::pos2(row_rect.right() - space::S16, row_rect.center().y + 14.0),
    );
    let mut fallback_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(input_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    egui::Frame::new()
        .fill(theme.control.fill_disabled)
        .stroke(stroke::control(&theme))
        .corner_radius(egui::CornerRadius::same(radius::KEY))
        .inner_margin(egui::Margin::symmetric(space::S8 as i8, space::S4 as i8))
        .show(&mut fallback_ui, |ui| {
            ui.label(
                egui::RichText::new("Namespace name")
                    .font(type_scale::BODY.font_id(ui.ctx()))
                    .color(theme.text.muted),
            );
        });
}

fn labeled_input(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(label)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.primary),
    );
    ui.add_space(space::S4);
    TextInput::new(value)
        .placeholder(hint)
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
