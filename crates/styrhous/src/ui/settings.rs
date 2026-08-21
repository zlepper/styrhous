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

#[derive(Debug, Default)]
pub(super) struct SettingsHomeBlade;

impl GlobalBladeContent for SettingsHomeBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        ui.label(
            egui::RichText::new("Settings")
                .font(typography::page_title())
                .color(gray::_900),
        );
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        ui.label(
            egui::RichText::new("Manage application preferences and add Kubernetes contexts.")
                .font(typography::body())
                .color(gray::_600),
        );
        ui.add_space(spacing::XL);
        ui.separator();
        ui.add_space(spacing::XL + spacing::XS);
        ui.label(
            egui::RichText::new("CONFIGURATION")
                .font(typography::semibold(typography::BODY_SIZE))
                .color(gray::_500),
        );
        ui.add_space(spacing::MD);
        let application_settings = settings_destination_card(
            ui,
            SettingsDestination {
                label: "Open application settings",
                description: "Configure terminal launching, debug images, and application updates.",
                icon: icons::settings_destination_application_icon(),
            },
        );
        ui.add_space(spacing::XL - spacing::XS);
        let cluster_discovery = settings_destination_card(
            ui,
            SettingsDestination {
                label: "Open cluster discovery",
                description: "Find and add clusters available through Azure CLI or Tailscale.",
                icon: icons::settings_destination_discovery_icon(),
            },
        );
        GlobalBladeRenderResult {
            next_content: application_settings
                .then(|| {
                    Box::new(TerminalSettingsBlade::new(
                        context.terminal_launch_settings().clone(),
                    )) as Box<dyn GlobalBladeContent>
                })
                .or_else(|| {
                    cluster_discovery.then(|| {
                        Box::new(ManagedClusterDiscoveryBlade::default())
                            as Box<dyn GlobalBladeContent>
                    })
                }),
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub(super) struct TerminalSettingsBlade {
    pub(super) draft: TerminalLaunchSettings,
    pub(super) error: Option<String>,
}

impl TerminalSettingsBlade {
    pub(super) fn new(draft: TerminalLaunchSettings) -> Self {
        Self { draft, error: None }
    }
}

impl GlobalBladeContent for TerminalSettingsBlade {
    #[cfg(test)]
    fn terminal_settings(&self) -> Option<&TerminalSettingsBlade> {
        Some(self)
    }
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        _context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        ui.label(
            egui::RichText::new("Application settings")
                .font(typography::page_title())
                .color(gray::_900),
        );
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let mut save = false;
        let mut reset = false;
        show_settings_introduction(ui);
        show_update_status(ui, context.update_status());
        ui.add_space(spacing::XL);
        ui.separator();
        ui.add_space(spacing::XL);
        let content_height = (ui.available_height() - FOOTER_HEIGHT).max(120.0);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), content_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                components::scroll::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        show_terminal_launcher(ui, &mut self.draft, &mut self.error);
                        ui.add_space(spacing::XL);
                        ui.separator();
                        ui.add_space(spacing::XL);
                        show_debug_image_presets(ui, &mut self.draft, &mut self.error);
                    });
            },
        );
        ui.separator();
        ui.add_space(spacing::SM);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            save |= TailwindButton::new("Save changes").show(ui).clicked();
            reset |= TailwindButton::secondary("Reset")
                .size(ButtonSize::Md)
                .show(ui)
                .clicked();
        });
        if reset {
            self.draft = TerminalLaunchSettings::default();
            self.error = None;
        }
        if save {
            match self.draft.validate() {
                Ok(()) => {
                    *context.terminal_launch_settings() = self.draft.clone();
                    self.error = None;
                    return GlobalBladeRenderResult {
                        close: true,
                        ..Default::default()
                    };
                }
                Err(error) => self.error = Some(error),
            }
        }
        GlobalBladeRenderResult::default()
    }
}

struct SettingsDestination<'a> {
    label: &'a str,
    description: &'a str,
    icon: egui::Image<'static>,
}

impl SettingsDestination<'_> {
    fn text(&self) -> egui::text::LayoutJob {
        let mut text = egui::text::LayoutJob::default();
        text.append(
            self.label,
            0.0,
            egui::TextFormat {
                font_id: typography::section_heading(),
                color: gray::_900,
                ..Default::default()
            },
        );
        text.append("\n", 0.0, Default::default());
        text.append(
            self.description,
            0.0,
            egui::TextFormat {
                font_id: typography::body(),
                color: gray::_600,
                ..Default::default()
            },
        );
        text
    }
}

fn settings_destination_card(ui: &mut egui::Ui, destination: SettingsDestination<'_>) -> bool {
    let saved_widgets = ui.visuals().widgets.clone();
    let saved_button_padding = ui.spacing().button_padding;
    let visuals = &mut ui.visuals_mut().widgets;
    for widget_visuals in [
        &mut visuals.inactive,
        &mut visuals.hovered,
        &mut visuals.active,
    ] {
        widget_visuals.bg_stroke = surface::muted_border();
        widget_visuals.corner_radius = radius::surface();
    }
    visuals.inactive.weak_bg_fill = WHITE;
    visuals.inactive.bg_fill = WHITE;
    visuals.hovered.weak_bg_fill = gray::_50;
    visuals.hovered.bg_fill = gray::_50;
    visuals.active.weak_bg_fill = gray::_100;
    visuals.active.bg_fill = gray::_100;
    ui.spacing_mut().button_padding = egui::vec2(spacing::LG, spacing::MD);
    let accessible_label = format!("{}: {}", destination.label, destination.description);
    let text = egui::WidgetText::from(destination.text()).atom_shrink(true);
    let response = ui.add(
        egui::Button::new((
            destination
                .icon
                .fit_to_exact_size(egui::Vec2::splat(SETTINGS_DESTINATION_ICON_TILE_SIZE)),
            text,
        ))
        .gap(spacing::XL)
        .right_text(
            icons::chevron_right_icon()
                .fit_to_exact_size(egui::Vec2::splat(SETTINGS_DESTINATION_CHEVRON_SIZE))
                .tint(gray::_700),
        )
        .min_size(egui::vec2(
            ui.available_width(),
            SETTINGS_DESTINATION_CONTENT_HEIGHT,
        ))
        .corner_radius(radius::surface()),
    );

    ui.visuals_mut().widgets = saved_widgets;
    ui.spacing_mut().button_padding = saved_button_padding;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            accessible_label.clone(),
        )
    });
    response.clicked()
}

#[derive(Debug, Default)]
pub(super) struct ManagedClusterDiscoveryBlade {
    initial_load: bool,
    pending_action: Option<ManagedDiscoveryAction>,
}

#[derive(Debug)]
enum ManagedDiscoveryAction {
    Refresh,
    AddAks {
        subscription_id: String,
        resource_group: String,
        cluster_name: String,
    },
    AddTailscale {
        host_name: String,
    },
}

impl GlobalBladeContent for ManagedClusterDiscoveryBlade {
    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let discovery = context.managed_cluster_discovery();
        ui.horizontal(|ui| {
            ui.add_space(-DISCOVERY_HEADER_TITLE_OFFSET);
            ui.label(
                egui::RichText::new("Cluster discovery")
                    .font(typography::page_title())
                    .color(gray::_900),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled_ui(!discovery.loading && discovery.importing.is_none(), |ui| {
                        discovery_refresh_button(ui).clicked()
                    })
                    .inner
                {
                    self.pending_action = Some(ManagedDiscoveryAction::Refresh);
                }
            });
        });
        GlobalBladeRenderResult::default()
    }

    fn render_body(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        if !self.initial_load {
            self.initial_load = true;
            self.pending_action = Some(ManagedDiscoveryAction::Refresh);
        }
        let discovery = context.managed_cluster_discovery();
        ui.add_space(spacing::SM);
        ui.label(
            egui::RichText::new(
                "Find and add clusters from supported providers to your kubeconfig.",
            )
            .font(typography::body())
            .color(gray::_600),
        );
        ui.add_space(spacing::SM);
        ui.label(
            egui::RichText::new("New clusters appear automatically. Refresh after making changes.")
                .font(typography::body())
                .color(gray::_600),
        );
        if let Some(error) = &discovery.error {
            ui.add_space(spacing::SM);
            ui.label(
                egui::RichText::new(error)
                    .font(typography::body())
                    .color(status::CRITICAL),
            );
        }
        ui.add_space(spacing::LG);
        ui.separator();
        ui.add_space(spacing::XL);
        self.show_azure(ui, discovery);
        ui.add_space(spacing::SM);
        ui.separator();
        ui.add_space(spacing::XL);
        self.show_tailscale(ui, discovery);
        GlobalBladeRenderResult::default()
    }

    fn take_effect(&mut self) -> Option<Box<dyn GlobalBladeEffect>> {
        self.pending_action
            .take()
            .map(|action| Box::new(ManagedDiscoveryEffect(action)) as Box<dyn GlobalBladeEffect>)
    }
}

fn discovery_refresh_button(ui: &mut egui::Ui) -> egui::Response {
    ui.add(
        egui::Button::image_and_text(
            icons::arrow_path_icon()
                .fit_to_exact_size(egui::Vec2::splat(16.0))
                .tint(indigo::_600),
            egui::RichText::new("Refresh")
                .font(typography::body())
                .color(indigo::_600),
        )
        .frame(false),
    )
    .with_pointing_hand()
}

impl ManagedClusterDiscoveryBlade {
    fn show_azure(
        &mut self,
        ui: &mut egui::Ui,
        discovery: &super::state::ManagedClusterDiscoveryState,
    ) {
        show_provider_heading(
            ui,
            icons::azure_icon(),
            "Azure Kubernetes Service",
            discovery_count(
                discovery.loading,
                discovery.tools.azure_cli,
                discovery.azure_error.as_ref(),
                discovery.aks_clusters.len(),
            ),
        );
        ui.add_space(spacing::XL + spacing::XS);
        if discovery.loading {
            show_discovery_loading(ui, "Checking Azure subscriptions for AKS clusters…");
        } else if !discovery.tools.azure_cli {
            show_unavailable_tool(
                ui,
                "Azure CLI",
                "Install Azure CLI and sign in with `az login` to discover AKS clusters.",
            );
        } else if let Some(error) = &discovery.azure_error {
            show_discovery_error(ui, error);
        } else if discovery.aks_clusters.is_empty() {
            show_empty_discovery(
                ui,
                "No AKS clusters were found for the signed-in Azure CLI account.",
            );
        } else {
            show_discovery_headers(
                ui,
                "Cluster name",
                "Tenant · subscription · resource group",
                "Location",
                "Action",
                true,
            );
            for cluster in &discovery.aks_clusters {
                self.show_aks_row(ui, cluster, discovery);
            }
        }
        if let Some(warning) = &discovery.azure_warning {
            ui.add_space(spacing::SM);
            show_discovery_warning(ui, warning);
        }
    }

    fn show_tailscale(
        &mut self,
        ui: &mut egui::Ui,
        discovery: &super::state::ManagedClusterDiscoveryState,
    ) {
        show_provider_heading(
            ui,
            icons::tailscale_icon(),
            "Tailscale",
            discovery_count(
                discovery.loading,
                discovery.tools.tailscale,
                discovery.tailscale_error.as_ref(),
                discovery.tailscale_clusters.len(),
            ),
        );
        ui.add_space(spacing::XL + spacing::XS);
        if discovery.loading {
            show_discovery_loading(ui, "Checking Tailscale peers for Kubernetes API proxies…");
        } else if !discovery.tools.tailscale {
            show_unavailable_tool(
                ui,
                "Tailscale",
                "Install and sign in to Tailscale to discover Kubernetes API proxies.",
            );
        } else if let Some(error) = &discovery.tailscale_error {
            show_discovery_error(ui, error);
        } else if discovery.tailscale_clusters.is_empty() {
            show_empty_discovery(ui, "No Tailscale peers tagged tag:k8s-operator were found.");
        } else {
            show_discovery_headers(
                ui,
                "Cluster name",
                "Hostname",
                "Connection",
                "Action",
                false,
            );
            for cluster in &discovery.tailscale_clusters {
                self.show_tailscale_row(ui, cluster, discovery);
            }
        }
    }

    fn show_aks_row(
        &mut self,
        ui: &mut egui::Ui,
        cluster: &AvailableAksCluster,
        discovery: &super::state::ManagedClusterDiscoveryState,
    ) {
        let import = ManagedClusterImport::Aks {
            subscription_id: cluster.subscription_id.clone(),
            resource_group: cluster.resource_group.clone(),
            cluster_name: cluster.name.clone(),
        };
        let metadata = format!(
            "{} · {} · {}",
            cluster.tenant_name, cluster.subscription_name, cluster.resource_group
        );
        DiscoveryRow {
            name: &cluster.name,
            metadata: &metadata,
            detail: &cluster.location,
            status: DiscoveryRowStatus::Aks,
            stack_metadata: true,
            height: DISCOVERY_ROW_HEIGHT,
            import_state: discovery_import_state(
                cluster.configured,
                discovery.importing.as_ref() == Some(&import),
                discovery.importing.is_some(),
            ),
        }
        .show(ui, || {
            self.pending_action = Some(ManagedDiscoveryAction::AddAks {
                subscription_id: cluster.subscription_id.clone(),
                resource_group: cluster.resource_group.clone(),
                cluster_name: cluster.name.clone(),
            });
        });
    }

    fn show_tailscale_row(
        &mut self,
        ui: &mut egui::Ui,
        cluster: &AvailableTailscaleCluster,
        discovery: &super::state::ManagedClusterDiscoveryState,
    ) {
        let import = ManagedClusterImport::Tailscale {
            host_name: cluster.host_name.clone(),
        };
        let status_text = if cluster.online { "Online" } else { "Offline" };
        DiscoveryRow {
            name: &cluster.host_name,
            metadata: &cluster.dns_name,
            detail: status_text,
            status: DiscoveryRowStatus::Tailscale {
                online: cluster.online,
            },
            stack_metadata: false,
            height: DISCOVERY_COMPACT_ROW_HEIGHT,
            import_state: discovery_import_state(
                cluster.configured,
                discovery.importing.as_ref() == Some(&import),
                discovery.importing.is_some(),
            ),
        }
        .show(ui, || {
            self.pending_action = Some(ManagedDiscoveryAction::AddTailscale {
                host_name: cluster.host_name.clone(),
            });
        });
    }
}

fn discovery_count(
    loading: bool,
    tool_available: bool,
    error: Option<&String>,
    count: usize,
) -> Option<usize> {
    (!loading && tool_available && error.is_none()).then_some(count)
}

fn show_provider_heading(
    ui: &mut egui::Ui,
    icon: egui::Image<'static>,
    title: &str,
    count: Option<usize>,
) {
    ui.horizontal(|ui| {
        ui.add(icon.fit_to_exact_size(egui::vec2(24.0, 24.0)));
        ui.add_space(spacing::SM);
        ui.label(
            egui::RichText::new(title)
                .font(typography::semibold(typography::SECTION_SIZE))
                .color(gray::_900),
        );
        if let Some(count) = count {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{count} cluster{}",
                        if count == 1 { "" } else { "s" }
                    ))
                    .font(typography::metadata())
                    .color(gray::_600),
                );
            });
        }
    });
}

fn show_discovery_headers(
    ui: &mut egui::Ui,
    name: &str,
    metadata: &str,
    detail: &str,
    action: &str,
    stack_metadata: bool,
) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::hover());
    let mut header = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    header.spacing_mut().item_spacing.x = 0.0;
    if stack_metadata {
        discovery_column_label(
            &mut header,
            DISCOVERY_NAME_COLUMN_WIDTH + DISCOVERY_METADATA_COLUMN_WIDTH,
            &format!("{name} · {metadata}"),
        );
    } else {
        discovery_column_label(&mut header, DISCOVERY_NAME_COLUMN_WIDTH, name);
        discovery_column_label(&mut header, DISCOVERY_METADATA_COLUMN_WIDTH, metadata);
    }
    discovery_column_label(&mut header, DISCOVERY_LOCATION_COLUMN_WIDTH, detail);
    discovery_fixed_column(
        &mut header,
        DISCOVERY_ACTION_COLUMN_WIDTH,
        18.0,
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(action)
                    .font(typography::metadata())
                    .color(gray::_500),
            );
        },
    );
    ui.painter()
        .hline(rect.x_range(), rect.bottom(), surface::border());
}

fn discovery_column_label(ui: &mut egui::Ui, width: f32, label: &str) {
    discovery_fixed_column(
        ui,
        width,
        18.0,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(label)
                    .font(typography::metadata())
                    .color(gray::_500),
            );
        },
    );
}

fn discovery_fixed_column(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    layout: egui::Layout,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let mut column = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    add_contents(&mut column);
}

#[derive(Clone, Copy)]
enum DiscoveryRowStatus {
    Aks,
    Tailscale { online: bool },
}

#[derive(Clone, Copy)]
enum DiscoveryImportState {
    Configured,
    Ready,
    Importing,
    Disabled,
}

#[derive(Clone, Copy)]
struct DiscoveryRow<'a> {
    name: &'a str,
    metadata: &'a str,
    detail: &'a str,
    status: DiscoveryRowStatus,
    stack_metadata: bool,
    height: f32,
    import_state: DiscoveryImportState,
}

impl DiscoveryRow<'_> {
    fn show(self, ui: &mut egui::Ui, on_click: impl FnOnce()) {
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), self.height),
            egui::Sense::hover(),
        );
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row.spacing_mut().item_spacing.x = 0.0;
        if self.stack_metadata {
            discovery_fixed_column(
                &mut row,
                DISCOVERY_NAME_COLUMN_WIDTH + DISCOVERY_METADATA_COLUMN_WIDTH,
                self.height,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    if let Some(color) = self.dot_color() {
                        ui.label(egui::RichText::new("●").color(color));
                        ui.add_space(spacing::SM);
                    }
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), self.height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add_space((self.height - 40.0) / 2.0);
                            show_truncated_label(
                                ui,
                                self.name,
                                typography::semibold(14.0),
                                gray::_900,
                            );
                            show_truncated_label(
                                ui,
                                self.metadata,
                                typography::metadata(),
                                gray::_600,
                            );
                        },
                    );
                },
            );
        } else {
            discovery_fixed_column(
                &mut row,
                DISCOVERY_NAME_COLUMN_WIDTH,
                self.height,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    if let Some(color) = self.dot_color() {
                        ui.label(egui::RichText::new("●").color(color));
                        ui.add_space(spacing::SM);
                    }
                    show_truncated_label(ui, self.name, typography::semibold(14.0), gray::_900);
                },
            );
            discovery_fixed_column(
                &mut row,
                DISCOVERY_METADATA_COLUMN_WIDTH,
                self.height,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    show_truncated_label(ui, self.metadata, typography::metadata(), gray::_600);
                },
            );
        }
        discovery_fixed_column(
            &mut row,
            DISCOVERY_LOCATION_COLUMN_WIDTH,
            self.height,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                show_truncated_label(ui, self.detail, typography::metadata(), self.detail_color());
            },
        );
        discovery_fixed_column(
            &mut row,
            DISCOVERY_ACTION_COLUMN_WIDTH,
            self.height,
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| show_import_button(ui, self.import_state, on_click),
        );
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), surface::border());
    }

    fn dot_color(self) -> Option<egui::Color32> {
        match self.status {
            DiscoveryRowStatus::Aks => Some(status::SUCCESS),
            DiscoveryRowStatus::Tailscale { online: true } => Some(status::SUCCESS),
            DiscoveryRowStatus::Tailscale { online: false } => Some(gray::_400),
        }
    }

    fn detail_color(self) -> egui::Color32 {
        match self.status {
            DiscoveryRowStatus::Aks => gray::_600,
            DiscoveryRowStatus::Tailscale { online: true } => status::SUCCESS,
            DiscoveryRowStatus::Tailscale { online: false } => gray::_500,
        }
    }
}

fn discovery_import_state(
    configured: bool,
    importing: bool,
    import_in_progress: bool,
) -> DiscoveryImportState {
    if configured {
        DiscoveryImportState::Configured
    } else if importing {
        DiscoveryImportState::Importing
    } else if import_in_progress {
        DiscoveryImportState::Disabled
    } else {
        DiscoveryImportState::Ready
    }
}

fn show_truncated_label(ui: &mut egui::Ui, text: &str, font: egui::FontId, color: egui::Color32) {
    ui.add(egui::Label::new(egui::RichText::new(text).font(font).color(color)).truncate())
        .on_hover_text(text);
}

fn show_discovery_loading(ui: &mut egui::Ui, message: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new());
        ui.label(
            egui::RichText::new(message)
                .font(typography::body())
                .color(gray::_600),
        );
    });
}

fn show_unavailable_tool(ui: &mut egui::Ui, tool: &str, message: &str) {
    ui.label(
        egui::RichText::new(format!("{tool} is not installed"))
            .font(typography::body())
            .color(gray::_800),
    );
    ui.label(
        egui::RichText::new(message)
            .font(typography::body())
            .color(gray::_600),
    );
}

fn show_empty_discovery(ui: &mut egui::Ui, message: &str) {
    ui.label(
        egui::RichText::new(message)
            .font(typography::body())
            .color(gray::_600),
    );
}

fn show_discovery_error(ui: &mut egui::Ui, error: &str) {
    ui.label(
        egui::RichText::new(error)
            .font(typography::body())
            .color(status::CRITICAL),
    );
}

fn show_discovery_warning(ui: &mut egui::Ui, warning: &str) {
    egui::Frame::new()
        .fill(surface::warning_fill())
        .stroke(surface::warning_border())
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            spacing::MD as i8,
            spacing::MD as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(spacing::XL);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("⚠").color(status::WARNING));
                ui.label(
                    egui::RichText::new(warning)
                        .font(typography::metadata())
                        .color(status::WARNING_TEXT),
                );
            });
        });
}

fn show_import_button(ui: &mut egui::Ui, state: DiscoveryImportState, on_click: impl FnOnce()) {
    if matches!(state, DiscoveryImportState::Configured) {
        ui.add_enabled_ui(false, |ui| {
            TailwindButton::secondary("Already in kubeconfig")
                .size(ButtonSize::Md)
                .show(ui)
        });
    } else if ui
        .add_enabled_ui(!matches!(state, DiscoveryImportState::Disabled), |ui| {
            TailwindButton::primary(if matches!(state, DiscoveryImportState::Importing) {
                "Adding…"
            } else {
                "Add to kubeconfig"
            })
            .size(ButtonSize::Md)
            .show(ui)
            .clicked()
        })
        .inner
    {
        on_click();
    }
}

#[derive(Debug)]
struct ManagedDiscoveryEffect(ManagedDiscoveryAction);

impl GlobalBladeEffect for ManagedDiscoveryEffect {
    fn apply(
        self: Box<Self>,
        context: &mut GlobalBladeEffectContext<'_>,
        navigation: &mut GlobalBladeNavigation<'_>,
    ) {
        let discovery = &mut context.ui_state.managed_cluster_discovery;
        discovery.error = None;
        match self.0 {
            ManagedDiscoveryAction::Refresh => {
                discovery.loading = true;
                navigation
                    .commands_to_send()
                    .push(Box::new(LoadManagedClusterDiscovery));
            }
            ManagedDiscoveryAction::AddAks {
                subscription_id,
                resource_group,
                cluster_name,
            } => {
                discovery.importing = Some(ManagedClusterImport::Aks {
                    subscription_id: subscription_id.clone(),
                    resource_group: resource_group.clone(),
                    cluster_name: cluster_name.clone(),
                });
                navigation.commands_to_send().push(Box::new(AddAksCluster {
                    subscription_id,
                    resource_group,
                    cluster_name,
                }));
            }
            ManagedDiscoveryAction::AddTailscale { host_name } => {
                discovery.importing = Some(ManagedClusterImport::Tailscale {
                    host_name: host_name.clone(),
                });
                navigation
                    .commands_to_send()
                    .push(Box::new(AddTailscaleCluster { host_name }));
            }
        }
    }
}

fn show_settings_introduction(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("APPLICATION")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(spacing::XS);
    ui.label(
        egui::RichText::new(
            "Configure local tools and display preferences.\nThese settings apply only to this application on this device.",
        )
        .font(typography::body())
        .color(gray::_600),
    );
}

fn show_update_status(ui: &mut egui::Ui, update_status: &UpdateStatus) {
    if matches!(
        update_status,
        UpdateStatus::LocalBuild | UpdateStatus::NotIncluded
    ) {
        return;
    }

    ui.add_space(spacing::XL);
    ui.label(
        egui::RichText::new("Application updates")
            .font(typography::section_heading())
            .color(gray::_900),
    );
    ui.add_space(spacing::XS);
    ui.label(
        egui::RichText::new(update_status.summary())
            .font(typography::body())
            .color(gray::_600),
    );
}

fn show_terminal_launcher(
    ui: &mut egui::Ui,
    draft: &mut TerminalLaunchSettings,
    error: &mut Option<String>,
) {
    ui.label(
        egui::RichText::new("Terminal launcher")
            .font(typography::section_heading())
            .color(gray::_900),
    );
    ui.add_space(spacing::SM);
    ui.label(
        egui::RichText::new("Choose how shells open on this computer.")
            .font(typography::body())
            .color(gray::_600),
    );
    ui.add_space(spacing::XL);

    let automatic = draft.custom_template.is_none();
    if launcher_choice(
        ui,
        automatic,
        "Automatic",
        "Use your system’s preferred terminal.",
        None,
        false,
        None,
    ) {
        draft.custom_template = None;
        *error = None;
    }
    ui.add_space(spacing::LG);
    let custom_launcher_clicked = {
        let template_error = error.clone();
        let template_error =
            template_error.filter(|error| error.starts_with("The launcher template"));
        let template_invalid = template_error.is_some();
        let template = draft.custom_template.as_mut();
        launcher_choice(
            ui,
            !automatic,
            "Custom launcher",
            "Use a command template for your preferred terminal.",
            template,
            template_invalid,
            template_error.as_deref(),
        )
    };
    if custom_launcher_clicked && draft.custom_template.is_none() {
        draft.custom_template = Some(String::new());
    }
}

fn show_debug_image_presets(
    ui: &mut egui::Ui,
    draft: &mut TerminalLaunchSettings,
    error: &mut Option<String>,
) {
    ui.label(
        egui::RichText::new("Debug images")
            .font(typography::section_heading())
            .color(gray::_900),
    );
    ui.add_space(spacing::SM);
    ui.label(
        egui::RichText::new(
            "Choose the images and profiles offered for Debug images and Pod debug shells. Node sessions create a debug Pod with the node filesystem available at /host; Pod sessions add an ephemeral debug container.",
        )
        .font(typography::body())
        .color(gray::_600),
    );
    ui.add_space(spacing::LG);

    let mut remove_preset = None;
    let table_width = ui.available_width();
    let original_item_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.y = 0.0;
    show_debug_image_preset_table_header(ui, table_width);
    let moved = ReorderableTable::new("debug-image-presets", DEBUG_IMAGE_TABLE_ROW_HEIGHT).show(
        ui,
        &mut draft.debug_image_presets,
        table_width,
        |ui, preset, index, handle| {
            show_debug_image_preset_row(ui, table_width, preset, index, handle, &mut remove_preset);
        },
        |ui, preset| show_debug_image_preset_preview_row(ui, table_width, preset),
    );
    ui.spacing_mut().item_spacing = original_item_spacing;
    if moved {
        *error = None;
    }
    if let Some(index) = remove_preset {
        draft.debug_image_presets.remove(index);
        *error = None;
    }
    if TailwindButton::icon(
        icons::plus_icon()
            .fit_to_exact_size(egui::Vec2::splat(16.0))
            .tint(indigo::_600),
    )
    .variant(ButtonVariant::Secondary)
    .size(ButtonSize::Sm)
    .accessibility_label("Add debug image")
    .show(ui)
    .clicked()
    {
        draft.debug_image_presets.push(DebugImagePreset {
            name: String::new(),
            image: String::new(),
            profile: DebugProfile::General,
        });
        *error = None;
    }
    if let Some(error) = error
        .as_deref()
        .filter(|error| !error.starts_with("The launcher template"))
    {
        ui.add_space(spacing::MD);
        show_validation_error(ui, "Debug image settings need attention", error);
    }
}

fn centered_table_control_ui(ui: &mut egui::Ui) -> egui::Ui {
    let cell_rect = ui.available_rect_before_wrap();
    let control_rect = egui::Rect::from_min_max(
        egui::pos2(cell_rect.left(), cell_rect.center().y - 16.0),
        egui::pos2(cell_rect.right(), cell_rect.center().y + 16.0),
    );
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(control_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    )
}

fn show_debug_image_preset_table_header(ui: &mut egui::Ui, table_width: f32) {
    let image_column_width = debug_image_column_width(table_width);
    let original_item_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.horizontal(|ui| {
        show_debug_image_table_header_cell(ui, DEBUG_IMAGE_REORDER_COLUMN_WIDTH, "");
        show_debug_image_table_header_cell(ui, DEBUG_IMAGE_NAME_COLUMN_WIDTH, "Name");
        show_debug_image_table_header_cell(ui, image_column_width, "Image");
        show_debug_image_table_header_cell(ui, DEBUG_IMAGE_PROFILE_COLUMN_WIDTH, "Debug profile");
        show_debug_image_table_header_cell(ui, DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH, "");
    });
    ui.spacing_mut().item_spacing = original_item_spacing;
}

fn show_debug_image_preset_row(
    ui: &mut egui::Ui,
    table_width: f32,
    preset: &mut DebugImagePreset,
    index: usize,
    drag_handle: &ReorderHandle,
    remove_preset: &mut Option<usize>,
) -> egui::Response {
    let image_column_width = debug_image_column_width(table_width);
    let original_item_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.x = 0.0;
    let response = ui
        .allocate_ui_with_layout(
            egui::vec2(table_width, DEBUG_IMAGE_TABLE_ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                show_debug_image_table_cell(ui, DEBUG_IMAGE_REORDER_COLUMN_WIDTH, |ui| {
                    let (handle_rect, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                    let mut handle_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(handle_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    icons::bars_3(&mut handle_ui, 16.0, gray::_500);
                    let response = ui
                        .interact(
                            handle_rect,
                            ui.id().with(("debug-image-preset-handle", index)),
                            egui::Sense::drag(),
                        )
                        .with_pointing_hand();
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            ui.is_enabled(),
                            format!("Reorder {}", preset.name),
                        )
                    });
                    drag_handle.register(&response);
                });
                show_debug_image_table_cell(ui, DEBUG_IMAGE_NAME_COLUMN_WIDTH, |ui| {
                    let mut control_ui = centered_table_control_ui(ui);
                    TailwindTextInput::new(&mut preset.name)
                        .id_salt(("debug-image-name", index))
                        .accessibility_label(format!("Debug image {} name", index + 1))
                        .show(&mut control_ui);
                });
                show_debug_image_table_cell(ui, image_column_width, |ui| {
                    let mut control_ui = centered_table_control_ui(ui);
                    TailwindTextInput::new(&mut preset.image)
                        .id_salt(("debug-image-image", index))
                        .accessibility_label(format!("Debug image {} image", index + 1))
                        .show(&mut control_ui);
                });
                show_debug_image_table_cell(ui, DEBUG_IMAGE_PROFILE_COLUMN_WIDTH, |ui| {
                    let mut combobox_ui = centered_table_control_ui(ui);
                    let response = TailwindCombobox::new(("debug-image-profile", index))
                        .accessibility_label(format!("Debug image {} debug profile", index + 1))
                        .selected_text(preset.profile.label())
                        .width(150.0)
                        .compact()
                        .filter_by(|profile: &DebugProfile| profile.label())
                        .show_items(&mut combobox_ui, &DebugProfile::ALL, |options, profile| {
                            if options
                                .item(profile.label(), *profile == preset.profile)
                                .clicked()
                            {
                                preset.profile = *profile;
                            }
                        });
                    response.response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::ComboBox,
                            combobox_ui.is_enabled(),
                            format!("Debug image {} debug profile", index + 1),
                        )
                    });
                });
                show_debug_image_table_cell(ui, DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH, |ui| {
                    let mut control_ui = centered_table_control_ui(ui);
                    if TailwindButton::icon(
                        icons::trash_icon()
                            .fit_to_exact_size(egui::Vec2::splat(16.0))
                            .tint(status::DANGER),
                    )
                    .variant(ButtonVariant::Secondary)
                    .size(ButtonSize::Sm)
                    .accessibility_label(format!("Remove {}", preset.name))
                    .show(&mut control_ui)
                    .clicked()
                    {
                        *remove_preset = Some(index);
                    }
                });
            },
        )
        .response;
    ui.spacing_mut().item_spacing = original_item_spacing;
    response
}

/// Render the floating drag preview without registering a second set of
/// editable controls or accessibility nodes for the preset.
fn show_debug_image_preset_preview_row(
    ui: &mut egui::Ui,
    table_width: f32,
    preset: &DebugImagePreset,
) {
    let image_column_width = debug_image_column_width(table_width);
    let original_item_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.allocate_ui_with_layout(
        egui::vec2(table_width, DEBUG_IMAGE_TABLE_ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            show_debug_image_table_cell(ui, DEBUG_IMAGE_REORDER_COLUMN_WIDTH, |ui| {
                icons::bars_3(ui, 16.0, gray::_500);
            });
            show_debug_image_table_cell(ui, DEBUG_IMAGE_NAME_COLUMN_WIDTH, |ui| {
                show_debug_image_preview_text_input(ui, &preset.name);
            });
            show_debug_image_table_cell(ui, image_column_width, |ui| {
                show_debug_image_preview_text_input(ui, &preset.image);
            });
            show_debug_image_table_cell(ui, DEBUG_IMAGE_PROFILE_COLUMN_WIDTH, |ui| {
                show_debug_image_preview_combobox(ui, preset.profile.label());
            });
            show_debug_image_table_cell(ui, DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH, |ui| {
                show_debug_image_preview_remove_button(ui);
            });
        },
    );
    ui.spacing_mut().item_spacing = original_item_spacing;
}

fn show_debug_image_preview_text_input(ui: &mut egui::Ui, value: &str) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 21.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, radius::control(), WHITE);
    ui.painter().rect_stroke(
        rect,
        radius::control(),
        surface::control_border(),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(4.0, 0.0),
        egui::Align2::LEFT_CENTER,
        value,
        typography::body(),
        gray::_800,
    );
}

fn show_debug_image_preview_combobox(ui: &mut egui::Ui, label: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 32.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, radius::control(), WHITE);
    ui.painter().rect_stroke(
        rect,
        radius::control(),
        surface::control_border(),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(spacing::MD, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        typography::metadata(),
        gray::_800,
    );
    let icon_rect = egui::Rect::from_center_size(
        rect.right_center() - egui::vec2(spacing::MD, 0.0),
        egui::Vec2::splat(16.0),
    );
    let mut icon_ui = ui.new_child(egui::UiBuilder::new().max_rect(icon_rect));
    icons::chevron_down(&mut icon_ui, 16.0, gray::_400);
}

fn show_debug_image_preview_remove_button(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(32.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, radius::control(), WHITE);
    ui.painter().rect_stroke(
        rect,
        radius::control(),
        surface::control_border(),
        egui::StrokeKind::Inside,
    );
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(16.0));
    let mut icon_ui = ui.new_child(egui::UiBuilder::new().max_rect(icon_rect));
    icon_ui.add(
        icons::trash_icon()
            .fit_to_exact_size(egui::Vec2::splat(16.0))
            .tint(status::DANGER),
    );
}

fn debug_image_column_width(table_width: f32) -> f32 {
    (table_width
        - DEBUG_IMAGE_REORDER_COLUMN_WIDTH
        - DEBUG_IMAGE_NAME_COLUMN_WIDTH
        - DEBUG_IMAGE_PROFILE_COLUMN_WIDTH
        - DEBUG_IMAGE_ACTIONS_COLUMN_WIDTH)
        .max(120.0)
}

fn show_debug_image_table_header_cell(ui: &mut egui::Ui, width: f32, label: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, DEBUG_IMAGE_TABLE_HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, TABLE_HEADER_BACKGROUND);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, TABLE_BORDER),
    );
    let mut cell_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cell_ui.add_space(spacing::LG);
    cell_ui.label(
        egui::RichText::new(label)
            .font(typography::body())
            .color(gray::_900)
            .strong(),
    );
}

fn show_debug_image_table_cell(ui: &mut egui::Ui, width: f32, content: impl FnOnce(&mut egui::Ui)) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, DEBUG_IMAGE_TABLE_ROW_HEIGHT),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, CONTENT_BACKGROUND);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, TABLE_BORDER),
    );
    let mut cell_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    cell_ui.add_space(spacing::LG);
    content(&mut cell_ui);
}

fn launcher_choice(
    ui: &mut egui::Ui,
    selected: bool,
    title: &str,
    description: &str,
    template: Option<&mut String>,
    template_invalid: bool,
    template_error: Option<&str>,
) -> bool {
    let stroke = if selected {
        egui::Stroke::new(1.0, indigo::_500)
    } else {
        surface::muted_border()
    };
    egui::Frame::new()
        .fill(if selected { indigo::_50 } else { WHITE })
        .stroke(stroke)
        .corner_radius(radius::surface())
        .inner_margin(egui::Margin::same(spacing::XXL as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let choice_clicked = ui
                .allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), CHOICE_CONTENT_MIN_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().icon_width = 20.0;
                        ui.spacing_mut().icon_width_inner = 12.0;
                        let response = ui.radio(selected, "").with_pointing_hand();
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::RadioButton,
                                ui.is_enabled(),
                                selected,
                                title,
                            )
                        });
                        ui.add_space(spacing::SM);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(title)
                                    .font(typography::semibold(18.0))
                                    .color(gray::_800),
                            );
                            ui.add_space(spacing::XS);
                            ui.label(
                                egui::RichText::new(description)
                                    .font(typography::body())
                                    .color(gray::_600),
                            );
                        });
                        response.clicked()
                    },
                )
                .inner;
            if let Some(template) = template {
                ui.add_space(spacing::XXL);
                ui.label(
                    egui::RichText::new("Command template")
                        .font(typography::body())
                        .color(gray::_800),
                );
                ui.add_space(spacing::SM);
                show_command_template_input(ui, template, template_invalid);
                ui.add_space(spacing::MD);
                egui::Frame::new()
                    .fill(indigo::_100)
                    .stroke(egui::Stroke::new(1.0, indigo::_200))
                    .corner_radius(radius::surface())
                    .inner_margin(egui::Margin::same(spacing::LG as i8))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            let (icon_rect, _) = ui
                                .allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::hover());
                            ui.painter().circle_stroke(
                                icon_rect.center(),
                                9.0,
                                egui::Stroke::new(1.5, indigo::_600),
                            );
                            ui.painter().text(
                                icon_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "i",
                                typography::semibold(14.0),
                                indigo::_600,
                            );
                            ui.add_space(spacing::SM);
                            ui.label(template_guidance());
                        });
                    });
                if let Some(error) = template_error {
                    ui.add_space(spacing::LG);
                    show_validation_error(ui, "Command template needs attention", error);
                }
            }
            choice_clicked
        })
        .inner
}

fn show_command_template_input(
    ui: &mut egui::Ui,
    template: &mut String,
    invalid: bool,
) -> egui::Response {
    let id = ui.make_persistent_id("terminal-command-template");
    let focused = ui.memory(|memory| memory.has_focus(id));
    let stroke = if invalid {
        egui::Stroke::new(1.0, status::DANGER)
    } else if focused {
        egui::Stroke::new(1.0, indigo::_500)
    } else {
        surface::control_border()
    };
    let response = egui::Frame::new()
        .fill(WHITE)
        .stroke(stroke)
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            spacing::MD as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            let full_width = ui.available_width() + spacing::XL;
            ui.set_min_width(full_width);
            ui.add_sized(
                egui::vec2(full_width, 20.0),
                egui::TextEdit::singleline(template)
                    .id(id)
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 2)))
                    .font(typography::monospace())
                    .text_color(gray::_800)
                    .hint_text("alacritty -e {command}"),
            )
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::TextEdit,
            ui.is_enabled(),
            "Command template",
        )
    });
    response
}

fn template_guidance() -> egui::text::LayoutJob {
    let mut guidance = egui::text::LayoutJob::default();
    let text = egui::TextFormat {
        font_id: typography::body(),
        color: indigo::_800,
        ..Default::default()
    };
    guidance.append("Use ", 0.0, text.clone());
    guidance.append(
        "{command}",
        0.0,
        egui::TextFormat {
            font_id: typography::monospace(),
            color: indigo::_900,
            ..Default::default()
        },
    );
    guidance.append(
        " as the placeholder.\nIt is replaced with the complete kubectl shell command.",
        0.0,
        text,
    );
    guidance
}

fn show_validation_error(ui: &mut egui::Ui, title: &str, error: &str) {
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, status::DANGER))
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::same(spacing::MD as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .font(typography::semibold(typography::BODY_SIZE))
                    .color(status::DANGER),
            );
            ui.add_space(spacing::XS);
            ui.label(
                egui::RichText::new(error)
                    .font(typography::metadata())
                    .color(gray::_700),
            );
        });
}
