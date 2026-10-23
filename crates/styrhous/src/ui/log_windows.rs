use super::state::{
    LogDisplayOptions, LogPageKey, LogTextPosition, LogTextSelection, PendingLogCaret,
    PodLogStatus, PodLogWindowState, UiState,
};
use crate::ansi::AnsiStyleSpan;
use crate::log_store::LogStoreService;
use crate::worker::{
    PodLogInstance, PodLogSourceFailed, PodLogSourceReconnecting, PodLogSourceRecovered,
    PodLogStreamEnded, PodLogStreamFailed, PodLogStreamStarted, StartPodLogStream,
    StopPodLogStream, WorkerCommandBox, WorkerResult,
};
use anstyle::{Ansi256Color, AnsiColor, Color, Effects, RgbColor, Style};
use components::colors::{SUCCESS, TABLE_BORDER, TOOLBAR_BACKGROUND, gray};
use components::design::{radius, search, spacing, status, surface, typography};
use components::{PointingHand, TailwindSearchInput, icons, search_navigation_button};
use std::time::{Duration, Instant};

const LOG_FONT_SIZE: f32 = 14.0;
const HORIZONTAL_OVERSCAN_POINTS: f32 = 120.0;
/// Scroll offsets are rasterized, so use one logical point when comparing the
/// rendered offset with the calculated end position.
const TAIL_BOTTOM_TOLERANCE_POINTS: f32 = 1.0;

impl WorkerResult for PodLogStreamStarted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id)
            && matches!(window.status, PodLogStatus::Connecting)
        {
            window.status = match window.instance {
                PodLogInstance::Current => PodLogStatus::Following,
                PodLogInstance::Previous => PodLogStatus::Loading,
            };
        }
    }
}

impl WorkerResult for PodLogStreamEnded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id)
            && !matches!(window.status, PodLogStatus::Failed(_))
        {
            window.status = PodLogStatus::Finished;
            window.source_reconnects.clear();
        }
    }
}

impl WorkerResult for PodLogStreamFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id) {
            window.status = PodLogStatus::Failed(self.error);
            window.source_reconnects.clear();
        }
    }
}

impl WorkerResult for PodLogSourceFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id) {
            window.source_reconnects.remove(&self.target);
            window
                .source_failures
                .entry(self.target)
                .and_modify(|existing| {
                    if !existing.contains(&self.error) {
                        existing.push('\n');
                        existing.push_str(&self.error);
                    }
                })
                .or_insert(self.error);
        }
    }
}

impl WorkerResult for PodLogSourceReconnecting {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id) {
            window.source_reconnects.insert(self.target, self.error);
        }
    }
}

impl WorkerResult for PodLogSourceRecovered {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id) {
            window.source_reconnects.remove(&self.target);
        }
    }
}

/// Render native, independent Pod log windows and stop both the Kubernetes
/// stream and the independent disk store when a window is closed.
pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    log_store: &LogStoreService,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    let ids = ui_state.log_windows.keys().copied().collect::<Vec<_>>();
    for id in ids {
        let (log_windows, display_options) =
            (&mut ui_state.log_windows, &mut ui_state.log_display_options);
        let Some(window) = log_windows.get_mut(&id) else {
            continue;
        };
        if !window.store_opened {
            window.store_opened = log_store.open(id);
        }
        let viewport_id = egui::ViewportId::from_hash_of(("pod-log-window", window.viewport_id));
        let mut title = if window.has_multiple_sources() {
            format!("Logs · {} sources", window.targets.len())
        } else {
            let target = window
                .targets
                .first()
                .expect("log windows always contain one source");
            format!(
                "Logs · {}/{} · {}",
                target.namespace, target.pod_name, target.container
            )
        };
        if window.instance == PodLogInstance::Previous {
            title.push_str(" · Previous");
        }
        let mut close_requested = false;
        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(crate::DEFAULT_NATIVE_WINDOW_SIZE)
                .with_min_inner_size(crate::MIN_NATIVE_WINDOW_SIZE),
            |window_ui, _| {
                let window_ctx = window_ui.ctx().clone();
                close_requested = window_ctx.input(|input| input.viewport().close_requested());
                show_log_window(
                    window_ui,
                    window,
                    display_options,
                    log_store,
                    &mut close_requested,
                );
            },
        );
        window.close_requested |= close_requested;
    }

    let closed = ui_state
        .log_windows
        .values()
        .filter(|window| window.close_requested)
        .map(|window| (window.id, window.cluster_key))
        .collect::<Vec<_>>();
    for (id, cluster_key) in closed {
        ui_state.log_windows.remove(&id);
        log_store.close(id);
        commands_to_send.push(Box::new(StopPodLogStream {
            cluster_key,
            log_window_id: id,
        }));
    }

    let switches = ui_state
        .log_windows
        .values_mut()
        .filter_map(|window| {
            window
                .requested_instance
                .take()
                .map(|instance| (window.id, instance))
        })
        .collect::<Vec<_>>();
    for (id, instance) in switches {
        ui_state.switch_log_window_instance(id, instance, log_store, commands_to_send);
        ctx.request_repaint();
    }
}

impl UiState {
    fn switch_log_window_instance(
        &mut self,
        id: u64,
        instance: PodLogInstance,
        log_store: &LogStoreService,
        commands_to_send: &mut Vec<WorkerCommandBox>,
    ) {
        let Some(old) = self.log_windows.remove(&id) else {
            return;
        };
        if old.instance == instance {
            self.log_windows.insert(id, old);
            return;
        }

        self.next_log_window_id += 1;
        let new_id = self.next_log_window_id;
        let mut replacement = PodLogWindowState::new(new_id, old.cluster_key, old.targets.clone())
            .expect("an existing log window has at least one target");
        replacement.viewport_id = old.viewport_id;
        replacement.instance = instance;
        replacement.show_source_labels = old.show_source_labels;
        replacement.search.query = old.search.query;
        replacement.search.regex_mode = old.search.regex_mode;
        replacement.search.filter_matches = old.search.filter_matches;
        if !replacement.search.query.is_empty() {
            replacement.search.search_deadline = Some(Instant::now());
            replacement.search.search_complete = false;
        }

        log_store.close(id);
        commands_to_send.push(Box::new(StopPodLogStream {
            cluster_key: old.cluster_key,
            log_window_id: id,
        }));
        commands_to_send.push(Box::new(StartPodLogStream {
            cluster_key: old.cluster_key,
            log_window_id: new_id,
            targets: old.targets,
            instance,
        }));
        self.log_windows.insert(new_id, replacement);
    }
}

pub(super) fn show_log_window(
    ui: &mut egui::Ui,
    window: &mut PodLogWindowState,
    display_options: &mut LogDisplayOptions,
    log_store: &LogStoreService,
    _close_requested: &mut bool,
) {
    let _ =
        show_log_window_with_scroll_state(ui, window, display_options, log_store, _close_requested);
}

mod caret;
mod layout;
mod navigation;
#[path = "log_windows/search.rs"]
mod search_controls;
mod viewer;

use caret::*;
use layout::*;
use navigation::*;
use search_controls::*;
use viewer::*;

#[cfg(test)]
mod tests;
