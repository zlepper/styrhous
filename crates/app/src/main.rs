mod api_resource;
mod cluster_connection_manager;
mod helpers;
mod minimal_namespace;
mod minimal_resource;
mod resource_catalog;
mod resource_detail;
mod resource_extensions;
mod resource_handlers;
mod resource_table;
mod sorted_name;
mod ui;
mod worker;

use crate::ui::MyEguiApp;

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([1000.0, 600.0]),
        ..Default::default()
    }
}

fn main() {
    tracing_subscriber::fmt().init();

    eframe::run_native(
        "Kubernetes dev UI",
        native_options(),
        Box::new(|cc| Ok(Box::new(MyEguiApp::<worker::Worker>::new(cc)))),
    )
    .expect("eframe failed to start");
}

#[cfg(test)]
mod tests {
    use super::native_options;

    #[test]
    fn native_window_starts_wide_and_has_a_minimum_width() {
        let viewport = native_options().viewport;

        assert_eq!(viewport.inner_size, Some(egui::vec2(1200.0, 800.0)));
        assert_eq!(viewport.min_inner_size, Some(egui::vec2(1000.0, 600.0)));
    }
}
