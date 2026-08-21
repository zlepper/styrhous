mod cluster_rail;
mod dialogs;
mod global_blade;
mod helm_releases;
mod log_state;
#[doc(hidden)]
pub mod log_viewer_profile;
mod log_windows;
mod persistence;
mod resource_actions;
mod resource_detail;
mod resource_navigation;
mod resource_owner;
mod resource_table_settings;
mod settings;
pub(crate) mod state;
mod table_preferences;
mod widgets;
mod workspace;
mod yaml_editor;
#[cfg(any(test, feature = "benchmarks"))]
#[doc(hidden)]
pub mod yaml_editor_profile;

use crate::log_store::LogStoreService;
use crate::terminal_launcher::{
    ShellRequest, SystemTerminalLauncher, TerminalLaunchSettings, TerminalLauncher,
};
use crate::worker::{Worker, WorkerTrait};
use components::{apply_light_theme, scroll};
use dialogs::{
    show_bulk_delete_confirmation, show_bulk_delete_error, show_cron_job_run_confirmation,
    show_cron_job_run_error, show_delete_confirmation, show_deployment_restart_confirmation,
    show_deployment_restart_error, show_force_delete_confirmation, show_force_delete_error,
    show_scale_dialog, show_scale_error, show_terminal_launch_error,
};
use state::{LogDisplayOptions, PersistedClusterSelections, ResourceNavigationExpansion, UiState};
use table_preferences::PersistedResourceTablePreferences;

const CLUSTER_SELECTIONS_STORAGE_KEY: &str = "cluster_selections";
const LOG_DISPLAY_OPTIONS_STORAGE_KEY: &str = "log_display_options";
const RESOURCE_NAVIGATION_EXPANSION_STORAGE_KEY: &str = "resource_navigation_expansion";
const TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY: &str = "terminal_launch_settings";
const RESOURCE_TABLE_PREFERENCES_STORAGE_KEY: &str = "resource_table_preferences";

pub struct MyEguiApp<W: WorkerTrait = Worker, L: TerminalLauncher = SystemTerminalLauncher> {
    worker: W,
    terminal_launcher: L,
    terminal_launch_settings: TerminalLaunchSettings,
    resource_table_preferences: PersistedResourceTablePreferences,
    ui_state: UiState,
    log_store: LogStoreService,
    updater: crate::updater::UpdaterService,
}

impl<W: WorkerTrait, L: TerminalLauncher> Default for MyEguiApp<W, L> {
    fn default() -> Self {
        let log_store = LogStoreService::default();
        let mut worker = W::default();
        worker.set_log_store_appender(log_store.appender());
        Self {
            worker,
            terminal_launcher: L::default(),
            terminal_launch_settings: TerminalLaunchSettings::default(),
            resource_table_preferences: PersistedResourceTablePreferences::default(),
            ui_state: UiState::default(),
            log_store,
            updater: crate::updater::UpdaterService::default(),
        }
    }
}

impl<W: WorkerTrait, L: TerminalLauncher> MyEguiApp<W, L> {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::new_with_updater(cc, crate::updater::UpdaterService::start())
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(cc: &eframe::CreationContext<'_>) -> Self {
        let mut updater = crate::updater::UpdaterService::default();
        updater.set_status_for_test(crate::updater::UpdateStatus::LocalBuild);
        Self::new_with_updater(cc, updater)
    }

    fn new_with_updater(
        cc: &eframe::CreationContext<'_>,
        updater: crate::updater::UpdaterService,
    ) -> Self {
        configure_egui_context(&cc.egui_ctx);
        let log_store = LogStoreService::with_repaint_context(cc.egui_ctx.clone());
        let mut worker = W::with_repaint_context(cc.egui_ctx.clone());
        worker.set_log_store_appender(log_store.appender());
        let mut app = Self {
            worker,
            terminal_launcher: L::default(),
            terminal_launch_settings: TerminalLaunchSettings::default(),
            resource_table_preferences: PersistedResourceTablePreferences::default(),
            ui_state: UiState::default(),
            log_store,
            updater,
        };
        app.load_persisted_state(cc.storage);
        app
    }

    fn load_persisted_state(&mut self, storage: Option<&dyn eframe::Storage>) {
        self.ui_state.log_display_options = storage
            .and_then(|storage| {
                eframe::get_value::<LogDisplayOptions>(storage, LOG_DISPLAY_OPTIONS_STORAGE_KEY)
            })
            .unwrap_or_default();
        self.terminal_launch_settings = storage
            .and_then(|storage| {
                eframe::get_value::<TerminalLaunchSettings>(
                    storage,
                    TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY,
                )
            })
            .unwrap_or_default();
        self.ui_state.cluster_selections = storage
            .and_then(|storage| {
                eframe::get_value::<PersistedClusterSelections>(
                    storage,
                    CLUSTER_SELECTIONS_STORAGE_KEY,
                )
            })
            .unwrap_or_default();
        self.ui_state.resource_navigation_expansion = storage
            .and_then(|storage| {
                eframe::get_value::<ResourceNavigationExpansion>(
                    storage,
                    RESOURCE_NAVIGATION_EXPANSION_STORAGE_KEY,
                )
            })
            .unwrap_or_default();
        self.resource_table_preferences = storage
            .and_then(|storage| {
                eframe::get_value::<PersistedResourceTablePreferences>(
                    storage,
                    RESOURCE_TABLE_PREFERENCES_STORAGE_KEY,
                )
            })
            .unwrap_or_default();
    }
}

/// Configure the one egui context shared by the main and child native windows.
fn configure_egui_context(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    apply_light_theme(ctx);
    scroll::configure_input(ctx);
}

impl<W: WorkerTrait, L: TerminalLauncher> eframe::App for MyEguiApp<W, L> {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.updater.poll();
        self.worker.start();
        let mut commands_to_send = self.ui_state.update(&mut self.worker);
        while let Some(result) = self.log_store.try_next_result() {
            if let crate::log_store::LogStoreResult::Failed { window_id, .. } = &result
                && let Some(window) = self.ui_state.log_windows.get(window_id)
                && !matches!(window.status, state::PodLogStatus::Failed(_))
            {
                commands_to_send.push(Box::new(crate::worker::StopPodLogStream {
                    cluster_key: window.cluster_key,
                    log_window_id: *window_id,
                }));
            }
            self.ui_state.apply_log_store_result(result);
        }

        for command in commands_to_send {
            self.worker.send_command(command);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut commands_to_send = Vec::new();
        let mut shell_requests = Vec::<ShellRequest>::new();
        let debug_image_presets = self.terminal_launch_settings.debug_image_presets.clone();

        cluster_rail::show(
            ui,
            &mut self.ui_state,
            &mut commands_to_send,
            &self.terminal_launch_settings,
            self.updater.status(),
        );
        let clicked_api_resource = resource_navigation::show(ui, &mut self.ui_state);
        yaml_editor::show(&ctx, &mut self.ui_state, &mut commands_to_send);
        workspace::show(
            ui,
            &mut self.ui_state,
            &mut commands_to_send,
            &mut shell_requests,
            &debug_image_presets,
            &mut self.resource_table_preferences,
        );
        global_blade::show(
            &ctx,
            &mut self.ui_state,
            &mut commands_to_send,
            &mut shell_requests,
            &debug_image_presets,
            &mut self.resource_table_preferences,
            &mut self.terminal_launch_settings,
            self.updater.status(),
        );
        log_windows::show(
            &ctx,
            &mut self.ui_state,
            &self.log_store,
            &mut commands_to_send,
        );
        show_delete_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_bulk_delete_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_force_delete_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_deployment_restart_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_cron_job_run_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_scale_dialog(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_terminal_launch_error(
            &ctx,
            &mut self.ui_state,
            &self.terminal_launch_settings,
            &mut commands_to_send,
        );
        show_deployment_restart_error(&ctx, &mut self.ui_state);
        show_cron_job_run_error(&ctx, &mut self.ui_state);
        show_bulk_delete_error(&ctx, &mut self.ui_state);
        show_force_delete_error(&ctx, &mut self.ui_state);
        show_scale_error(&ctx, &mut self.ui_state);

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
            CLUSTER_SELECTIONS_STORAGE_KEY,
            &self.ui_state.cluster_selections,
        );
        eframe::set_value(
            storage,
            RESOURCE_NAVIGATION_EXPANSION_STORAGE_KEY,
            &self.ui_state.resource_navigation_expansion,
        );
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
        eframe::set_value(
            storage,
            RESOURCE_TABLE_PREFERENCES_STORAGE_KEY,
            &self.resource_table_preferences,
        );
    }

    fn persist_egui_memory(&self) -> bool {
        // Persist only the app settings explicitly written in `save`. Egui's complete memory
        // includes `Area` z-ordering, which can leave a stale overlay layer above a later blade.
        false
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod persistence_tests;
