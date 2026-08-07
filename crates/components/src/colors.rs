//! Tailwind CSS color palette for egui
//!
//! Color values sourced from Tailwind CSS v3.

use egui::Color32;

/// Tailwind Indigo palette
pub mod indigo {
    use super::Color32;

    pub const _50: Color32 = Color32::from_rgb(238, 242, 255); // #eef2ff
    pub const _100: Color32 = Color32::from_rgb(224, 231, 255); // #e0e7ff
    pub const _200: Color32 = Color32::from_rgb(199, 210, 254); // #c7d2fe
    pub const _300: Color32 = Color32::from_rgb(165, 180, 252); // #a5b4fc
    pub const _400: Color32 = Color32::from_rgb(129, 140, 248); // #818cf8
    pub const _500: Color32 = Color32::from_rgb(99, 102, 241); // #6366f1
    pub const _600: Color32 = Color32::from_rgb(79, 70, 229); // #4f46e5
    pub const _700: Color32 = Color32::from_rgb(67, 56, 202); // #4338ca
    pub const _800: Color32 = Color32::from_rgb(55, 48, 163); // #3730a3
    pub const _900: Color32 = Color32::from_rgb(49, 46, 129); // #312e81
    pub const _950: Color32 = Color32::from_rgb(30, 27, 75); // #1e1b4b
}

/// Tailwind Gray palette
pub mod gray {
    use super::Color32;

    pub const _50: Color32 = Color32::from_rgb(249, 250, 251); // #f9fafb
    pub const _100: Color32 = Color32::from_rgb(243, 244, 246); // #f3f4f6
    pub const _200: Color32 = Color32::from_rgb(229, 231, 235); // #e5e7eb
    pub const _300: Color32 = Color32::from_rgb(209, 213, 219); // #d1d5db
    pub const _400: Color32 = Color32::from_rgb(156, 163, 175); // #9ca3af
    pub const _500: Color32 = Color32::from_rgb(107, 114, 128); // #6b7280
    pub const _600: Color32 = Color32::from_rgb(75, 85, 99); // #4b5563
    pub const _700: Color32 = Color32::from_rgb(55, 65, 81); // #374151
    pub const _800: Color32 = Color32::from_rgb(31, 41, 55); // #1f2937
    pub const _900: Color32 = Color32::from_rgb(17, 24, 39); // #111827
    pub const _950: Color32 = Color32::from_rgb(3, 7, 18); // #030712
}

/// Common color constants
pub const WHITE: Color32 = Color32::from_rgb(255, 255, 255);
pub const BLACK: Color32 = Color32::from_rgb(0, 0, 0);

/// Surface tones sampled from the native-resolution workspace reference.
///
/// They deliberately sit just off pure white so the navigation, toolbar, and
/// data canvas read as related planes without card-like chrome.
pub const CONTENT_BACKGROUND: Color32 = Color32::from_rgb(253, 253, 253);
pub const TOOLBAR_BACKGROUND: Color32 = Color32::from_rgb(249, 249, 251);
pub const TABLE_HEADER_BACKGROUND: Color32 = Color32::from_rgb(249, 249, 250);
pub const CLUSTER_RAIL_BACKGROUND: Color32 = Color32::from_rgb(3, 9, 18);
pub const NAVIGATION_BACKGROUND: Color32 = Color32::from_rgb(10, 18, 29);
pub const TABLE_BORDER: Color32 = Color32::from_rgb(234, 235, 238);
pub const SUCCESS: Color32 = Color32::from_rgb(14, 150, 30);
