use super::discovery::ManagedDiscoveryAction;
use super::*;
use std::collections::BTreeMap;

pub(super) fn discovery_tag_summary(tags: &BTreeMap<String, String>) -> Option<String> {
    if tags.is_empty() {
        return None;
    }

    let mut summary = String::from("Tags: ");
    for (index, (key, value)) in tags.iter().enumerate() {
        if index > 0 {
            summary.push_str(" · ");
        }
        summary.push_str(key);
        summary.push('=');
        summary.push_str(value);
    }
    Some(summary)
}

pub(super) fn discovery_count(
    loading: bool,
    tool_available: bool,
    error: Option<&String>,
    count: usize,
) -> Option<usize> {
    (!loading && tool_available && error.is_none()).then_some(count)
}

pub(super) fn show_provider_heading(
    ui: &mut egui::Ui,
    icon: egui::Image<'static>,
    title: &str,
    count: Option<usize>,
) {
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        ui.add(icon.fit_to_exact_size(egui::vec2(24.0, 24.0)));
        ui.add_space(space::S8);
        ui.label(
            egui::RichText::new(title)
                .font(type_scale::TITLE.font_id(ui.ctx()))
                .color(theme.text.primary),
        );
        if let Some(count) = count {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{count} cluster{}",
                        if count == 1 { "" } else { "s" }
                    ))
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(theme.text.secondary),
                );
            });
        }
    });
}

pub(super) fn show_discovery_headers(
    ui: &mut egui::Ui,
    name: &str,
    metadata: &str,
    detail: &str,
    action: &str,
    stack_metadata: bool,
) {
    let theme = ui.tokens();
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
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(theme.text.muted),
            );
        },
    );
    ui.painter()
        .hline(rect.x_range(), rect.bottom(), stroke::divider(&theme));
}

pub(super) fn discovery_column_label(ui: &mut egui::Ui, width: f32, label: &str) {
    let theme = ui.tokens();
    discovery_fixed_column(
        ui,
        width,
        18.0,
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(
                egui::RichText::new(label)
                    .font(type_scale::META.font_id(ui.ctx()))
                    .color(theme.text.muted),
            );
        },
    );
}

pub(super) fn discovery_fixed_column(
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
pub(super) enum DiscoveryRowStatus {
    Aks,
    Eks,
    Tailscale { online: bool },
}

#[derive(Clone, Copy)]
pub(super) enum DiscoveryImportState {
    Configured,
    Ready,
    Importing,
    Disabled,
}

#[derive(Clone, Copy)]
pub(super) struct DiscoveryRow<'a> {
    pub(super) name: &'a str,
    pub(super) metadata: &'a str,
    pub(super) tags: Option<&'a str>,
    pub(super) detail: &'a str,
    pub(super) status: DiscoveryRowStatus,
    pub(super) stack_metadata: bool,
    pub(super) height: f32,
    pub(super) import_state: DiscoveryImportState,
}

impl DiscoveryRow<'_> {
    pub(super) fn show(self, ui: &mut egui::Ui, on_click: impl FnOnce()) {
        let theme = ui.tokens();
        let has_tag_line = self.stack_metadata && self.tags.is_some();
        let row_height = self.height
            + if has_tag_line {
                DISCOVERY_TAGS_EXTRA_HEIGHT
            } else {
                0.0
            };
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), row_height),
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
                row_height,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    if let Some(color) = self.dot_color(&theme) {
                        ui.label(egui::RichText::new("●").color(color));
                        ui.add_space(space::S8);
                    }
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), row_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add_space((self.height - 40.0) / 2.0);
                            show_truncated_label(
                                ui,
                                self.name,
                                type_scale::BODY_STRONG.font_id(ui.ctx()),
                                theme.text.primary,
                            );
                            show_truncated_label(
                                ui,
                                self.metadata,
                                type_scale::META.font_id(ui.ctx()),
                                theme.text.secondary,
                            );
                            if let Some(tags) = self.tags {
                                show_truncated_label(
                                    ui,
                                    tags,
                                    type_scale::META.font_id(ui.ctx()),
                                    theme.text.secondary,
                                );
                            }
                        },
                    );
                },
            );
        } else {
            discovery_fixed_column(
                &mut row,
                DISCOVERY_NAME_COLUMN_WIDTH,
                row_height,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    if let Some(color) = self.dot_color(&theme) {
                        ui.label(egui::RichText::new("●").color(color));
                        ui.add_space(space::S8);
                    }
                    show_truncated_label(
                        ui,
                        self.name,
                        type_scale::BODY_STRONG.font_id(ui.ctx()),
                        theme.text.primary,
                    );
                },
            );
            discovery_fixed_column(
                &mut row,
                DISCOVERY_METADATA_COLUMN_WIDTH,
                row_height,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    show_truncated_label(
                        ui,
                        self.metadata,
                        type_scale::META.font_id(ui.ctx()),
                        theme.text.secondary,
                    );
                },
            );
        }
        discovery_fixed_column(
            &mut row,
            DISCOVERY_LOCATION_COLUMN_WIDTH,
            row_height,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                show_truncated_label(
                    ui,
                    self.detail,
                    type_scale::META.font_id(ui.ctx()),
                    self.detail_color(&theme),
                );
            },
        );
        discovery_fixed_column(
            &mut row,
            DISCOVERY_ACTION_COLUMN_WIDTH,
            row_height,
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| show_import_button(ui, self.import_state, on_click),
        );
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), stroke::divider(&theme));
    }

    fn dot_color(self, theme: &Theme) -> Option<egui::Color32> {
        match self.status {
            DiscoveryRowStatus::Aks | DiscoveryRowStatus::Eks => Some(theme.success.fill),
            DiscoveryRowStatus::Tailscale { online: true } => Some(theme.success.fill),
            DiscoveryRowStatus::Tailscale { online: false } => Some(theme.status.neutral),
        }
    }

    fn detail_color(self, theme: &Theme) -> egui::Color32 {
        match self.status {
            DiscoveryRowStatus::Aks | DiscoveryRowStatus::Eks => theme.text.secondary,
            DiscoveryRowStatus::Tailscale { online: true } => theme.success.text,
            DiscoveryRowStatus::Tailscale { online: false } => theme.text.muted,
        }
    }
}

pub(super) fn discovery_import_state(
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

pub(super) fn show_truncated_label(
    ui: &mut egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
) {
    ui.add(egui::Label::new(egui::RichText::new(text).font(font).color(color)).truncate());
}

pub(super) fn show_discovery_loading(ui: &mut egui::Ui, message: &str) {
    let theme = ui.tokens();
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new());
        ui.label(
            egui::RichText::new(message)
                .font(type_scale::BODY.font_id(ui.ctx()))
                .color(theme.text.secondary),
        );
    });
}

pub(super) fn show_unavailable_tool(ui: &mut egui::Ui, tool: &str, message: &str) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(format!("{tool} is not installed"))
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.primary),
    );
    ui.label(
        egui::RichText::new(message)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.secondary),
    );
}

pub(super) fn show_empty_discovery(ui: &mut egui::Ui, message: &str) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(message)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.text.secondary),
    );
}

pub(super) fn show_discovery_error(ui: &mut egui::Ui, error: &str) {
    let theme = ui.tokens();
    ui.label(
        egui::RichText::new(error)
            .font(type_scale::BODY.font_id(ui.ctx()))
            .color(theme.danger.text),
    );
}

pub(super) fn show_discovery_warning(ui: &mut egui::Ui, warning: &str) {
    let theme = ui.tokens();
    egui::Frame::new()
        .fill(theme.warning.tint)
        .stroke(stroke::hairline(theme.warning.outline))
        .corner_radius(egui::CornerRadius::same(radius::KEY))
        .inner_margin(egui::Margin::symmetric(space::S12 as i8, space::S12 as i8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(space::S24);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("⚠").color(theme.warning.text));
                ui.label(
                    egui::RichText::new(warning)
                        .font(type_scale::META.font_id(ui.ctx()))
                        .color(theme.warning.text_on_tint),
                );
            });
        });
}

pub(super) fn show_import_button(
    ui: &mut egui::Ui,
    state: DiscoveryImportState,
    on_click: impl FnOnce(),
) {
    if matches!(state, DiscoveryImportState::Configured) {
        ui.add_enabled_ui(false, |ui| {
            TailwindButton::secondary("Already in kubeconfig")
                // Discovery rows reserve a narrow fixed action column. Keep
                // the long configured-state label within that column now that
                // the application-wide default button is more spacious.
                .size(ButtonSize::Xs)
                .show(ui)
        });
    } else if ui
        .add_enabled_ui(matches!(state, DiscoveryImportState::Ready), |ui| {
            TailwindButton::primary(if matches!(state, DiscoveryImportState::Importing) {
                "Adding…"
            } else {
                "Add to kubeconfig"
            })
            .size(ButtonSize::Xs)
            .show(ui)
            .clicked()
        })
        .inner
    {
        on_click();
    }
}

#[derive(Debug)]
pub(super) struct ManagedDiscoveryEffect(pub(super) ManagedDiscoveryAction);

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
                discovery.aws.account_loading = true;
                discovery.aws.clear_scan();
                navigation
                    .commands_to_send()
                    .push(Box::new(LoadManagedClusterDiscovery));
                navigation
                    .commands_to_send()
                    .push(Box::new(LoadAwsAccounts {
                        request_id: discovery.aws.request_id,
                    }));
            }
            ManagedDiscoveryAction::ScanEks {
                account_id,
                profile,
            } => {
                discovery
                    .aws
                    .begin_scan(account_id.clone(), profile.clone());
                navigation
                    .commands_to_send()
                    .push(Box::new(ScanEksClusters {
                        request_id: discovery.aws.request_id,
                        account_id,
                        profile,
                    }));
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
            ManagedDiscoveryAction::AddEks {
                account_id,
                region,
                name,
                profile,
            } => {
                discovery.aws.import_error = None;
                discovery.importing = Some(ManagedClusterImport::Eks {
                    account_id: account_id.clone(),
                    region: region.clone(),
                    name: name.clone(),
                    profile: profile.clone(),
                });
                navigation.commands_to_send().push(Box::new(AddEksCluster {
                    request_id: discovery.aws.request_id,
                    account_id,
                    region,
                    name,
                    profile,
                }));
            }
        }
    }
}
