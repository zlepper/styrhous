//! Shared visual language for the application and reusable components.
//!
//! Keep values here semantic rather than scattering CSS-inspired numbers across
//! widgets. Domain-specific geometry (such as inspector blade animation) still
//! belongs with the feature that owns it.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke};

use crate::colors::{TABLE_BORDER, WHITE, gray};

pub mod typography {
    use super::{FontFamily, FontId};

    const INTER_SEMIBOLD: &str = "Inter SemiBold";

    pub const META_SIZE: f32 = 12.0;
    pub const BODY_SIZE: f32 = 14.0;
    pub const SECTION_SIZE: f32 = 16.0;
    pub const PAGE_TITLE_SIZE: f32 = 20.0;
    pub const MONOSPACE_SIZE: f32 = 13.0;

    pub fn metadata() -> FontId {
        FontId::proportional(META_SIZE)
    }

    pub fn body() -> FontId {
        FontId::proportional(BODY_SIZE)
    }

    pub fn section_heading() -> FontId {
        FontId::proportional(SECTION_SIZE)
    }

    pub fn page_title() -> FontId {
        FontId::proportional(PAGE_TITLE_SIZE)
    }

    pub fn monospace() -> FontId {
        FontId::monospace(MONOSPACE_SIZE)
    }

    /// Select Inter's real semibold face for an intentional emphasis level.
    pub fn semibold(size: f32) -> FontId {
        FontId::new(size, FontFamily::Name(INTER_SEMIBOLD.into()))
    }
}

pub mod spacing {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

pub mod radius {
    pub const SUBTLE: u8 = 4;
    pub const CONTROL: u8 = 6;
    pub const SURFACE: u8 = 8;

    pub fn subtle() -> egui::CornerRadius {
        egui::CornerRadius::same(SUBTLE)
    }

    pub fn control() -> egui::CornerRadius {
        egui::CornerRadius::same(CONTROL)
    }

    pub fn surface() -> egui::CornerRadius {
        egui::CornerRadius::same(SURFACE)
    }
}

pub mod surface {
    use super::{Color32, CornerRadius, Stroke, TABLE_BORDER, WHITE, gray};

    pub const BORDER_WIDTH: f32 = 1.0;

    pub fn border() -> Stroke {
        Stroke::new(BORDER_WIDTH, TABLE_BORDER)
    }

    pub fn muted_border() -> Stroke {
        Stroke::new(BORDER_WIDTH, gray::_200)
    }

    pub fn control_border() -> Stroke {
        Stroke::new(BORDER_WIDTH, gray::_300)
    }

    pub const fn canvas() -> Color32 {
        crate::colors::CONTENT_BACKGROUND
    }

    pub const fn card_fill() -> Color32 {
        WHITE
    }

    pub const TERMINAL_BACKGROUND: Color32 = Color32::from_rgb(10, 10, 11);

    pub fn control_radius() -> CornerRadius {
        crate::design::radius::control()
    }

    pub fn card_radius() -> CornerRadius {
        crate::design::radius::surface()
    }
}

pub mod status {
    use egui::Color32;

    pub const SUCCESS: Color32 = Color32::from_rgb(14, 150, 30);
    pub const WARNING: Color32 = Color32::from_rgb(202, 138, 4);
    pub const DANGER: Color32 = Color32::from_rgb(185, 28, 28);
    pub const CRITICAL: Color32 = Color32::from_rgb(220, 38, 38);
}

pub mod search {
    use egui::Color32;

    /// Background for inactive search matches on dark code and log surfaces.
    pub const MATCH_BACKGROUND: Color32 = Color32::from_rgb(120, 53, 15);
    /// Background for the match selected by find navigation.
    pub const ACTIVE_MATCH_BACKGROUND: Color32 = Color32::from_rgb(67, 56, 202);
}
