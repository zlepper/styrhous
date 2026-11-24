//! Which surface owns the keyboard: a blade, a log window or a YAML window
//! in front of the workspace grabs the keyboard: the shortcuts of the
//! workspace behind it (`/`, Ctrl+F, `[`, `]`, `P`, `G`, Ctrl+K, Ctrl+1…9) do
//! nothing.
//!
//! Each frame publishes the answer to the egui context, so the controls deep
//! in the workspace read it without it being threaded through every call.

use super::state::UiState;

/// The context key of the frame's published answer.
#[derive(Clone, Copy)]
struct SurfaceInFront(bool);

impl UiState {
    /// Whether a blade, a log window or a YAML window is open over the
    /// workspace.
    pub(super) fn surface_in_front(&self) -> bool {
        self.global_blades.navigator().is_some()
            || !self.log_windows.is_empty()
            || !self.yaml_editors.is_empty()
    }
}

/// Publish whether a surface is in front for the frame being drawn.
pub(super) fn publish(ctx: &egui::Context, ui_state: &UiState) {
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::NULL, SurfaceInFront(ui_state.surface_in_front()));
    });
}

/// Whether the workspace's own shortcuts (`/`, Ctrl+F, `[`, `]`, `P`, `G`) may
/// act: no surface is in front of it.
pub(super) fn workspace_owns_keys(ctx: &egui::Context) -> bool {
    !ctx.data(|data| data.get_temp::<SurfaceInFront>(egui::Id::NULL))
        .is_some_and(|surface| surface.0)
}
