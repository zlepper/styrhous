//! Tailwind-inspired UI components for egui
//!
//! This crate provides styled, reusable UI components that follow
//! Tailwind CSS design principles.
//!
//! # Components
//!
//! - [`Button`] - The Harbour button: six variants plus links, three sizes
//! - [`ToggleButton`], [`ButtonGroup`], [`Segmented`] - Pressed-state controls
//! - [`Combobox`] - Select and searchable combobox (C05)
//! - [`TextInput`], [`SearchField`], [`TextArea`], [`NumberField`] - Text inputs (C04)
//! - [`Checkbox`], [`Radio`], [`RadioCard`], [`Switch`] - Selection controls (C06)
//!
//! # Example
//!
//! ```ignore
//! use components::{Button, ButtonSize};
//!
//! Button::secondary("Refresh").show(ui);
//!
//! Button::primary("Save")
//!     .size(ButtonSize::Lg)
//!     .show(ui);
//! ```

mod a11y;
pub mod avatar;
pub mod badge;
pub mod blade;
pub mod button;
pub mod combobox;
pub mod confirmation_dialog;
mod control;
mod copy;
pub mod design;
pub mod error_dialog;
pub mod field;
pub mod frames;
pub mod fuzzy;
pub mod icons;
pub mod inspector_details;
pub mod interaction;
pub mod keycap;
pub mod loading;
pub mod measured;
pub mod more_button;
pub mod number_field;
mod popover_footer;
pub mod progress_checklist;
pub mod reorderable_table;
pub mod scroll;
pub mod search;
pub mod selection;
pub mod sidebar;
pub mod status;
pub mod table;
pub mod tabs;
#[doc(hidden)]
pub mod test_support;
pub mod text_input;
pub mod textarea;
pub mod title_stack;
pub mod toggle;
pub mod workspace;

// C07
pub mod menu;
pub mod tooltip;

// C13
pub mod card;
pub mod code_block;
pub mod list_row;
pub mod property_grid;
pub mod tile;
pub mod usage_chart;

// C14
pub mod window_chrome;

// C15
pub mod code_surface;
pub mod completion;
pub mod diagnostics;

pub use avatar::Avatar;
pub use badge::{
    COUNT_BADGE_CAP, Chip, CountBadge, Pill, corner_rect, kind_badge, kind_badge_width, well_frame,
};
pub use blade::{
    BLADE_BODY_FOOTER, BLADE_WIDTH, BladeFrame, BladeLayer, BladeNavigation, BladeNavigator,
    BladeResponse, BladeStack, BladeTransition, blade_body_footer, blade_footer, blade_title,
    blade_title_with_actions, paint_blade_peek, paint_blade_peek_on,
};
pub use button::{Button, ButtonSize, ButtonVariant};
pub use combobox::{
    Combobox, ComboboxResponse, ComboboxUi, ItemResponse, NoFilter, SelectionAction, WithFilter,
};
pub use confirmation_dialog::{
    ConfirmationDialog, ConfirmationDialogAcknowledgement, ConfirmationDialogAction,
    ConfirmationDialogKind, ConfirmationDialogWarning,
};
pub use error_dialog::{CLOSE_LABEL, ErrorDialog, ErrorDialogAction};
pub use field::{FieldMessage, FieldSize, FormField, TEXT_EDIT_MARGIN};
pub use frames::{overlay_frame, popover_frame, raised_frame};
pub use inspector_details::{
    DetailCell, DetailColumn, DetailRow, DetailTableCell, DetailTableRow, DetailValue,
    InspectorDetails, InspectorDetailsResponse,
};
pub use interaction::{ControlState, NOT_IMPLEMENTED, PointingHand, not_implemented};
pub use keycap::Keycap;
pub use loading::{Spinner, skeleton_bar};
pub use more_button::MoreButton;
pub use number_field::{NumberField, NumberFieldResponse};
pub use progress_checklist::{ChecklistStep, ChecklistStepState, ProgressChecklist};
pub use reorderable_table::{ReorderHandle, ReorderableTable, move_item};
pub use search::{
    SEARCH_FIELD_WIDTH, SearchField, SearchFieldResponse, SearchStatus, counted, format_exact_count,
};
pub use selection::{
    BoxLook, CheckState, Checkbox, Radio, RadioCard, RadioCardResponse, Switch, paint_box,
    paint_checkbox, paint_radio, paint_switch,
};
pub use sidebar::{
    ClusterCard, FloatingSidebar, SideItem, kind_filter, kind_filter_with_key, side_caption,
    side_footer,
};
pub use status::{StatusLabel, container_square, paint_status_dot, status_dot};
pub use table::{
    BulkSelectionBar, BulkSelectionResponse, CellAlign, ColumnVisibilityResponse, HeaderState,
    SortDirection, SortState, Table, TableColumnBuilder, TableRowBuilder, column_visibility_menu,
};
pub use tabs::{Tabs, TabsContent, TabsResponse};
pub use text_input::TextInput;
pub use textarea::TextArea;
pub use toggle::{ButtonGroup, Segmented, SegmentedSize, ToggleButton};
// C07
pub use menu::{Menu, MenuItem, MenuLook};
pub use tooltip::{TooltipContent, TooltipExt};
pub use workspace::WorkspacePage;

// C11
pub mod callout;
pub mod dialog;
pub mod prose;

pub use callout::{Callout, tone_icon};
pub use dialog::{Dialog, DialogResponse};
pub use prose::Prose;

// C12
pub mod banner;
pub mod empty_state;
pub mod mini_button;
pub mod toast;

pub use banner::{Banner, BannerButton, BannerResponse};
pub use empty_state::{EMPTY_STATE_MAX_WIDTH, EmptyState};
pub use mini_button::{MINI_BUTTON, MiniButton};
pub use toast::{
    TOAST_BOTTOM_INSET, TOAST_GAP, TOAST_INSET, TOAST_WIDTH, Toast, ToastResponse,
    show_toast_stack, toast_icon,
};

// C13
pub use badge::{CHIP_GAP, chip_flow, labelled_chips, labelled_chips_with_label_width};
pub use card::{Card, Disclosure, DisclosureResponse};
pub use code_block::{CodeBlock, CodeBlockResponse};
pub use list_row::{ListGroup, ListRow, ListRowTrailing, list_group};
pub use property_grid::{Copyable, Property, PropertyGrid, PropertyGridResponse, PropertyValue};
pub use tile::{Tile, TileSize};
pub use usage_chart::{UsageChart, UsageResource};

// C14
pub use window_chrome::{
    StatusBar, Toolbar, WindowHeader, WindowHeaderResponse, problems_toggle, status_icon,
    status_number, status_ok, status_text, toolbar_activity, toolbar_divider,
};

// C15
pub use badge::small_kind_badge;
pub use code_surface::{
    CodeColor, CodeDecoration, CodeLine, CodeMark, CodeSpan, CodeSurface, CodeSurfaceResponse,
};
pub use completion::{
    Completion, CompletionDoc, CompletionExpects, CompletionResponse, completion_doc_position,
    completion_position,
};
pub use diagnostics::{Problem, ProblemsPanel, ProblemsResponse, Severity};

// C16
pub mod top_bar;

pub use search::SearchFieldStyle;
pub use top_bar::{FilterChip, LiveMeta, ScopePill, SearchPill, TopBar, TopBarKeys};

// C17
pub mod group_header;
pub mod problem_ticks;

pub use group_header::{GroupHeader, GroupHeaderResponse};
pub use problem_ticks::{ProblemTick, ProblemTicks, ProblemTicksResponse};
pub use table::{GroupedTableResponse, TableGroup};

// C19
pub mod readout;
pub mod section_rail;

pub use badge::kind_badge_on;
pub use blade::{
    BladeDescription, BladeHeader, BladePeek, BladeStyle, HeaderAction, InspectorHeader,
    InspectorHeaderResponse, InspectorStatus,
};
pub use readout::{Readout, ReadoutRow, ReadoutSize, ReadoutTone};
pub use section_rail::{InspectorBody, Section, SectionRail, inspector_section_heading};

// C20
pub mod log_viewer;

pub use log_viewer::{
    FloatingToolbar, LevelBadge, LogHeader, LogHeaderResponse, LogLevel, LogLine, LogLineResponse,
    LogSource, LogStream, SourcePill, SourceState, VolumeBar, VolumeBars, VolumeBarsResponse,
    VolumeStrip, VolumeStripResponse,
};

// C18
pub mod cluster_switcher;

pub use cluster_switcher::{
    ClusterAction, ClusterEntry, ClusterSection, ClusterSummary, ClusterSwitcher,
    ClusterSwitcherState, ClusterTarget, ClusterTile,
};

// A09
pub mod swatches;

pub use swatches::Swatches;
