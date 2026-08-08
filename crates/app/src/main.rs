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

pub(crate) const DEFAULT_NATIVE_WINDOW_SIZE: [f32; 2] = [1200.0, 800.0];
pub(crate) const MIN_NATIVE_WINDOW_SIZE: [f32; 2] = [1000.0, 600.0];

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(DEFAULT_NATIVE_WINDOW_SIZE)
            .with_min_inner_size(MIN_NATIVE_WINDOW_SIZE),
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
    use super::{DEFAULT_NATIVE_WINDOW_SIZE, MIN_NATIVE_WINDOW_SIZE, native_options};

    #[test]
    fn native_window_starts_wide_and_has_a_minimum_width() {
        let viewport = native_options().viewport;

        assert_eq!(
            viewport.inner_size,
            Some(egui::Vec2::from(DEFAULT_NATIVE_WINDOW_SIZE))
        );
        assert_eq!(
            viewport.min_inner_size,
            Some(egui::Vec2::from(MIN_NATIVE_WINDOW_SIZE))
        );
    }
}
