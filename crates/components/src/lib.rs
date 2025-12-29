//! Tailwind-inspired UI components for egui
//!
//! This crate provides styled, reusable UI components that follow
//! Tailwind CSS design principles.
//!
//! # Components
//!
//! - [`TailwindButton`] - A styled button with Primary, Secondary, and Soft variants
//!
//! # Example
//!
//! ```ignore
//! use components::{TailwindButton, ButtonSize, ButtonVariant};
//!
//! TailwindButton::new("Click me").show(ui);
//!
//! TailwindButton::primary("Save")
//!     .size(ButtonSize::Lg)
//!     .show(ui);
//! ```

pub mod button;
pub mod colors;
pub mod icons;
pub mod sidebar;

pub use button::{ButtonRounding, ButtonSize, ButtonVariant, TailwindButton};
pub use sidebar::{Sidebar, SidebarContent, SidebarMode};
