use super::global_blade::{
    GlobalBladeContent, GlobalBladeEffect, GlobalBladeEffectContext, GlobalBladeNavigation,
    GlobalBladeRenderContext, GlobalBladeRenderResult,
};
use super::state::ManagedClusterImport;
use crate::cluster_connection_manager::{AvailableAksCluster, AvailableTailscaleCluster};
use crate::terminal_launcher::{DebugImagePreset, DebugProfile, TerminalLaunchSettings};
use crate::updater::UpdateStatus;
use crate::worker::{AddAksCluster, AddTailscaleCluster, LoadManagedClusterDiscovery};
use components::colors::{
    CONTENT_BACKGROUND, TABLE_BORDER, TABLE_HEADER_BACKGROUND, WHITE, gray, indigo,
};
use components::design::{radius, spacing, status, surface, typography};
use components::{
    ButtonSize, ButtonVariant, PointingHand, ReorderHandle, ReorderableTable, TailwindButton,
    TailwindCombobox, TailwindTextInput, icons,
};
use egui::AtomExt as _;

const FOOTER_HEIGHT: f32 = 52.0;
const CHOICE_CONTENT_MIN_HEIGHT: f32 = 44.0;
const DEBUG_IMAGE_TABLE_HEADER_HEIGHT: f32 = 40.0;
const DEBUG_IMAGE_TABLE_ROW_HEIGHT: f32 = 44.0;
const DEBUG_IMAGE_REORDER_COLUMN_WIDTH: f32 = 44.0;
const DEBUG_IMAGE_NAME_COLUMN_WIDTH: f32 = 170.0;
const DEBUG_IMAGE_PROFILE_COLUMN_WIDTH: f32 = 170.0;
const DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH: f32 = 52.0;
const DISCOVERY_ROW_HEIGHT: f32 = 70.0;
const DISCOVERY_COMPACT_ROW_HEIGHT: f32 = 54.0;
const DISCOVERY_NAME_COLUMN_WIDTH: f32 = 148.0;
const DISCOVERY_METADATA_COLUMN_WIDTH: f32 = 292.0;
const DISCOVERY_LOCATION_COLUMN_WIDTH: f32 = 94.0;
const DISCOVERY_ACTION_COLUMN_WIDTH: f32 = 160.0;
const DISCOVERY_HEADER_TITLE_OFFSET: f32 = 18.0;
const SETTINGS_DESTINATION_CONTENT_HEIGHT: f32 = 140.0;
const SETTINGS_DESTINATION_ICON_TILE_SIZE: f32 = 84.0;
const SETTINGS_DESTINATION_CHEVRON_SIZE: f32 = 24.0;

mod debug_images;
mod destinations;
mod discovery;
mod discovery_table;
mod navigation;
mod templates;
mod terminal;

use debug_images::*;
pub(super) use discovery::ManagedClusterDiscoveryBlade;
use discovery_table::*;
pub(super) use navigation::SettingsHomeBlade;
pub(super) use navigation::TerminalSettingsBlade;
use templates::*;
