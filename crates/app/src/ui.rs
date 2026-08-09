mod cluster_rail;
mod dialogs;
mod log_windows;
mod resource_actions;
mod resource_detail;
mod resource_navigation;
mod settings;
mod state;
mod widgets;
mod workspace;
mod yaml_editor;

#[cfg(test)]
pub(super) const APP_SNAPSHOT_SIZE: egui::Vec2 = egui::vec2(1536.0, 1024.0);

use crate::log_store::LogStoreService;
use crate::terminal_launcher::{
    PodShellRequest, SystemTerminalLauncher, TerminalLaunchSettings, TerminalLauncher,
};
use crate::worker::{Worker, WorkerTrait};
use components::apply_light_theme;
use dialogs::{show_delete_confirmation, show_terminal_launch_error};
use state::{LogDisplayOptions, UiState};

const LOG_DISPLAY_OPTIONS_STORAGE_KEY: &str = "log_display_options";
const TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY: &str = "terminal_launch_settings";

pub struct MyEguiApp<W: WorkerTrait = Worker, L: TerminalLauncher = SystemTerminalLauncher> {
    worker: W,
    terminal_launcher: L,
    terminal_launch_settings: TerminalLaunchSettings,
    ui_state: UiState,
    log_store: LogStoreService,
}

impl<W: WorkerTrait, L: TerminalLauncher> Default for MyEguiApp<W, L> {
    fn default() -> Self {
        Self {
            worker: W::default(),
            terminal_launcher: L::default(),
            terminal_launch_settings: TerminalLaunchSettings::default(),
            ui_state: UiState::default(),
            log_store: LogStoreService::default(),
        }
    }
}

impl<W: WorkerTrait, L: TerminalLauncher> MyEguiApp<W, L> {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        apply_light_theme(&cc.egui_ctx);
        let mut app = Self::default();
        app.ui_state.log_display_options = cc
            .storage
            .and_then(|storage| {
                eframe::get_value::<LogDisplayOptions>(storage, LOG_DISPLAY_OPTIONS_STORAGE_KEY)
            })
            .unwrap_or_default();
        app.terminal_launch_settings = cc
            .storage
            .and_then(|storage| {
                eframe::get_value::<TerminalLaunchSettings>(
                    storage,
                    TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY,
                )
            })
            .unwrap_or_default();
        app
    }
}

impl<W: WorkerTrait, L: TerminalLauncher> eframe::App for MyEguiApp<W, L> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.worker.start();
        let mut commands_to_send = self.ui_state.update(&mut self.worker, &self.log_store);
        let mut shell_requests = Vec::<PodShellRequest>::new();
        while let Some(result) = self.log_store.try_next_result() {
            if let crate::log_store::LogStoreResult::Failed { window_id, .. } = &result
                && let Some(window) = self.ui_state.log_windows.get(window_id)
                && !matches!(window.status, state::PodLogStatus::Failed(_))
            {
                commands_to_send.push(crate::worker::WorkerCommand::StopPodLogStream {
                    cluster_key: window.cluster_key,
                    log_window_id: *window_id,
                });
            }
            self.ui_state.apply_log_store_result(result);
        }

        cluster_rail::show(
            ctx,
            &mut self.ui_state,
            &mut commands_to_send,
            &self.terminal_launch_settings,
        );
        let clicked_api_resource = resource_navigation::show(ctx, &self.ui_state);
        yaml_editor::show(ctx, &mut self.ui_state, &mut commands_to_send);
        workspace::show(
            ctx,
            &mut self.ui_state,
            &mut commands_to_send,
            &mut shell_requests,
        );
        resource_detail::show(
            ctx,
            &mut self.ui_state,
            &mut commands_to_send,
            &mut shell_requests,
        );
        log_windows::show(
            ctx,
            &mut self.ui_state,
            &self.log_store,
            &mut commands_to_send,
        );
        show_delete_confirmation(ctx, &mut self.ui_state, &mut commands_to_send);
        settings::show(ctx, &mut self.ui_state, &mut self.terminal_launch_settings);
        show_terminal_launch_error(ctx, &mut self.ui_state, &self.terminal_launch_settings);

        if let (Some(cluster_key), Some(api_resource)) =
            (self.ui_state.selected_cluster, clicked_api_resource)
        {
            self.ui_state
                .select_api_resource(cluster_key, api_resource, &mut commands_to_send);
        }

        for command in commands_to_send {
            self.worker.send_command(command);
        }
        for request in shell_requests {
            if let Err(error) = self
                .terminal_launcher
                .launch(&request, &self.terminal_launch_settings)
            {
                self.ui_state.terminal_launch_error = Some(error);
            }
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(
            storage,
            LOG_DISPLAY_OPTIONS_STORAGE_KEY,
            &self.ui_state.log_display_options,
        );
        eframe::set_value(
            storage,
            TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY,
            &self.terminal_launch_settings,
        );
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryStorage(HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn log_display_options_round_trip_through_eframe_storage() {
        let expected = LogDisplayOptions {
            show_line_numbers: true,
            show_timestamps: true,
            render_ansi: false,
        };
        let mut storage = MemoryStorage::default();

        eframe::set_value(&mut storage, LOG_DISPLAY_OPTIONS_STORAGE_KEY, &expected);

        assert_eq!(
            eframe::get_value::<LogDisplayOptions>(&storage, LOG_DISPLAY_OPTIONS_STORAGE_KEY),
            Some(expected)
        );
    }

    #[test]
    fn terminal_launch_settings_round_trip_through_eframe_storage() {
        let expected = TerminalLaunchSettings {
            custom_template: Some("alacritty -e {command}".into()),
        };
        let mut storage = MemoryStorage::default();

        eframe::set_value(
            &mut storage,
            TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY,
            &expected,
        );

        assert_eq!(
            eframe::get_value::<TerminalLaunchSettings>(
                &storage,
                TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY
            ),
            Some(expected)
        );
    }
}
