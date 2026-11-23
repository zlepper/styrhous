//! Shared visual language for the application and reusable components.
//!
//! Keep values here semantic rather than scattering CSS-inspired numbers across
//! widgets. Domain-specific geometry (such as inspector blade animation) still
//! belongs with the feature that owns it.
//!
//! # Harbour design system
//!
//! Components read colours only through theme roles:
//!
//! - [`palette`] — layer 1: raw colour values, the only colour literals.
//! - [`theme`] — layers 2 and 3: semantic roles and component overrides,
//!   resolved into a typed [`theme::Theme`] stored on the egui context, plus
//!   user theme files and context re-pointing scopes.
//! - [`type_scale`], [`space`], [`radius`], [`elevation`], [`size`] and [`stroke`] — fixed
//!   scales. They are deliberately not theme keys.
//! - [`fonts`] — the embedded font files and their egui families.

pub mod elevation;
pub mod fonts;
pub mod palette;
pub mod radius;
pub mod size;
pub mod space;
pub mod stroke;
pub mod theme;
pub mod type_scale;
