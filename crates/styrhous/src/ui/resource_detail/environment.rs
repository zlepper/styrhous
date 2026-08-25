use super::*;

const SOURCE_COLUMN_WIDTH: f32 = spacing::XXL * 2.0;
const SOURCE_ICON_SIZE: f32 = spacing::LG;
const ROW_HEIGHT: f32 = spacing::XXL;
const HEADER_HEIGHT: f32 = spacing::XL;

pub(super) fn environment_variables(
    ui: &mut egui::Ui,
    secret_reveal_scope: egui::Id,
    variables: &[crate::resource_detail::PodEnvironmentVariableDetail],
) {
    section_header(
        ui,
        "Environment variables",
        Some(environment_variable_count_label(variables.len())),
    );
    ui.add_space(spacing::SM);
    egui::Frame::new()
        .fill(WHITE)
        .stroke(surface::muted_border())
        .corner_radius(radius::surface())
        .inner_margin(egui::Margin::symmetric(
            spacing::LG as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            environment_variable_header(ui);
            ui.separator();
            for (index, variable) in variables.iter().enumerate() {
                environment_variable_row(ui, secret_reveal_scope, variable);
                if index + 1 < variables.len() {
                    ui.separator();
                }
            }
        });
}

pub(super) fn environment_variable_count_label(count: usize) -> String {
    if count == 1 {
        "1 variable".to_owned()
    } else {
        format!("{count} variables")
    }
}

pub(super) fn environment_variable_header(ui: &mut egui::Ui) {
    environment_variable_columns(ui, |ui, text_column_width| {
        environment_variable_text_cell(ui, text_column_width, "Key", true, HEADER_HEIGHT);
        environment_variable_text_cell(ui, text_column_width, "Value", true, HEADER_HEIGHT);
        environment_variable_source_header(ui);
    });
}

pub(super) fn environment_variable_row(
    ui: &mut egui::Ui,
    secret_reveal_scope: egui::Id,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    environment_variable_columns(ui, |ui, text_column_width| {
        environment_variable_text_cell(ui, text_column_width, &variable.name, false, ROW_HEIGHT);
        environment_variable_value_cell(ui, text_column_width, secret_reveal_scope, variable);
        environment_variable_source_cell(ui, variable);
    });
}

fn environment_variable_columns(ui: &mut egui::Ui, add_cells: impl FnOnce(&mut egui::Ui, f32)) {
    let gaps = ui.spacing().item_spacing.x * 2.0;
    let text_column_width = ((ui.available_width() - SOURCE_COLUMN_WIDTH - gaps) / 2.0).max(0.0);
    ui.horizontal(|ui| add_cells(ui, text_column_width));
}

fn environment_variable_text_cell(
    ui: &mut egui::Ui,
    width: f32,
    value: &str,
    header: bool,
    height: f32,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(ui.available_width());
            let text = if header {
                egui::RichText::new(value)
                    .font(typography::metadata())
                    .color(gray::_500)
            } else {
                egui::RichText::new(value)
                    .monospace()
                    .font(typography::monospace())
                    .strong()
                    .color(gray::_800)
            };
            ui.add(egui::Label::new(text).selectable(!header).truncate());
        },
    );
}

fn environment_variable_source_header(ui: &mut egui::Ui) {
    ui.allocate_ui_with_layout(
        egui::vec2(SOURCE_COLUMN_WIDTH, HEADER_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true),
        |ui| {
            ui.label(
                egui::RichText::new("Source")
                    .font(typography::metadata())
                    .color(gray::_500),
            );
        },
    );
}

pub(super) fn environment_variable_value_cell(
    ui: &mut egui::Ui,
    width: f32,
    secret_reveal_scope: egui::Id,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let secret = matches!(
        variable.source,
        crate::resource_detail::PodEnvironmentVariableSource::SecretKey { .. }
    );
    let revealed =
        secret && environment_variable_secret_revealed(ui, secret_reveal_scope, variable);
    let value = if secret && !revealed {
        "••••••"
    } else {
        variable.value.as_deref().unwrap_or("Unavailable")
    };
    ui.allocate_ui_with_layout(
        egui::vec2(width, ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secret && variable.value.is_some() {
                    let action = if revealed { "Hide" } else { "Reveal" };
                    let response = components::icons::eye_button(ui, 14.0, gray::_600, action);
                    if response.on_hover_text(action).clicked() {
                        ui.data_mut(|data| {
                            data.insert_temp(
                                environment_variable_secret_id(secret_reveal_scope, variable),
                                !revealed,
                            )
                        });
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(value)
                                .monospace()
                                .font(typography::monospace())
                                .color(gray::_700),
                        )
                        .selectable(true)
                        .truncate(),
                    );
                });
            });
        },
    );
}

pub(super) fn environment_variable_source_cell(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let source = environment_variable_source_label(variable);
    ui.allocate_ui_with_layout(
        egui::vec2(SOURCE_COLUMN_WIDTH, ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true),
        |ui| {
            ui.add(
                environment_variable_source_icon(variable)
                    .fit_to_exact_size(egui::Vec2::splat(SOURCE_ICON_SIZE))
                    .tint(gray::_600)
                    .alt_text(&source)
                    .sense(egui::Sense::hover()),
            )
            .on_hover_text(source);
        },
    );
}

fn environment_variable_source_icon(
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> egui::Image<'static> {
    use crate::resource_detail::PodEnvironmentVariableSource;

    match variable.source {
        PodEnvironmentVariableSource::Literal => components::icons::code_bracket_icon(),
        PodEnvironmentVariableSource::ConfigMapKey { .. }
        | PodEnvironmentVariableSource::ConfigMapImport { .. } => components::icons::folder_icon(),
        PodEnvironmentVariableSource::SecretKey { .. }
        | PodEnvironmentVariableSource::SecretImport { .. } => components::icons::key_icon(),
        PodEnvironmentVariableSource::Field { .. } => components::icons::document_text_icon(),
        PodEnvironmentVariableSource::ResourceField { .. } => components::icons::chart_bar_icon(),
        PodEnvironmentVariableSource::Unspecified => components::icons::question_mark_circle_icon(),
    }
}

pub(super) fn environment_variable_secret_revealed(
    ui: &egui::Ui,
    secret_reveal_scope: egui::Id,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> bool {
    ui.data(|data| {
        data.get_temp::<bool>(environment_variable_secret_id(
            secret_reveal_scope,
            variable,
        ))
        .unwrap_or(false)
    })
}

pub(super) fn environment_variable_secret_id(
    secret_reveal_scope: egui::Id,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> egui::Id {
    secret_reveal_scope.with((
        "environment-variable-secret",
        &variable.name,
        environment_variable_source_label(variable),
    ))
}

pub(super) fn environment_variable_source_label(
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> String {
    use crate::resource_detail::PodEnvironmentVariableSource;

    let resolved = if variable.value.is_some() {
        "resolved"
    } else {
        "unavailable"
    };
    match &variable.source {
        PodEnvironmentVariableSource::Literal => "Literal".to_owned(),
        PodEnvironmentVariableSource::ConfigMapKey {
            name,
            key,
            optional,
        } => {
            format!(
                "ConfigMap {name}/{key}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::SecretKey {
            name,
            key,
            optional,
        } => {
            format!(
                "Secret {name}/{key}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::Field { path } => format!("Field {path} · {resolved}"),
        PodEnvironmentVariableSource::ResourceField {
            resource,
            container_name,
        } => format!(
            "Resource field {resource}{} · {resolved}",
            container_name
                .as_deref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default()
        ),
        PodEnvironmentVariableSource::ConfigMapImport { name, optional, .. } => {
            format!(
                "ConfigMap import {name}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::SecretImport { name, optional, .. } => {
            format!(
                "Secret import {name}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::Unspecified => "Unspecified source".to_owned(),
    }
}

pub(super) fn optional_label(optional: bool) -> &'static str {
    if optional { " (optional)" } else { "" }
}
