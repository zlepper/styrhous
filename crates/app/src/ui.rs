mod cluster_rail;
mod dialogs;
mod resource_actions;
mod resource_detail;
mod resource_navigation;
mod state;
mod widgets;
mod workspace;
mod yaml_editor;

use crate::worker::{Worker, WorkerTrait};
use components::apply_light_theme;
use dialogs::show_delete_confirmation;
use state::UiState;

pub struct MyEguiApp<W: WorkerTrait = Worker> {
    worker: W,
    ui_state: UiState,
}

impl<W: WorkerTrait> Default for MyEguiApp<W> {
    fn default() -> Self {
        Self {
            worker: W::default(),
            ui_state: UiState::default(),
        }
    }
}

impl<W: WorkerTrait> MyEguiApp<W> {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        apply_light_theme(&cc.egui_ctx);
        Self::default()
    }
}

impl<W: WorkerTrait> eframe::App for MyEguiApp<W> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.worker.start();
        let mut commands_to_send = self.ui_state.update(&mut self.worker);

        cluster_rail::show(ctx, &mut self.ui_state, &mut commands_to_send);
        let clicked_api_resource = resource_navigation::show(ctx, &self.ui_state);
        yaml_editor::show(ctx, &mut self.ui_state, &mut commands_to_send);
        workspace::show(ctx, &mut self.ui_state, &mut commands_to_send);
        resource_detail::show(ctx, &mut self.ui_state, &mut commands_to_send);
        show_delete_confirmation(ctx, &mut self.ui_state, &mut commands_to_send);

        if let (Some(cluster_key), Some(api_resource)) =
            (self.ui_state.selected_cluster, clicked_api_resource)
        {
            self.ui_state
                .select_api_resource(cluster_key, api_resource, &mut commands_to_send);
        }

        for command in commands_to_send {
            self.worker.send_command(command);
        }
    }
}

#[cfg(test)]
mod tests;
