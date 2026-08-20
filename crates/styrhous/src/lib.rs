mod ansi;
mod api_resource;
mod cluster_connection_manager;
mod helm_release;
mod helpers;
mod log_store;
mod minimal_namespace;
mod minimal_resource;
mod pod_metrics;
mod resource_catalog;
mod resource_detail;
mod resource_extensions;
mod resource_handlers;
mod resource_schema;
mod resource_table;
mod sorted_name;
mod terminal_launcher;
mod ui;
mod updater;
mod worker;

use ui::MyEguiApp;

#[doc(hidden)]
pub use ui::log_viewer_profile::LogViewerProfile;
#[cfg(any(test, feature = "benchmarks"))]
#[doc(hidden)]
pub use ui::yaml_editor_profile::YamlEditorProfile;

pub(crate) const DEFAULT_NATIVE_WINDOW_SIZE: [f32; 2] = [1200.0, 800.0];
pub(crate) const MIN_NATIVE_WINDOW_SIZE: [f32; 2] = [1000.0, 600.0];

fn native_app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!(
        "../../../assets/icons/kubernetes-dev-ui.png"
    ))
    .expect("the bundled application icon must be a valid PNG")
}

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(DEFAULT_NATIVE_WINDOW_SIZE)
            .with_min_inner_size(MIN_NATIVE_WINDOW_SIZE)
            .with_icon(native_app_icon()),
        ..Default::default()
    }
}

pub fn run_native() {
    tracing_subscriber::fmt().init();
    updater::apply_staged_update();

    eframe::run_native(
        "Styrhous",
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
        assert_eq!(
            viewport.icon.as_ref().map(|icon| (icon.width, icon.height)),
            Some((512, 512))
        );
    }
}
