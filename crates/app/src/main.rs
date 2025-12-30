mod cluster_connection_manager;
mod worker;
mod ui;
mod resource_extensions;
mod sorted_name;
mod helpers;
mod minimal_namespace;
mod minimal_resource;
mod api_resource;

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
