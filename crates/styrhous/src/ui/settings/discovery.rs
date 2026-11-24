use super::discovery_table::{
    DiscoveryRow, ManagedDiscoveryEffect, cluster_count_label, discovery_count,
    discovery_import_state, discovery_tag_summary, provider_card, show_discovery_error,
    show_discovery_headers, show_discovery_loading, show_discovery_warning, show_empty_discovery,
    show_install_hint,
};
use super::*;
use crate::ui::blade_text::gap;
use components::design::theme::Tone;
use egui::{Align, Layout, vec2};

#[derive(Debug, Default)]

pub(crate) struct ManagedClusterDiscoveryBlade {
    initial_load: bool,
    /// The account and profile picked in the selects.
    account: Option<String>,
    profile: Option<AwsProfile>,
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
    ScanEks {
        account_id: String,
        profile: AwsProfile,
    },
    AddEks {
        account_id: String,
        region: String,
        name: String,
        profile: AwsProfile,
    },
}

impl GlobalBladeContent for ManagedClusterDiscoveryBlade {
    fn description(&self) -> components::BladeDescription {
        components::BladeDescription::named("Cluster discovery")
    }

    fn sidebar_destination(&self) -> Option<crate::ui::global_blade::SidebarDestination> {
        Some(crate::ui::global_blade::SidebarDestination::Settings)
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        _layer: components::BladeLayer,
        context: &mut GlobalBladeRenderContext<'_>,
    ) -> GlobalBladeRenderResult {
        let discovery = context.managed_cluster_discovery();
        components::blade_title_with_actions(ui, None, "Cluster discovery", |ui| {
            // Laid out from the right: the more-actions trigger, then Refresh.
            crate::ui::blade_text::blade_more_actions(ui);
            let idle = !discovery.loading
                && !discovery.aws.account_loading
                && !discovery.aws.scan_loading
                && discovery.importing.is_none();
            if discovery_refresh_button(ui, idle).clicked() {
                self.pending_action = Some(ManagedDiscoveryAction::Refresh);
            }
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
        type_scale::BODY.label(
            ui,
            "Find clusters in your cloud accounts and add them to your kubeconfig. New clusters appear in the rail automatically.",
            ui.tokens().text.secondary,
        );
        gap(ui, space::S20);
        if let Some(error) = &discovery.error {
            show_discovery_error(ui, error);
            gap(ui, space::S12);
        }
        self.show_azure(ui, discovery);
        gap(ui, space::S12);
        self.show_aws(ui, discovery);
        gap(ui, space::S12);
        self.show_tailscale(ui, discovery);
        GlobalBladeRenderResult::default()
    }

    fn take_effect(&mut self) -> Option<Box<dyn GlobalBladeEffect>> {
        self.pending_action
            .take()
            .map(|action| Box::new(ManagedDiscoveryEffect(action)) as Box<dyn GlobalBladeEffect>)
    }
}

pub(super) fn discovery_refresh_button(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    Button::ghost("Refresh")
        .size(ButtonSize::Sm)
        .leading_icon(icons::Icon::ArrowPath)
        .enabled(enabled)
        .show(ui)
}

/// The chip of a provider whose tool is missing, found, or still working.
fn provider_status(
    loading: bool,
    tool_available: bool,
    error: Option<&String>,
    count: usize,
    missing: &'static str,
) -> Option<(String, Tone)> {
    if loading {
        return None;
    }
    if !tool_available {
        return Some((missing.to_owned(), Tone::Neutral));
    }
    discovery_count(loading, tool_available, error, count)
        .map(|count| (cluster_count_label(count), Tone::Accent))
}

/// `add_contents` in a column `width` points wide.
fn fixed_width(ui: &mut egui::Ui, width: f32, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        add_contents(ui);
    });
}

impl ManagedClusterDiscoveryBlade {
    /// The account and profile the selects show: the user's choice while it
    /// is still on offer, else the scanned ones, else the first of each.
    fn aws_selection(
        &mut self,
        aws: &super::super::state::AwsDiscoveryState,
    ) -> Option<(String, AwsProfile)> {
        let account = self
            .account
            .as_ref()
            .and_then(|id| aws.accounts.iter().find(|account| &account.id == id))
            .or_else(|| {
                aws.selected_account_id
                    .as_ref()
                    .and_then(|id| aws.accounts.iter().find(|account| &account.id == id))
            })
            .or_else(|| aws.accounts.first())?;
        let profile = self
            .profile
            .as_ref()
            .filter(|profile| account.profiles.contains(profile))
            .or_else(|| {
                aws.selected_profile
                    .as_ref()
                    .filter(|profile| account.profiles.contains(profile))
            })
            .or_else(|| account.profiles.first())?
            .clone();
        Some((account.id.clone(), profile))
    }

    fn show_aws(
        &mut self,
        ui: &mut egui::Ui,
        discovery: &super::super::state::ManagedClusterDiscoveryState,
    ) {
        let aws = &discovery.aws;
        let scanned = aws.selected_account_id.is_some();
        let status = provider_status(
            aws.account_loading || aws.scan_loading,
            aws.cli_installed,
            aws.scan_error.as_ref().or(aws.account_error.as_ref()),
            aws.clusters.len(),
            "CLI not installed",
        )
        .filter(|_| scanned || !aws.cli_installed);
        let selection = self.aws_selection(aws);
        provider_card(
            ui,
            icons::Icon::Cloud,
            Tone::Warning,
            "Amazon Elastic Kubernetes Service",
            status.as_ref().map(|(text, tone)| (text.as_str(), *tone)),
            |ui| {
                if aws.account_loading {
                    show_discovery_loading(ui, "Resolving AWS CLI accounts…");
                } else if !aws.cli_installed {
                    show_install_hint(
                        ui,
                        "Install the AWS CLI and configure a profile",
                        "aws configure",
                        "to discover EKS clusters.",
                    );
                } else if let Some(error) = &aws.account_error {
                    show_discovery_error(ui, error);
                } else {
                    self.show_aws_controls(ui, discovery, selection);
                    self.show_aws_results(ui, discovery);
                }
                if let Some(warning) = &aws.account_warning {
                    gap(ui, space::S8);
                    show_discovery_warning(ui, warning);
                }
                if let Some(warning) = &aws.scan_warning {
                    gap(ui, space::S8);
                    show_discovery_warning(ui, warning);
                }
            },
        );
    }

    /// The account and profile selects with the scan button.
    fn show_aws_controls(
        &mut self,
        ui: &mut egui::Ui,
        discovery: &super::super::state::ManagedClusterDiscoveryState,
        selection: Option<(String, AwsProfile)>,
    ) {
        let aws = &discovery.aws;
        let Some((account_id, profile)) = selection else {
            show_empty_discovery(
                ui,
                "No AWS accounts were found for the configured profiles.",
            );
            return;
        };
        let profiles = aws
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .map(|account| account.profiles.clone())
            .unwrap_or_default();
        let ids = aws
            .accounts
            .iter()
            .map(|account| account.id.clone())
            .collect::<Vec<_>>();
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), 0.0),
            Layout::left_to_right(Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.x = space::S12;
                fixed_width(ui, AWS_ACCOUNT_SELECT_WIDTH, |ui| {
                    FormField::new("AWS account").show(ui, |ui| {
                        // The account is named by its id: the CLI gives no friendly name.
                        let response = Combobox::new("aws-account")
                            .accessibility_label("AWS account")
                            .selected_text(account_id.clone())
                            .width(AWS_ACCOUNT_SELECT_WIDTH)
                            .searchable(false)
                            .filter_by(|id: &String| id.as_str())
                            .show_items(ui, &ids, |options, id| {
                                if options.item(id.as_str(), *id == account_id).clicked() {
                                    self.account = Some(id.clone());
                                    self.profile = None;
                                }
                            });
                        response.response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::ComboBox,
                                true,
                                "AWS account",
                            )
                        });
                    })
                });
                fixed_width(ui, AWS_PROFILE_SELECT_WIDTH, |ui| {
                    FormField::new("Profile").show(ui, |ui| {
                        let response = Combobox::new("aws-profile")
                            .accessibility_label("AWS profile")
                            .selected_text(profile.label())
                            .width(AWS_PROFILE_SELECT_WIDTH)
                            .searchable(false)
                            .filter_by(|profile: &AwsProfile| profile.label())
                            .show_items(ui, &profiles, |options, candidate| {
                                if options
                                    .item(candidate.label(), *candidate == profile)
                                    .clicked()
                                {
                                    self.profile = Some(candidate.clone());
                                }
                            });
                        response.response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::ComboBox,
                                true,
                                "AWS profile",
                            )
                        });
                    })
                });
                let scanning = aws.scan_loading;
                ui.vertical(|ui| {
                    // Level with the selects, under their labels.
                    ui.add_space(type_scale::FIELD_LABEL.line_height() + space::LABEL_TO_CONTROL);
                    if Button::secondary(if scanning {
                        "Scanning…"
                    } else {
                        "Scan clusters"
                    })
                    .leading_icon(icons::Icon::ArrowPath)
                    .accessibility_label("Scan clusters")
                    .enabled(!discovery.loading && !scanning && discovery.importing.is_none())
                    .show(ui)
                    .clicked()
                    {
                        self.pending_action = Some(ManagedDiscoveryAction::ScanEks {
                            account_id: account_id.clone(),
                            profile: profile.clone(),
                        });
                    }
                });
            },
        );
    }

    /// What the last scan found, as a line of words and a table.
    fn show_aws_results(
        &mut self,
        ui: &mut egui::Ui,
        discovery: &super::super::state::ManagedClusterDiscoveryState,
    ) {
        let aws = &discovery.aws;
        if let Some(error) = &aws.import_error {
            gap(ui, space::S12);
            show_discovery_error(ui, error);
        }
        if aws.scan_loading {
            gap(ui, space::S12);
            show_discovery_loading(ui, "Checking enabled Regions for EKS clusters…");
        } else if let Some(error) = &aws.scan_error {
            gap(ui, space::S12);
            show_discovery_error(ui, error);
        } else if let (Some(account_id), Some(profile)) =
            (&aws.selected_account_id, &aws.selected_profile)
        {
            gap(ui, space::S16);
            let mut regions = aws
                .clusters
                .iter()
                .map(|cluster| cluster.region.as_str())
                .collect::<Vec<_>>();
            regions.sort_unstable();
            regions.dedup();
            let found = match regions.as_slice() {
                [] => format!(
                    "No EKS clusters found for account {account_id} · {}",
                    profile.label()
                ),
                [region] => format!(
                    "{} found in {region}",
                    cluster_count_label(aws.clusters.len())
                ),
                _ => format!(
                    "{} found in {} Regions",
                    cluster_count_label(aws.clusters.len()),
                    regions.len()
                ),
            };
            type_scale::META.label(ui, &found, ui.tokens().text.muted);
            if !aws.clusters.is_empty() {
                gap(ui, space::S8);
                show_discovery_headers(ui, ["Cluster", "Region", "Tags"]);
                for cluster in &aws.clusters {
                    self.show_eks_row(ui, cluster, discovery);
                }
            }
        }
    }

    fn show_eks_row(
        &mut self,
        ui: &mut egui::Ui,
        cluster: &AvailableEksCluster,
        discovery: &super::super::state::ManagedClusterDiscoveryState,
    ) {
        let import = ManagedClusterImport::Eks {
            account_id: cluster.account_id.clone(),
            region: cluster.region.clone(),
            name: cluster.name.clone(),
            profile: cluster.profile.clone(),
        };
        let tags = discovery_tag_summary(&cluster.tags).unwrap_or_default();
        DiscoveryRow {
            cells: [&cluster.name, &cluster.region, &tags],
            second_tone: None,
            import_state: discovery_import_state(
                cluster.configured,
                discovery.importing.as_ref() == Some(&import),
                discovery.importing.is_some() || discovery.loading,
            ),
        }
        .show(ui, || {
            self.pending_action = Some(ManagedDiscoveryAction::AddEks {
                account_id: cluster.account_id.clone(),
                region: cluster.region.clone(),
                name: cluster.name.clone(),
                profile: cluster.profile.clone(),
            });
        });
    }

    fn show_azure(
        &mut self,
        ui: &mut egui::Ui,
        discovery: &super::super::state::ManagedClusterDiscoveryState,
    ) {
        let status = provider_status(
            discovery.loading,
            discovery.tools.azure_cli,
            discovery.azure_error.as_ref(),
            discovery.aks_clusters.len(),
            "CLI not installed",
        );
        provider_card(
            ui,
            icons::Icon::Cloud,
            Tone::Accent,
            "Azure Kubernetes Service",
            status.as_ref().map(|(text, tone)| (text.as_str(), *tone)),
            |ui| {
                if discovery.loading {
                    show_discovery_loading(ui, "Checking Azure subscriptions for AKS clusters…");
                } else if !discovery.tools.azure_cli {
                    show_install_hint(
                        ui,
                        "Install the Azure CLI and run",
                        "az login",
                        "to discover AKS clusters.",
                    );
                } else if let Some(error) = &discovery.azure_error {
                    show_discovery_error(ui, error);
                } else if discovery.aks_clusters.is_empty() {
                    show_empty_discovery(
                        ui,
                        "No AKS clusters were found for the signed-in Azure CLI account.",
                    );
                } else {
                    show_discovery_headers(ui, ["Cluster", "Location", "Tags"]);
                    for cluster in &discovery.aks_clusters {
                        self.show_aks_row(ui, cluster, discovery);
                    }
                }
                if let Some(warning) = &discovery.azure_warning {
                    gap(ui, space::S8);
                    show_discovery_warning(ui, warning);
                }
            },
        );
    }

    fn show_tailscale(
        &mut self,
        ui: &mut egui::Ui,
        discovery: &super::super::state::ManagedClusterDiscoveryState,
    ) {
        let status = provider_status(
            discovery.loading,
            discovery.tools.tailscale,
            discovery.tailscale_error.as_ref(),
            discovery.tailscale_clusters.len(),
            "Not installed",
        );
        provider_card(
            ui,
            icons::Icon::Signal,
            Tone::Neutral,
            "Tailscale",
            status.as_ref().map(|(text, tone)| (text.as_str(), *tone)),
            |ui| {
                if discovery.loading {
                    type_scale::UI.label(
                        ui,
                        "Checking Tailscale peers for Kubernetes API proxies…",
                        ui.tokens().text.secondary,
                    );
                } else if !discovery.tools.tailscale {
                    show_empty_discovery(
                        ui,
                        "Install Tailscale and sign in to discover Kubernetes API proxies on your tailnet.",
                    );
                } else if let Some(error) = &discovery.tailscale_error {
                    show_discovery_error(ui, error);
                } else if discovery.tailscale_clusters.is_empty() {
                    show_empty_discovery(
                        ui,
                        "No Tailscale peers tagged tag:k8s-operator were found.",
                    );
                } else {
                    show_discovery_headers(ui, ["Cluster", "Connection", "Hostname"]);
                    for cluster in &discovery.tailscale_clusters {
                        self.show_tailscale_row(ui, cluster, discovery);
                    }
                }
            },
        );
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
        let tags = discovery_tag_summary(&cluster.tags).unwrap_or_default();
        DiscoveryRow {
            cells: [&cluster.name, &cluster.location, &tags],
            second_tone: None,
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
        DiscoveryRow {
            cells: [
                &cluster.host_name,
                if cluster.online { "Online" } else { "Offline" },
                &cluster.dns_name,
            ],
            second_tone: Some(if cluster.online {
                Tone::Success
            } else {
                Tone::Neutral
            }),
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
