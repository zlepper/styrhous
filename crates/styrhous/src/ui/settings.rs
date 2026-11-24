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
use crate::worker::{LoadAwsAccounts, LoadManagedClusterDiscovery, ScanEksClusters};
use components::design::stroke;
use components::design::theme::ThemeExt;
use components::design::{radius, space, type_scale};
use components::{
    Button, ButtonSize, ButtonVariant, Card, Combobox, FieldSize, FormField, Radio, RadioCard,
    ReorderableTable, TextInput, icons,
};

/// Room the settings blades leave under their form for the blade footer.
const FOOTER_HEIGHT: f32 = components::BLADE_BODY_FOOTER + space::S16;
const DEBUG_IMAGE_NAME_COLUMN_WIDTH: f32 = 150.0;
const DEBUG_IMAGE_PROFILE_COLUMN_WIDTH: f32 = 164.0;
/// The custom launcher's command template field.
/// The debug profile menu, wider than its select.
const DEBUG_PROFILE_POPUP_WIDTH: f32 = 160.0;
const TEMPLATE_INPUT_WIDTH: f32 = 596.0;
const LICENSE_DEVICE_NAME_WIDTH: f32 = 320.0;
const AWS_ACCOUNT_SELECT_WIDTH: f32 = 280.0;
const AWS_PROFILE_SELECT_WIDTH: f32 = 200.0;

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

pub(super) use contexts::ContextsSettingsBlade;
use debug_images::*;
pub(super) use discovery::ManagedClusterDiscoveryBlade;
pub(super) use licensing::LicenseSettingsBlade;
pub(super) use namespace_selector::NamespaceSelectorSettingsBlade;
pub(super) use navigation::SettingsHomeBlade;
pub(super) use navigation::TerminalSettingsBlade;
use templates::*;
