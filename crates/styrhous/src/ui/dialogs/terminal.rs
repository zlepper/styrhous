use super::*;

pub(crate) fn show_terminal_launch_error(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    settings: &TerminalLaunchSettings,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let Some(error) = ui_state.terminal_launch_error.clone() else {
        return;
    };
    match (ErrorDialog {
        id: egui::Id::new("terminal-launch-error"),
        eyebrow: "SHELL",
        title: "Couldn’t open a terminal",
        message: "Styrhous could not start an external terminal for this shell.",
        details: Some(&error),
        recovery: Some("Choose a custom command in Settings to use another installed terminal."),
        primary_action_label: Some("Open settings"),
    })
    .show(ctx)
    {
        ErrorDialogAction::PrimaryAction => {
            ui_state.open_terminal_settings(settings, commands_to_send);
            ui_state.terminal_launch_error = None;
        }
        ErrorDialogAction::Dismiss => ui_state.terminal_launch_error = None,
        ErrorDialogAction::None => {}
    }
}
