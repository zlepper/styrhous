use super::discovery_table::{
    DiscoveryRow, DiscoveryRowStatus, ManagedDiscoveryEffect, discovery_count,
};
use super::*;

#[derive(Debug, Default)]

pub(crate) struct ManagedClusterDiscoveryBlade {
    initial_load: bool,
    pending_action: Option<ManagedDiscoveryAction>,
}

#[derive(Debug)]
pub(super) enum ManagedDiscoveryAction {
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

pub(super) fn discovery_refresh_button(ui: &mut egui::Ui) -> egui::Response {
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
        discovery: &super::super::state::ManagedClusterDiscoveryState,
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
        discovery: &super::super::state::ManagedClusterDiscoveryState,
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
        discovery: &super::super::state::ManagedClusterDiscoveryState,
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
        discovery: &super::super::state::ManagedClusterDiscoveryState,
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
