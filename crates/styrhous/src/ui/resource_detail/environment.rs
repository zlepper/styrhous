use super::*;

pub(super) fn environment_variables(
    ui: &mut egui::Ui,
    variables: &[crate::resource_detail::PodEnvironmentVariableDetail],
) {
    ui.label(
        egui::RichText::new("Environment variables")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(gray::_100)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            spacing::SM as i8,
            (spacing::SM + 2.0) as i8,
        ))
        .show(ui, |ui| {
            environment_variable_header(ui);
            for variable in variables {
                ui.add_space(2.0);
                environment_variable_row(ui, variable);
            }
        });
}

pub(super) fn environment_variable_header(ui: &mut egui::Ui) {
    ui.columns(3, |columns| {
        environment_variable_cell(&mut columns[0], "Key", true);
        environment_variable_cell(&mut columns[1], "Value", true);
        environment_variable_cell(&mut columns[2], "Source", true);
    });
}

pub(super) fn environment_variable_row(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    ui.columns(3, |columns| {
        environment_variable_cell(&mut columns[0], &variable.name, false);
        environment_variable_value_cell(&mut columns[1], variable);
        environment_variable_source_cell(&mut columns[2], variable);
    });
}

pub(super) fn environment_variable_cell(ui: &mut egui::Ui, value: &str, header: bool) {
    let text = egui::RichText::new(value)
        .monospace()
        .font(typography::monospace())
        .color(if header { gray::_600 } else { gray::_800 });
    let text = if header { text.strong() } else { text };
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM - 2.0) as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.add(egui::Label::new(text).selectable(!header).wrap());
        });
}

pub(super) fn environment_variable_value_cell(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let secret = matches!(
        variable.source,
        crate::resource_detail::PodEnvironmentVariableSource::SecretKey { .. }
    );
    let revealed = secret && environment_variable_secret_revealed(ui, variable);
    let value = if secret && !revealed {
        "••••••"
    } else {
        variable.value.as_deref().unwrap_or("Unavailable")
    };
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM - 2.0) as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if secret && variable.value.is_some() {
                    let action = if revealed { "Hide" } else { "Reveal" };
                    let response = components::icons::eye_button(ui, 14.0, gray::_600, action);
                    if response.on_hover_text(action).clicked() {
                        ui.data_mut(|data| {
                            data.insert_temp(
                                environment_variable_secret_id(ui, variable),
                                !revealed,
                            )
                        });
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(value)
                                .monospace()
                                .font(typography::monospace())
                                .color(gray::_800),
                        )
                        .selectable(true)
                        .wrap(),
                    );
                });
            });
        });
}

pub(super) fn environment_variable_source_cell(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let source = environment_variable_source_label(variable);
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM - 2.0) as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let response = ui.add(
                    egui::Label::new(
                        egui::RichText::new(&source)
                            .font(typography::metadata())
                            .color(gray::_600),
                    )
                    .wrap()
                    .sense(egui::Sense::click()),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        response.enabled(),
                        "Copy environment variable source",
                    )
                });
                if response.clicked() {
                    ui.ctx().copy_text(source);
                }
            });
        });
}

pub(super) fn environment_variable_secret_revealed(
    ui: &egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> bool {
    ui.data(|data| {
        data.get_temp::<bool>(environment_variable_secret_id(ui, variable))
            .unwrap_or(false)
    })
}

pub(super) fn environment_variable_secret_id(
    _ui: &egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> egui::Id {
    egui::Id::new((
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
