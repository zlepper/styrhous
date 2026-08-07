//! Tailwind-inspired UI components for egui
//!
//! This crate provides styled, reusable UI components that follow
//! Tailwind CSS design principles.
//!
//! # Components
//!
//! - [`TailwindButton`] - A styled button with Primary, Secondary, and Soft variants
//! - [`TailwindCombobox`] - A filterable combobox with dropdown
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
pub mod combobox;
pub mod fuzzy;
pub mod icons;
pub mod sidebar;
pub mod table;
pub mod tabs;
pub mod theme;

pub use button::{ButtonRounding, ButtonSize, ButtonVariant, TailwindButton};
pub use combobox::{ComboboxUi, ItemResponse, NoFilter, TailwindCombobox, WithFilter};
pub use sidebar::{
    ExpandableResponse, NarrowSidebar, NarrowSidebarContent, WideSidebar, WideSidebarContent,
};
pub use table::{
    SortDirection, SortState, TableColumnBuilder, TableRowBuilder, TailwindTable,
};
pub use tabs::{Tabs, TabsContent, TabsResponse};
pub use theme::apply_light_theme;
