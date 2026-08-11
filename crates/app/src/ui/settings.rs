use super::state::UiState;
use crate::terminal_launcher::TerminalLaunchSettings;
use components::colors::{WHITE, gray, indigo};
use components::design::{radius, spacing, status, surface, typography};
use components::{BladeNavigator, BladeStack, ButtonSize, PointingHand, TailwindButton};

const FOOTER_HEIGHT: f32 = 52.0;
const CHOICE_CONTENT_MIN_HEIGHT: f32 = 44.0;

/// Render application settings as a first-class workspace blade rather than a
/// transient native dialog, so its controls have room for explanation.
pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    settings: &mut TerminalLaunchSettings,
) {
    if !ui_state.terminal_settings_open {
        return;
    }

    let mut close = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let mut save = false;
    let mut reset = false;
    let stack = BladeStack::new("settings-blade");
    let mut blade = ui_state
        .terminal_settings_blade
        .take()
        .unwrap_or_else(|| BladeNavigator::new(()));
    let response = stack.show_with_title(
        ctx,
        &mut blade,
        |_| "Settings".to_owned(),
        |ui, _, _| {
            show_settings_introduction(ui);
            ui.add_space(spacing::XL);
            ui.separator();
            ui.add_space(spacing::XL);

            let content_height = (ui.available_height() - FOOTER_HEIGHT).max(120.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), content_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    components::scroll::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| show_terminal_launcher(ui, ui_state));
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
        },
    );
    close |= response.dismissed;

    if reset {
        ui_state.terminal_settings_draft = TerminalLaunchSettings::default();
        ui_state.terminal_settings_error = None;
    }
    if save {
        match ui_state.terminal_settings_draft.validate() {
            Ok(()) => {
                *settings = ui_state.terminal_settings_draft.clone();
                ui_state.terminal_settings_error = None;
                close = true;
            }
            Err(error) => ui_state.terminal_settings_error = Some(error),
        }
    }
    if close && blade.begin_close() {
        stack.seed_transition(ctx, &mut blade);
    }
    if response.close_finished {
        ui_state.terminal_settings_open = false;
        ui_state.terminal_settings_blade = None;
    } else {
        ui_state.terminal_settings_blade = Some(blade);
    }
}

fn show_settings_introduction(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("APPLICATION")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(spacing::XS);
    ui.label(
        egui::RichText::new(
            "Configure local tools and display preferences.\nThese settings apply only to this application on this device.",
        )
        .font(typography::body())
        .color(gray::_600),
    );
}

fn show_terminal_launcher(ui: &mut egui::Ui, ui_state: &mut UiState) {
    ui.label(
        egui::RichText::new("Terminal launcher")
            .font(typography::section_heading())
            .color(gray::_900),
    );
    ui.add_space(spacing::SM);
    ui.label(
        egui::RichText::new("Choose how pod shells open on this computer.")
            .font(typography::body())
            .color(gray::_600),
    );
    ui.add_space(spacing::XL);

    let automatic = ui_state.terminal_settings_draft.custom_template.is_none();
    if launcher_choice(
        ui,
        automatic,
        "Automatic",
        "Use your system’s preferred terminal.",
        None,
        false,
        None,
    ) {
        ui_state.terminal_settings_draft.custom_template = None;
        ui_state.terminal_settings_error = None;
    }
    ui.add_space(spacing::LG);
    let custom_launcher_clicked = {
        let template_error = ui_state.terminal_settings_error.clone();
        let template_invalid = template_error.is_some();
        let template = ui_state.terminal_settings_draft.custom_template.as_mut();
        launcher_choice(
            ui,
            !automatic,
            "Custom launcher",
            "Use a command template for your preferred terminal.",
            template,
            template_invalid,
            template_error.as_deref(),
        )
    };
    if custom_launcher_clicked && ui_state.terminal_settings_draft.custom_template.is_none() {
        ui_state.terminal_settings_draft.custom_template = Some(String::new());
    }
}

fn launcher_choice(
    ui: &mut egui::Ui,
    selected: bool,
    title: &str,
    description: &str,
    template: Option<&mut String>,
    template_invalid: bool,
    template_error: Option<&str>,
) -> bool {
    let stroke = if selected {
        egui::Stroke::new(1.0, indigo::_500)
    } else {
        surface::muted_border()
    };
    egui::Frame::new()
        .fill(if selected { indigo::_50 } else { WHITE })
        .stroke(stroke)
        .corner_radius(radius::surface())
        .inner_margin(egui::Margin::same(spacing::XXL as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let choice_clicked = ui
                .allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), CHOICE_CONTENT_MIN_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().icon_width = 20.0;
                        ui.spacing_mut().icon_width_inner = 12.0;
                        let response = ui.radio(selected, "").with_pointing_hand();
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::RadioButton,
                                ui.is_enabled(),
                                selected,
                                title,
                            )
                        });
                        ui.add_space(spacing::SM);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(title)
                                    .font(typography::semibold(18.0))
                                    .color(gray::_800),
                            );
                            ui.add_space(spacing::XS);
                            ui.label(
                                egui::RichText::new(description)
                                    .font(typography::body())
                                    .color(gray::_600),
                            );
                        });
                        response.clicked()
                    },
                )
                .inner;
            if let Some(template) = template {
                ui.add_space(spacing::XXL);
                ui.label(
                    egui::RichText::new("Command template")
                        .font(typography::body())
                        .color(gray::_800),
                );
                ui.add_space(spacing::SM);
                show_command_template_input(ui, template, template_invalid);
                ui.add_space(spacing::MD);
                egui::Frame::new()
                    .fill(indigo::_100)
                    .stroke(egui::Stroke::new(1.0, indigo::_200))
                    .corner_radius(radius::surface())
                    .inner_margin(egui::Margin::same(spacing::LG as i8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            let (icon_rect, _) = ui
                                .allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::hover());
                            ui.painter().circle_stroke(
                                icon_rect.center(),
                                9.0,
                                egui::Stroke::new(1.5, indigo::_600),
                            );
                            ui.painter().text(
                                icon_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "i",
                                typography::semibold(14.0),
                                indigo::_600,
                            );
                            ui.add_space(spacing::SM);
                            ui.label(template_guidance());
                        });
                    });
                if let Some(error) = template_error {
                    ui.add_space(spacing::LG);
                    show_validation_error(ui, error);
                }
            }
            choice_clicked
        })
        .inner
}

fn show_command_template_input(
    ui: &mut egui::Ui,
    template: &mut String,
    invalid: bool,
) -> egui::Response {
    let id = ui.make_persistent_id("terminal-command-template");
    let focused = ui.memory(|memory| memory.has_focus(id));
    let stroke = if invalid {
        egui::Stroke::new(1.0, status::DANGER)
    } else if focused {
        egui::Stroke::new(1.0, indigo::_500)
    } else {
        surface::control_border()
    };
    let response = egui::Frame::new()
        .fill(WHITE)
        .stroke(stroke)
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            spacing::MD as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            let full_width = ui.available_width() + spacing::XL;
            ui.set_min_width(full_width);
            ui.add_sized(
                egui::vec2(full_width, 20.0),
                egui::TextEdit::singleline(template)
                    .id(id)
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 2)))
                    .font(typography::monospace())
                    .text_color(gray::_800)
                    .hint_text("alacritty -e {command}"),
            )
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::TextEdit,
            ui.is_enabled(),
            "Command template",
        )
    });
    response
}

fn template_guidance() -> egui::text::LayoutJob {
    let mut guidance = egui::text::LayoutJob::default();
    let text = egui::TextFormat {
        font_id: typography::body(),
        color: indigo::_800,
        ..Default::default()
    };
    guidance.append("Use ", 0.0, text.clone());
    guidance.append(
        "{command}",
        0.0,
        egui::TextFormat {
            font_id: typography::monospace(),
            color: indigo::_900,
            ..Default::default()
        },
    );
    guidance.append(
        " as the placeholder.\nIt is replaced with the complete kubectl exec command.",
        0.0,
        text,
    );
    guidance
}

fn show_validation_error(ui: &mut egui::Ui, error: &str) {
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, status::DANGER))
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::same(spacing::MD as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new("Command template needs attention")
                    .font(typography::semibold(typography::BODY_SIZE))
                    .color(status::DANGER),
            );
            ui.add_space(spacing::XS);
            ui.label(
                egui::RichText::new(error)
                    .font(typography::metadata())
                    .color(gray::_700),
            );
        });
}
