mod api_resource;
mod cluster_connection_manager;
mod helpers;
mod minimal_namespace;
mod minimal_resource;
mod resource_catalog;
mod resource_extensions;
mod resource_handlers;
mod resource_table;
mod sorted_name;
mod ui;
mod worker;

use crate::ui::MyEguiApp;

fn main() {
    tracing_subscriber::fmt().init();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Kubernetes dev UI",
        native_options,
        Box::new(|cc| Ok(Box::new(MyEguiApp::<worker::Worker>::new(cc)))),
    )
    .expect("eframe failed to start");
}
