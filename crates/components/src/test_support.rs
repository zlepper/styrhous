//! Shared setup for component UI tests.
//!
//! Keeping this setup in the component crate lets both unit tests and the
//! public-API showcase snapshots render with the same deterministic theme and
//! image loaders.

use egui::Vec2;
use egui_kittest::Harness;

/// The deterministic viewport used by all egui tests and snapshots.
pub const EGUI_TEST_SIZE: Vec2 = Vec2::new(1536.0, 1024.0);

/// Configure a test harness for component tests and snapshots.
pub fn setup_egui<State>(harness: &mut Harness<'_, State>) {
    crate::apply_light_theme(&harness.ctx);
    egui_extras::install_image_loaders(&harness.ctx);
    harness.set_size(EGUI_TEST_SIZE);
}
