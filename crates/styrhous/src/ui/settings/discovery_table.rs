use super::discovery::ManagedDiscoveryAction;
use super::*;

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

pub(super) fn show_discovery_headers(
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

pub(super) fn discovery_column_label(ui: &mut egui::Ui, width: f32, label: &str) {
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
    pub(super) detail: &'a str,
    pub(super) status: DiscoveryRowStatus,
    pub(super) stack_metadata: bool,
    pub(super) height: f32,
    pub(super) import_state: DiscoveryImportState,
}

impl DiscoveryRow<'_> {
    pub(super) fn show(self, ui: &mut egui::Ui, on_click: impl FnOnce()) {
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
    ui.add(egui::Label::new(egui::RichText::new(text).font(font).color(color)).truncate())
        .on_hover_text(text);
}

pub(super) fn show_discovery_loading(ui: &mut egui::Ui, message: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new());
        ui.label(
            egui::RichText::new(message)
                .font(typography::body())
                .color(gray::_600),
        );
    });
}

pub(super) fn show_unavailable_tool(ui: &mut egui::Ui, tool: &str, message: &str) {
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

pub(super) fn show_empty_discovery(ui: &mut egui::Ui, message: &str) {
    ui.label(
        egui::RichText::new(message)
            .font(typography::body())
            .color(gray::_600),
    );
}

pub(super) fn show_discovery_error(ui: &mut egui::Ui, error: &str) {
    ui.label(
        egui::RichText::new(error)
            .font(typography::body())
            .color(status::CRITICAL),
    );
}

pub(super) fn show_discovery_warning(ui: &mut egui::Ui, warning: &str) {
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

pub(super) fn show_import_button(
    ui: &mut egui::Ui,
    state: DiscoveryImportState,
    on_click: impl FnOnce(),
) {
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
