//! Shared setup for component UI tests.
//!
//! Keeping this setup in the component crate lets both unit tests and the
//! public-API showcase snapshots render with the same deterministic theme and
//! image loaders.

use egui::Context;

/// Configure an egui context for component tests and snapshots.
pub fn setup_egui(ctx: &Context) {
    crate::apply_light_theme(ctx);
    egui_extras::install_image_loaders(ctx);
}
