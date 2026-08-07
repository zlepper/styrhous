mod cluster_rail;
mod dialogs;
mod resource_navigation;
mod state;
mod widgets;
mod workspace;
mod yaml_editor;

use crate::worker::{Worker, WorkerCommand, WorkerTrait};
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

    #[cfg(test)]
    pub fn select_cluster(&mut self, cluster_key: i32) {
        if let Some(command) = self.ui_state.select_cluster(cluster_key) {
            self.worker.send_command(command);
        }
    }
}

impl<W: WorkerTrait> eframe::App for MyEguiApp<W> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.worker.start();
        self.ui_state.update(&mut self.worker);

        let mut commands_to_send = Vec::<WorkerCommand>::new();
        cluster_rail::show(ctx, &mut self.ui_state, &mut commands_to_send);
        let clicked_api_resource = resource_navigation::show(ctx, &self.ui_state);
        yaml_editor::show(ctx, &mut self.ui_state, &mut commands_to_send);
        workspace::show(ctx, &mut self.ui_state, &mut commands_to_send);
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
