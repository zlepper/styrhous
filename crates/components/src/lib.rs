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

pub mod blade;
pub mod button;
pub mod colors;
pub mod combobox;
pub mod design;
pub mod error_dialog;
pub mod fuzzy;
pub mod icons;
pub mod interaction;
pub mod more_button;
pub mod search;
pub mod sidebar;
pub mod table;
pub mod tabs;
#[doc(hidden)]
pub mod test_support;
pub mod textarea;
pub mod theme;
pub mod workspace;

pub use blade::{
    BLADE_WIDTH, BladeLayer, BladeNavigator, BladeResponse, BladeStack, BladeTransition,
};
pub use button::{ButtonRounding, ButtonSize, ButtonVariant, TailwindButton};
pub use combobox::{
    ComboboxResponse, ComboboxUi, ItemResponse, NoFilter, SelectionAction, TailwindCombobox,
    WithFilter,
};
pub use error_dialog::{ErrorDialog, ErrorDialogAction};
pub use interaction::PointingHand;
pub use more_button::{MoreButton, MoreMenu};
pub use search::{SearchInputResponse, TailwindSearchInput};
pub use sidebar::{
    ExpandableResponse, NarrowSidebar, NarrowSidebarContent, WideSidebar, WideSidebarContent,
};
pub use table::{SortDirection, SortState, TableColumnBuilder, TableRowBuilder, TailwindTable};
pub use tabs::{Tabs, TabsContent, TabsResponse};
pub use textarea::TailwindTextArea;
pub use theme::{apply_light_theme, semibold_font};
pub use workspace::{
    WorkspaceCard, WorkspaceDrawer, WorkspaceEmptyState, WorkspacePage, workspace_section_header,
};
