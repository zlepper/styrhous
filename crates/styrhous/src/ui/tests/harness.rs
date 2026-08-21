//! Semantic helpers for deterministic UI tests.
//!
//! Keep ordinary accessibility interactions in the test body. These helpers
//! cover only state setup and worker delivery, where the implementation detail
//! would otherwise obscure the scenario being exercised.

use super::super::MyEguiApp;
use super::super::state::UiState;
use crate::terminal_launcher::TerminalLauncher;
use crate::worker::{MockWorker, WorkerCommand, WorkerCommandBox, WorkerResult};
use egui_kittest::Harness;

pub(super) trait MockUiHarnessExt {
    /// Replace the UI state and render it before interacting with the screen.
    fn seed_ui_state(&mut self, state: UiState);

    /// Deliver one worker result and render the frame that consumes it.
    fn deliver_worker_result<R: WorkerResult>(&mut self, result: R);
}

impl<L: TerminalLauncher> MockUiHarnessExt for Harness<'_, MyEguiApp<MockWorker, L>> {
    fn seed_ui_state(&mut self, state: UiState) {
        self.state_mut().ui_state = state;
        self.run();
    }

    fn deliver_worker_result<R: WorkerResult>(&mut self, result: R) {
        self.state_mut().worker.enqueue_result(result);
        self.run();
    }
}

pub(super) fn command_is<C: WorkerCommand>(command: &WorkerCommandBox) -> Option<&C> {
    command.as_ref().as_any().downcast_ref()
}
