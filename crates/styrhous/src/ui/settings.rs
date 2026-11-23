use super::global_blade::{
    GlobalBladeContent, GlobalBladeEffect, GlobalBladeEffectContext, GlobalBladeNavigation,
    GlobalBladeRenderContext, GlobalBladeRenderResult,
};
use super::state::ManagedClusterImport;
use crate::cluster_connection_manager::{
    AvailableAksCluster, AvailableEksCluster, AvailableTailscaleCluster, AwsProfile,
};
use crate::terminal_launcher::{DebugImagePreset, DebugProfile, TerminalLaunchSettings};
use crate::updater::UpdateStatus;
use crate::worker::{
    AddAksCluster, AddEksCluster, AddTailscaleCluster, LoadAwsAccounts,
    LoadManagedClusterDiscovery, ScanEksClusters,
};
use components::design::stroke;
use components::design::theme::{Theme, ThemeExt};
use components::design::{radius, space, type_scale};
use components::{
    Button, ButtonSize, ButtonVariant, Card, Combobox, FieldSize, FormField, Radio, RadioCard,
    ReorderableTable, TextInput, icons,
};

/// Room the settings blades leave under their form for the blade footer.
const FOOTER_HEIGHT: f32 = components::BLADE_BODY_FOOTER + space::S16;
const DEBUG_IMAGE_NAME_COLUMN_WIDTH: f32 = 170.0;
const DEBUG_IMAGE_PROFILE_COLUMN_WIDTH: f32 = 170.0;
const DISCOVERY_ROW_HEIGHT: f32 = 70.0;
const DISCOVERY_TAGS_EXTRA_HEIGHT: f32 = 18.0;
const DISCOVERY_COMPACT_ROW_HEIGHT: f32 = 54.0;
const AWS_PROFILE_ROW_HEIGHT: f32 = 32.0;
const DISCOVERY_NAME_COLUMN_WIDTH: f32 = 148.0;
/// The flexible column: the blade body (720 less 24 points of padding a side)
/// less the fixed columns.
const DISCOVERY_METADATA_COLUMN_WIDTH: f32 = 269.0;
const DISCOVERY_LOCATION_COLUMN_WIDTH: f32 = 94.0;
const DISCOVERY_ACTION_COLUMN_WIDTH: f32 = 161.0;

mod contexts;
mod debug_images;
mod destinations;
mod discovery;
mod discovery_table;
mod licensing;
mod namespace_selector;
mod navigation;
mod templates;
mod terminal;

use contexts::ContextsSettingsBlade;
use debug_images::*;
pub(super) use discovery::ManagedClusterDiscoveryBlade;
use discovery_table::*;
pub(super) use licensing::LicenseSettingsBlade;
pub(super) use namespace_selector::NamespaceSelectorSettingsBlade;
pub(super) use navigation::SettingsHomeBlade;
pub(super) use navigation::TerminalSettingsBlade;
use templates::*;
