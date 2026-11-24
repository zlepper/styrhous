mod blade_text;
pub(crate) mod clock;
mod cluster_landing;
mod cluster_roster;
mod cluster_switcher;
mod dialogs;
mod global_blade;
mod helm_releases;
mod log_state;
#[doc(hidden)]
pub mod log_viewer_profile;
mod log_windows;
mod metadata_fields;
mod namespace_selector;
mod notification_history;
mod operation_outcome;
mod operation_toasts;
mod persistence;
mod preference_table;
mod resource_actions;
mod resource_detail;
mod resource_owner;
mod resource_table_cache;
#[cfg(any(test, feature = "benchmarks"))]
#[doc(hidden)]
pub mod resource_table_profile;
mod resource_table_settings;
mod save_file_dialog;
mod settings;
mod sidebar;
pub(crate) mod state;
mod table_preferences;
mod theme_file;
mod widgets;
mod workspace;
mod yaml_editor;
#[cfg(any(test, feature = "benchmarks"))]
#[doc(hidden)]
pub mod yaml_editor_profile;

use crate::licensing::{LicensingService, LicensingSettings};
use crate::log_store::LogStoreService;
use crate::terminal_launcher::{
    ShellRequest, SystemTerminalLauncher, TerminalLaunchSettings, TerminalLauncher,
};
use crate::worker::{Worker, WorkerTrait};
use components::design::size;
use components::design::theme::{self, configure_default};
use components::scroll;
use dialogs::{
    show_bulk_delete_confirmation, show_cron_job_run_confirmation, show_delete_confirmation,
    show_deployment_restart_confirmation, show_deployment_rollback_dialog,
    show_force_delete_confirmation, show_helm_rollback_confirmation, show_log_source_confirmation,
    show_node_drain_confirmation, show_scale_dialog, show_terminal_launch_error,
};
use namespace_selector::NamespaceSelectorSettings;
use save_file_dialog::{SaveFileDialog, SystemSaveFileDialog};
use state::{
    LogDisplayOptions, PersistedApiDiscovery, PersistedClusterSelections, PinnedKinds,
    ResourceNavigationExpansion, UiState,
};
use table_preferences::PersistedResourceTablePreferences;
use theme_file::{ThemeFileChange, ThemeFileWatcher};

const CLUSTER_SELECTIONS_STORAGE_KEY: &str = "cluster_selections";
const API_DISCOVERY_CACHE_STORAGE_KEY: &str = "api_discovery_cache";
const LOG_DISPLAY_OPTIONS_STORAGE_KEY: &str = "log_display_options";
const RESOURCE_NAVIGATION_EXPANSION_STORAGE_KEY: &str = "resource_navigation_expansion";
const TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY: &str = "terminal_launch_settings";
const RESOURCE_TABLE_PREFERENCES_STORAGE_KEY: &str = "resource_table_preferences";
const NAMESPACE_SELECTOR_SETTINGS_STORAGE_KEY: &str = "namespace_selector_settings";
const LICENSING_SETTINGS_STORAGE_KEY: &str = "licensing_settings";
const PINNED_KINDS_STORAGE_KEY: &str = "pinned_kinds";
const CLUSTER_ROSTER_STORAGE_KEY: &str = "cluster_roster";

pub struct MyEguiApp<W: WorkerTrait = Worker, L: TerminalLauncher = SystemTerminalLauncher> {
    worker: W,
    terminal_launcher: L,
    save_file_dialog: Box<dyn SaveFileDialog>,
    terminal_launch_settings: TerminalLaunchSettings,
    resource_table_preferences: PersistedResourceTablePreferences,
    namespace_selector_settings: NamespaceSelectorSettings,
    ui_state: UiState,
    log_store: LogStoreService,
    updater: crate::updater::UpdaterService,
    licensing: LicensingService,
    /// The user theme file, or `None` in tests and on platforms without a
    /// data directory.
    theme_file: Option<ThemeFileWatcher>,
}

impl<W: WorkerTrait, L: TerminalLauncher> Default for MyEguiApp<W, L> {
    fn default() -> Self {
        let log_store = LogStoreService::default();
        let mut worker = W::default();
        worker.set_log_store_appender(log_store.appender());
        Self {
            worker,
            terminal_launcher: L::default(),
            save_file_dialog: Box::new(SystemSaveFileDialog::default()),
            terminal_launch_settings: TerminalLaunchSettings::default(),
            resource_table_preferences: PersistedResourceTablePreferences::default(),
            namespace_selector_settings: NamespaceSelectorSettings::default(),
            ui_state: UiState::default(),
            log_store,
            updater: crate::updater::UpdaterService::default(),
            licensing: LicensingService::default(),
            theme_file: None,
        }
    }
}

impl<W: WorkerTrait, L: TerminalLauncher> MyEguiApp<W, L> {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // The updater starts before the rest of the stored state is read, so it
        // needs the stored choice to look for updates.
        let check_for_updates = cc
            .storage
            .and_then(|storage| {
                eframe::get_value::<TerminalLaunchSettings>(
                    storage,
                    TERMINAL_LAUNCH_SETTINGS_STORAGE_KEY,
                )
            })
            .is_none_or(|settings| settings.check_for_updates);
        Self::new_with_updater(cc, crate::updater::UpdaterService::start(check_for_updates))
    }

    #[cfg(test)]
    pub(crate) fn set_save_file_dialog(&mut self, dialog: impl SaveFileDialog + 'static) {
        self.save_file_dialog = Box::new(dialog);
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(cc: &eframe::CreationContext<'_>) -> Self {
        let mut updater = crate::updater::UpdaterService::default();
        updater.set_status_for_test(crate::updater::UpdateStatus::LocalBuild);
        let mut app = Self::new_with_updater(cc, updater);
        // Never open a real dialog from a test; tests that export install their own.
        app.set_save_file_dialog(
            save_file_dialog::test_support::MockSaveFileDialog::answering(
                save_file_dialog::SaveFileChoice::Cancelled,
            ),
        );
        // Never read the user's theme file from a test.
        app.theme_file = None;
        app.licensing
            .set_status_for_test(crate::licensing::LicenseStatus::Licensed(
                test_commercial_lease(),
            ));
        app
    }

    fn new_with_updater(
        cc: &eframe::CreationContext<'_>,
        updater: crate::updater::UpdaterService,
    ) -> Self {
        configure_egui_context(&cc.egui_ctx);
        let log_store = LogStoreService::with_repaint_context(cc.egui_ctx.clone());
        let mut worker = W::with_repaint_context(cc.egui_ctx.clone());
        worker.set_log_store_appender(log_store.appender());
        let licensing_settings = cc
            .storage
            .and_then(|storage| {
                eframe::get_value::<LicensingSettings>(storage, LICENSING_SETTINGS_STORAGE_KEY)
            })
            .unwrap_or_default();
        let mut app = Self {
            worker,
            terminal_launcher: L::default(),
            save_file_dialog: Box::new(SystemSaveFileDialog::default()),
            terminal_launch_settings: TerminalLaunchSettings::default(),
            resource_table_preferences: PersistedResourceTablePreferences::default(),
            namespace_selector_settings: NamespaceSelectorSettings::default(),
            ui_state: UiState::default(),
            log_store,
            updater,
            licensing: LicensingService::start(licensing_settings, cc.egui_ctx.clone()),
            theme_file: ThemeFileWatcher::in_data_dir(crate::APP_NAME),
        };
        app.ui_state.kubeconfig_source =
            crate::cluster_connection_manager::kubeconfig::kubeconfig_source_label();
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
        self.ui_state.api_discovery_cache = storage
            .and_then(|storage| {
                eframe::get_value::<PersistedApiDiscovery>(storage, API_DISCOVERY_CACHE_STORAGE_KEY)
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
        self.ui_state.pinned_kinds = storage
            .and_then(|storage| eframe::get_value::<PinnedKinds>(storage, PINNED_KINDS_STORAGE_KEY))
            .unwrap_or_default();
        self.ui_state.cluster_roster = storage
            .and_then(|storage| {
                eframe::get_value::<cluster_roster::ClusterRoster>(
                    storage,
                    CLUSTER_ROSTER_STORAGE_KEY,
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
        self.namespace_selector_settings = storage
            .and_then(|storage| {
                eframe::get_value::<NamespaceSelectorSettings>(
                    storage,
                    NAMESPACE_SELECTOR_SETTINGS_STORAGE_KEY,
                )
            })
            .unwrap_or_default();
    }
}

/// How long an invalid theme file's error stays on screen.
const THEME_FILE_ERROR_TOAST_DURATION: std::time::Duration = std::time::Duration::from_secs(12);

impl<W: WorkerTrait, L: TerminalLauncher> MyEguiApp<W, L> {
    /// Apply changes to the user theme file, and keep polling it.
    fn poll_theme_file(&mut self, ctx: &egui::Context) {
        let Some(watcher) = &mut self.theme_file else {
            return;
        };
        let now = std::time::Instant::now();
        match watcher.poll(now) {
            Some(ThemeFileChange::Loaded(loaded) | ThemeFileChange::Removed(loaded)) => {
                tracing::info!(
                    "Using theme \"{}\" from {}",
                    loaded.name(),
                    watcher.path().display()
                );
                theme::switch(ctx, loaded);
            }
            Some(ThemeFileChange::Failed(message)) => {
                tracing::warn!(
                    "Ignoring theme file {}: {message}",
                    watcher.path().display()
                );
                self.ui_state.push_application_toast(
                    state::OperationOutcome::Failure,
                    "Couldn’t apply the theme file",
                    message,
                    THEME_FILE_ERROR_TOAST_DURATION,
                );
            }
            None => {}
        }
        ctx.request_repaint_after(watcher.time_until_next_check(now));
    }

    #[cfg(test)]
    pub(crate) fn watch_theme_file(&mut self, path: impl Into<std::path::PathBuf>) {
        self.theme_file = Some(ThemeFileWatcher::new(path));
    }
}

/// Configure the one egui context shared by the main and child native windows.
fn configure_egui_context(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    configure_default(ctx);
    scroll::configure_input(ctx);
}

impl<W: WorkerTrait, L: TerminalLauncher> eframe::App for MyEguiApp<W, L> {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_theme_file(ctx);
        self.updater.poll();
        self.licensing.poll(ctx);
        self.worker.start();
        self.ui_state.now = ctx.input(|input| input.time);
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
        for answer in self.save_file_dialog.poll() {
            self.ui_state
                .answer_log_export_dialog(answer, &self.log_store);
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

        clock::publish(&ctx, self.ui_state.clock);
        let banner = show_license_banner(
            ui,
            &mut self.ui_state,
            &mut commands_to_send,
            &self.licensing,
        );
        self.ui_state
            .global_blades
            .set_top_inset(if banner { size::BANNER } else { 0.0 });
        cluster_switcher::note_known_nodes(&mut self.ui_state);
        let other_modal = self.ui_state.cluster_switcher.is_none()
            && ctx.memory(|memory| memory.top_modal_layer().is_some());
        if !other_modal
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::K))
        {
            self.ui_state.toggle_cluster_switcher();
        }
        cluster_landing::quick_switch(&ctx, &mut self.ui_state, &mut commands_to_send);
        sidebar::show(
            ui,
            &mut self.ui_state,
            &mut commands_to_send,
            self.updater.status(),
            self.licensing.status(),
        );
        yaml_editor::show(&ctx, &mut self.ui_state, &mut commands_to_send);
        workspace::show(
            ui,
            &mut self.ui_state,
            &mut commands_to_send,
            &mut shell_requests,
            &self.terminal_launch_settings,
            &mut self.resource_table_preferences,
            &self.namespace_selector_settings,
        );
        global_blade::show(
            &ctx,
            &mut self.ui_state,
            &mut commands_to_send,
            &mut shell_requests,
            &debug_image_presets,
            &mut self.resource_table_preferences,
            &mut self.terminal_launch_settings,
            &mut self.namespace_selector_settings,
            self.updater.status(),
            &mut self.licensing,
        );
        log_windows::show(
            &ctx,
            &mut self.ui_state,
            &self.log_store,
            &mut commands_to_send,
        );
        for request in self
            .ui_state
            .log_windows
            .values_mut()
            .filter_map(state::PodLogWindowState::take_save_request)
        {
            self.save_file_dialog.request(&ctx, request);
        }
        show_delete_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_bulk_delete_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_log_source_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_force_delete_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_deployment_restart_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_cron_job_run_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_node_drain_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_helm_rollback_confirmation(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_deployment_rollback_dialog(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_scale_dialog(&ctx, &mut self.ui_state, &mut commands_to_send);
        show_terminal_launch_error(
            &ctx,
            &mut self.ui_state,
            &self.terminal_launch_settings,
            &mut commands_to_send,
        );
        operation_toasts::show(&ctx, &mut self.ui_state);
        cluster_switcher::show(&ctx, &mut self.ui_state, &mut commands_to_send);

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
            API_DISCOVERY_CACHE_STORAGE_KEY,
            &self.ui_state.api_discovery_cache,
        );
        eframe::set_value(
            storage,
            RESOURCE_NAVIGATION_EXPANSION_STORAGE_KEY,
            &self.ui_state.resource_navigation_expansion,
        );
        eframe::set_value(
            storage,
            PINNED_KINDS_STORAGE_KEY,
            &self.ui_state.pinned_kinds,
        );
        eframe::set_value(
            storage,
            CLUSTER_ROSTER_STORAGE_KEY,
            &self.ui_state.cluster_roster,
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
        eframe::set_value(
            storage,
            NAMESPACE_SELECTOR_SETTINGS_STORAGE_KEY,
            &self.namespace_selector_settings,
        );
        eframe::set_value(
            storage,
            LICENSING_SETTINGS_STORAGE_KEY,
            self.licensing.settings(),
        );
    }

    fn persist_egui_memory(&self) -> bool {
        // Persist only the app settings explicitly written in `save`. Egui's complete memory
        // includes `Area` z-ordering, which can leave a stale overlay layer above a later blade.
        false
    }
}

/// The licence warning (A11) across the window's top until it is dismissed
/// for the session (it returns when the warning changes). Returns whether it
/// shows.
fn show_license_banner(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<crate::worker::WorkerCommandBox>,
    licensing: &LicensingService,
) -> bool {
    if !licensing.status().shows_warning() {
        // A later warning with the same text is a new one; a check in progress
        // (which every refresh passes through) is neither.
        if !matches!(
            licensing.status(),
            crate::licensing::LicenseStatus::Checking
        ) {
            ui_state.dismissed_license_warning = None;
        }
        return false;
    }
    let summary = licensing.status().summary();
    if ui_state.dismissed_license_warning.as_ref() == Some(&summary) {
        return false;
    }
    let mut response = components::BannerResponse::default();
    egui::Panel::top("license-status-banner")
        .exact_size(size::BANNER)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            // A licence that no longer works outranks one that needs attention soon.
            let tone = match licensing.status() {
                crate::licensing::LicenseStatus::Revoked
                | crate::licensing::LicenseStatus::Failed { .. } => {
                    components::design::theme::Tone::Danger
                }
                _ => components::design::theme::Tone::Warning,
            };
            response = components::Banner::new(tone, &summary)
                .action("Manage license")
                .dismissible(true)
                .show(ui);
        });
    if response.action {
        ui_state.open_license_settings(commands_to_send);
    }
    if response.dismissed {
        ui_state.dismissed_license_warning = Some(summary);
    }
    true
}

#[cfg(test)]
fn test_commercial_lease() -> crate::licensing::VerifiedLease {
    crate::licensing::VerifiedLease {
        user_id: uuid::Uuid::now_v7(),
        seat_id: uuid::Uuid::now_v7(),
        billing_account_id: uuid::Uuid::now_v7(),
        installation_id: uuid::Uuid::now_v7(),
        activation_id: uuid::Uuid::now_v7(),
        state: "commercial".into(),
        reason_code: "active_subscription".into(),
        expires_at: u64::MAX,
        refresh_after: u64::MAX,
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod persistence_tests;
