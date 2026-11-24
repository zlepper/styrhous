//! The floating workspace sidebar (C09, S09): the cluster card, the kind
//! filter, the kind list and the Notifications/Settings footer, in one card
//! [`size::SIDEBAR_INSET`] off the window's edges.

mod kinds;

use super::global_blade::SidebarDestination;
use super::state::{ClusterConnectionState, ClusterState, UiState};
use crate::api_resource::ApiResource;
use crate::licensing::LicenseStatus;
use crate::updater::UpdateStatus;
use crate::worker::WorkerCommandBox;
use components::design::size;
use components::design::theme::{ThemeExt, Tone};
use components::icons::Icon;
use components::tooltip::TooltipExt;
use components::{
    ClusterCard, FloatingSidebar, MenuItem, MoreButton, SideItem, StatusLabel,
    kind_filter_with_key, side_caption, side_footer,
};
use kinds::{KindList, KindListItem, KindRow};

/// What the sidebar asks the app to do once it is drawn.
#[derive(Default)]
struct SidebarEffects {
    open_kind: Option<ApiResource>,
    open_switcher: bool,
    toggle_node: Option<(String, bool)>,
    pin: Option<(ApiResource, bool)>,
    open_notifications: bool,
    open_settings: bool,
}

/// Draw the sidebar in a [`size::WORKSPACE_LEFT`] band on the window's left
/// and apply what was clicked.
pub(super) fn show(
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommandBox>,
    update_status: &UpdateStatus,
    license_status: &LicenseStatus,
) {
    let theme = ui.tokens();
    let mut effects = SidebarEffects::default();
    // The filter is edited while the rest of the state is only read.
    let mut filter = std::mem::take(&mut ui_state.kind_filter);
    let mut footer_clicks = (false, false);
    egui::Panel::left("sidebar")
        .exact_size(size::WORKSPACE_LEFT)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(theme.surface.canvas))
        .show(ui, |ui| {
            let band = ui.max_rect();
            let rect = egui::Rect::from_min_max(
                band.min + egui::Vec2::splat(size::SIDEBAR_INSET),
                egui::pos2(
                    band.left() + size::SIDEBAR_INSET + size::SIDEBAR,
                    band.bottom() - size::SIDEBAR_INSET,
                ),
            );
            FloatingSidebar::new(rect).show(
                ui,
                |ui| body(ui, ui_state, &mut filter, &mut effects),
                |ui| footer_clicks = footer(ui, ui_state, update_status, license_status),
            );
        });
    ui_state.kind_filter = filter;
    (effects.open_notifications, effects.open_settings) = footer_clicks;
    apply(ui_state, effects, commands_to_send);
}

fn apply(
    ui_state: &mut UiState,
    effects: SidebarEffects,
    commands_to_send: &mut Vec<WorkerCommandBox>,
) {
    if effects.open_switcher {
        ui_state.open_cluster_switcher();
    }
    if let Some((node_id, expanded)) = effects.toggle_node {
        ui_state.set_resource_navigation_node_expanded(node_id, expanded);
    }
    if let Some((api_resource, pinned)) = effects.pin {
        ui_state.pinned_kinds.set_pinned(&api_resource, pinned);
    }
    if let (Some(cluster_key), Some(api_resource)) = (ui_state.selected_cluster, effects.open_kind)
    {
        ui_state.select_api_resource(cluster_key, api_resource, commands_to_send);
    }
    if effects.open_notifications {
        ui_state.open_notification_history(commands_to_send);
    }
    if effects.open_settings {
        ui_state.open_settings_home(commands_to_send);
    }
}

/// The kubeconfig contexts in the order the card counts them ("2 of 4").
pub(super) fn contexts(ui_state: &UiState) -> Vec<&ClusterState> {
    let mut clusters = ui_state.clusters.values().collect::<Vec<_>>();
    clusters.sort_by_cached_key(|cluster| (cluster.name.to_lowercase(), cluster.cluster_key));
    clusters
}

/// A context's connection as its avatar lamp and a word for it.
pub(super) fn connection(cluster: &ClusterState) -> (Tone, &'static str) {
    match &cluster.connection {
        ClusterConnectionState::Connected => (Tone::Success, "Connected"),
        ClusterConnectionState::Connecting => (Tone::Warning, "Connecting"),
        ClusterConnectionState::Failed(_) => (Tone::Danger, "Unreachable"),
        ClusterConnectionState::Disconnected => (Tone::Neutral, "Disconnected"),
    }
}

/// The card's meta line: the connection while it is neither connected nor
/// disconnected, and where the context sits among the others. Upper case, as
/// `.ccard .mt` sets it.
fn card_meta(cluster: &ClusterState, position: usize, contexts: usize) -> String {
    let state = match &cluster.connection {
        ClusterConnectionState::Connected | ClusterConnectionState::Disconnected => None,
        ClusterConnectionState::Connecting | ClusterConnectionState::Failed(_) => {
            Some(connection(cluster).1.to_owned())
        }
    };
    let place = format!("{position} of {contexts}");
    match state {
        Some(state) => format!("{state} · {place}"),
        None => place,
    }
    .to_uppercase()
}

fn body(ui: &mut egui::Ui, ui_state: &UiState, filter: &mut String, effects: &mut SidebarEffects) {
    let contexts = contexts(ui_state);
    let selected = ui_state.selected_cluster.and_then(|key| {
        contexts
            .iter()
            .position(|cluster| cluster.cluster_key == key)
    });
    let card = match selected {
        Some(index) => {
            let cluster = contexts[index];
            let (status, _) = connection(cluster);
            let name = cluster.presentation.display_name(&cluster.name);
            let initials = cluster.presentation.badge(&cluster.name);
            // A colour picked in the context's settings wins over its place.
            let avatar = cluster.presentation.colour.map_or_else(
                || crate::context_presentation::automatic_colour(index),
                usize::from,
            );
            let meta = card_meta(cluster, index + 1, contexts.len());
            let card = ClusterCard::new(name, &meta, &initials, avatar, status).show(ui);
            match &cluster.connection {
                ClusterConnectionState::Failed(error) => card.tooltip(error),
                _ => card,
            }
        }
        None => {
            let meta = crate::ui::widgets::count_noun(contexts.len(), "context");
            ClusterCard::empty("Choose a cluster", &meta, "⌘K").show(ui)
        }
    };
    effects.open_switcher |= card.clicked();

    let Some(cluster) = selected.map(|index| contexts[index]) else {
        side_caption(ui, "Resources", None);
        ui.add_space(components::design::space::S4);
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(
                components::design::space::S8 as i8,
                0,
            ))
            .show(ui, |ui| {
                components::design::type_scale::META.label(
                    ui,
                    "Pick a cluster to see its resource kinds.",
                    ui.tokens().nav.text_muted,
                );
            });
        return;
    };
    let list = KindList::new(
        cluster,
        &ui_state.pinned_kinds,
        &ui_state.resource_navigation_expansion,
        filter,
    );
    // Until discovery has found the kinds there is no count to give.
    let placeholder = match list.kind_count {
        0 => "Filter kinds".to_owned(),
        count => format!("Filter {count} kinds"),
    };
    let field = kind_filter_with_key(ui, filter, &placeholder, !ui_state.surface_in_front());
    if field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        // Enter on an empty filter has nothing to open, and one that matches
        // nothing keeps what was typed.
        if !filter.trim().is_empty()
            && let Some(kind) = list.first_kind()
        {
            effects.open_kind = Some(kind.clone());
            filter.clear();
        }
    }
    let selected_kind = cluster.selected_api_resource.as_ref();
    let discovering = !cluster.api_discovery.complete
        && !matches!(cluster.connection, ClusterConnectionState::Failed(_));
    components::scroll::vertical()
        .id_salt("sidebar-kinds")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            for item in &list.items {
                kind_list_item(ui, item, selected_kind, effects);
            }
            if discovering {
                ui.add_space(components::design::space::S8);
                StatusLabel::new("Discovering API groups…", Tone::Neutral).show(ui);
            }
        });
}

fn kind_list_item(
    ui: &mut egui::Ui,
    item: &KindListItem,
    selected_kind: Option<&ApiResource>,
    effects: &mut SidebarEffects,
) {
    match item {
        KindListItem::Caption { label, detail } => side_caption(ui, label, detail.as_deref()),
        KindListItem::Kind(row) => {
            let colour = ui.tokens().category.nth(row.category);
            kind_row(
                ui,
                SideItem::kind(&row.label, colour),
                row,
                selected_kind,
                effects,
            );
        }
        KindListItem::Child(row) => {
            kind_row(ui, SideItem::child(&row.label), row, selected_kind, effects);
        }
        KindListItem::Group {
            node_id,
            label,
            kinds,
            expanded,
        } => {
            if SideItem::group(label, *expanded)
                .count(*kinds)
                .show(ui)
                .clicked()
            {
                effects.toggle_node = Some((node_id.clone(), !expanded));
            }
        }
        KindListItem::ApiGroup {
            node_id,
            group,
            kinds,
            expanded,
        } => {
            if SideItem::api_group(group, *expanded)
                .count(*kinds)
                .show(ui)
                .clicked()
            {
                effects.toggle_node = Some((node_id.clone(), !expanded));
            }
        }
    }
}

/// A kind's item: its count, selection
/// and a context menu that pins or unpins it.
fn kind_row(
    ui: &mut egui::Ui,
    item: SideItem<'_>,
    row: &KindRow,
    selected_kind: Option<&ApiResource>,
    effects: &mut SidebarEffects,
) {
    let mut item = item.selected(selected_kind == Some(&row.api_resource));
    if let Some(count) = row.count {
        item = item.count(count);
    }
    let response = item.show(ui);
    if response.clicked() {
        effects.open_kind = Some(row.api_resource.clone());
    }
    MoreButton::show_context_menu(&response, |menu| {
        let label = if row.pinned {
            "Unpin from sidebar"
        } else {
            "Pin to sidebar"
        };
        if menu.item(MenuItem::new(label)).clicked() {
            effects.pin = Some((row.api_resource.clone(), !row.pinned));
        }
    });
}

fn footer(
    ui: &mut egui::Ui,
    ui_state: &UiState,
    update_status: &UpdateStatus,
    license_status: &LicenseStatus,
) -> (bool, bool) {
    let blades = &ui_state.global_blades;
    let unread = ui_state.unread_operation_count();
    let mut notifications = SideItem::action(Icon::Bell, "Notifications")
        .selected(blades.shows_destination(SidebarDestination::Notifications));
    if unread > 0 {
        notifications = notifications.alert_count(unread);
    }
    let mut settings = SideItem::action(Icon::Cog6Tooth, "Settings")
        .selected(blades.shows_destination(SidebarDestination::Settings));
    if update_status.shows_badge() || license_status.shows_warning() {
        settings = settings.lamp(Tone::Warning);
    }
    let (notifications, settings) = side_footer(ui, notifications, settings);
    let open_settings = settings
        .tooltip(format!(
            "{}\n{}",
            update_status.summary(),
            license_status.summary()
        ))
        .clicked();
    (notifications.clicked(), open_settings)
}
