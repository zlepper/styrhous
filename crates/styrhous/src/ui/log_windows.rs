use super::state::{
    LogDisplayOptions, LogPageKey, LogTextPosition, LogTextSelection, PendingLogCaret,
    PodLogStatus, PodLogWindowState, UiState,
};
use crate::ansi::AnsiStyleSpan;
use crate::log_store::LogStoreService;
use crate::worker::{
    PodLogStreamEnded, PodLogStreamFailed, PodLogStreamStarted, StopPodLogStream, WorkerCommandBox,
    WorkerResult,
};
use anstyle::{Ansi256Color, AnsiColor, Color, Effects, RgbColor, Style};
use components::colors::{SUCCESS, TABLE_BORDER, TOOLBAR_BACKGROUND, gray};
use components::design::{radius, search, spacing, status, surface, typography};
use components::{PointingHand, TailwindSearchInput, icons, search_navigation_button};
use std::time::{Duration, Instant};

const LOG_FONT_SIZE: f32 = 14.0;
const HORIZONTAL_OVERSCAN_POINTS: f32 = 120.0;

impl WorkerResult for PodLogStreamStarted {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id)
            && matches!(window.status, PodLogStatus::Connecting)
        {
            window.status = PodLogStatus::Following;
        }
    }
}

impl WorkerResult for PodLogStreamEnded {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id)
            && !matches!(window.status, PodLogStatus::Failed(_))
        {
            window.status = PodLogStatus::Finished;
        }
    }
}

impl WorkerResult for PodLogStreamFailed {
    fn apply(self, ui: &mut UiState, _commands: &mut Vec<WorkerCommandBox>) {
        if let Some(window) = ui.log_windows.get_mut(&self.log_window_id) {
            window.status = PodLogStatus::Failed(self.error);
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
        let viewport_id = egui::ViewportId::from_hash_of(("pod-log-window", id));
        let title = format!(
            "Logs · {}/{} · {}",
            window.namespace, window.pod_name, window.container.name
        );
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
