use crate::cluster_connection_manager::Cluster;
use crate::worker::{Worker, WorkerResult};
use egui::{Button, Widget};
use tracing::{error, info};

#[derive(Default)]
pub struct MyEguiApp {
    counter_value: i32,
    worker: Worker,
    ui_state: UiState,
}

#[derive(Default)]
pub struct UiState {
    clusters: Vec<Cluster>,
}

impl UiState {
    fn update(&mut self, worker: &mut Worker) {
        while let Some(result) = worker.get_next_message() {
            match result {
                WorkerResult::CommandFailed { error, command } => {
                    error!("Command '{:?}' failed with error: {}", command, error);
                }
                WorkerResult::KubernetesClustersUpdated(clusters) => {
                    self.clusters = clusters;
                }
            }
        }
    }
}

impl MyEguiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self::default()
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.worker.start();

        self.ui_state.update(&mut self.worker);

        egui::SidePanel::left("left_panel")
            .exact_width(64f32)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    for cluster in &self.ui_state.clusters {
                        if Button::new(&cluster.name)
                            .corner_radius(5.0)
                            .ui(ui)
                            .clicked()
                        {
                            info!("Cluster '{}' selected", cluster.name)
                        }
                    }
                })
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");

            ui_counter(ui, &mut self.counter_value)
        });
    }
}

fn ui_counter(ui: &mut egui::Ui, counter: &mut i32) {
    // Put the buttons and label on the same row:
    ui.horizontal(|ui| {
        if ui.button("−").clicked() {
            *counter -= 1;
        }
        ui.label(counter.to_string());
        if ui.button("+").clicked() {
            *counter += 1;
        }
    });
}
