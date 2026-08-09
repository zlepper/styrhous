use super::resource_actions::show_resource_action_items;
use super::state::{ResourceAction, ResourceDetailHistoryEntry, ResourceDetailPanelState, UiState};
use super::widgets::show_resource_cell;
use crate::minimal_resource::{MinimalResource, format_age};
use crate::resource_detail::{
    ConfigMapDetail, ManagedResource, NodeDetail, PodDetail, ResourceDetail, ResourceDetailPayload,
    ResourceEvent, SecretDetail,
};
use crate::resource_handlers::table_definition;
use crate::resource_table::{CONTAINERS_COLUMN, NODE_COLUMN, ResourceTableDefinition};
use crate::terminal_launcher::PodShellRequest;
use crate::worker::{ResourceDataUpdate, WorkerCommand};
use components::colors::{WHITE, gray, indigo};
use components::design::{radius, spacing, status, typography};
use components::icons;
use components::{
    BladeTransition as ResourceDetailTransition, ButtonSize, ButtonVariant, MoreButton,
    PointingHand, TableRowBuilder, TailwindButton, TailwindTable, TailwindTextArea, WorkspaceCard,
};
use std::collections::BTreeMap;

const PANEL_WIDTH: f32 = 744.0;
const PANEL_PADDING: i8 = spacing::XL as i8;
const CARD_CONTENT_PADDING: i8 = spacing::MD as i8;
const CARD_HEADER_HEIGHT: f32 = 40.0;
const CARD_HEADER_PADDING: f32 = spacing::LG;
const CARD_GAP: f32 = spacing::MD;
const ACTIVE_BLADE_INSET: f32 = 8.0;
const HISTORY_BLADE_SCALES: [f32; 2] = [0.9, 0.8];
/// Horizontal recession as a fraction of the unscaled blade width. The older
/// blade's additional step shrinks with the blade, so its recession remains
/// visually proportional to its scale.
const HISTORY_BLADE_X_TRANSLATIONS: [f32; 2] = [
    1.0 / 3.0,
    (1.0 / 3.0) * (1.0 + HISTORY_BLADE_SCALES[1] / HISTORY_BLADE_SCALES[0]),
];
const BLADE_TRANSITION_DURATION: f32 = 0.25;

#[derive(Clone, Copy)]
struct BladeTransform {
    position: egui::Pos2,
    scale: f32,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct BladeNavigation {
    can_go_back: bool,
    can_go_forward: bool,
}

#[derive(Default)]
struct BladeResult {
    action: Option<ResourceAction>,
    close: bool,
}

pub(super) fn show(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
    shell_requests: &mut Vec<PodShellRequest>,
) {
    show_shared_blade(ctx, ui_state, commands_to_send, shell_requests);
}

#[allow(dead_code, unreachable_code)]
fn show_legacy(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
    shell_requests: &mut Vec<PodShellRequest>,
) {
    show_shared_blade(ctx, ui_state, commands_to_send, shell_requests);
    return;

    let Some(cluster_key) = ui_state.selected_cluster else {
        return;
    };
    if ui_state
        .clusters
        .get(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_ref())
        .is_none()
    {
        return;
    }

    let viewport = ctx.content_rect();
    let closing_progress = ui_state
        .clusters
        .get(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_ref())
        .and_then(|panel| {
            matches!(
                panel.navigator.transition(),
                Some(ResourceDetailTransition::Closing)
            )
            .then(|| animated_blade_transition_progress(ctx, panel))
        })
        .unwrap_or(0.0);
    let mut close = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let dismiss_on_outside_click = ui_state
        .clusters
        .get_mut(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
        .map(|panel| {
            let value = panel.dismiss_on_outside_click;
            panel.dismiss_on_outside_click = true;
            value
        })
        .unwrap_or(false);
    egui::Area::new(egui::Id::new("resource-detail-scrim"))
        .order(egui::Order::Foreground)
        .fixed_pos(viewport.min)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_min_size(viewport.size());
            ui.painter().rect_filled(
                ui.max_rect(),
                0.0,
                egui::Color32::BLACK.gamma_multiply(0.58 * (1.0 - closing_progress)),
            );
        });

    let (history_layers, history_offset, transition, active_blade_id) = ui_state
        .clusters
        .get(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_ref())
        .map(|panel| {
            let transition = panel.navigator.transition().map(|transition| {
                let progress = animated_blade_transition_progress(ctx, panel);
                (transition, progress)
            });
            let first_visible_history = panel.navigator.back_stack().len().saturating_sub(2);
            (
                panel.navigator.back_stack()[first_visible_history..]
                    .iter()
                    .collect::<Vec<_>>(),
                first_visible_history,
                transition,
                blade_id(panel.history_entry_id),
            )
        })
        .unwrap_or_else(|| {
            (
                Vec::new(),
                0,
                None,
                egui::Id::new("missing-resource-detail-blade"),
            )
        });
    let panel_layer_id = blade_layer_id(active_blade_id);
    let blade_height = viewport.height() - ACTIVE_BLADE_INSET * 2.0;
    let active_transform = transition.map_or_else(
        || active_blade_transform(viewport),
        |(transition, progress)| match transition {
            ResourceDetailTransition::Opening => BladeTransform {
                position: active_blade_transform(viewport).position
                    + egui::vec2((1.0 - progress) * PANEL_WIDTH, 0.0),
                scale: 1.0,
            },
            ResourceDetailTransition::Forward => BladeTransform {
                position: active_blade_transform(viewport).position
                    + egui::vec2((1.0 - progress) * PANEL_WIDTH, 0.0),
                scale: 1.0,
            },
            ResourceDetailTransition::Back => interpolate_blade_transform(
                history_blade_transform(viewport, 0),
                active_blade_transform(viewport),
                progress,
            ),
            ResourceDetailTransition::Closing => {
                closing_blade_transform(viewport, active_blade_transform(viewport), progress)
            }
        },
    );
    let has_history_layers = !history_layers.is_empty();
    show_history_layers(ctx, viewport, &history_layers, history_offset, transition);

    let active_rect = egui::Rect::from_min_size(
        active_transform.position,
        egui::vec2(PANEL_WIDTH, blade_height) * active_transform.scale,
    );
    let panel = ui_state
        .clusters
        .get_mut(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
        .expect("resource detail panel was checked above");
    let is_closing = matches!(transition, Some((ResourceDetailTransition::Closing, _)));
    let outgoing_foreground_blade_is_visible = matches!(
        transition,
        Some((ResourceDetailTransition::Back, progress)) if progress < 1.0
    );

    let panel_blade_id = blade_id(panel.history_entry_id);
    if let Some((transition_kind, progress)) = transition
        && progress < 1.0
    {
        match transition_kind {
            ResourceDetailTransition::Back => {
                if let Some(outgoing_entry) = panel.navigator.forward_stack().last() {
                    show_outgoing_blade(
                        ctx,
                        viewport,
                        outgoing_entry,
                        BladeNavigation {
                            // The back action was available only if the
                            // outgoing blade had an older predecessor.
                            can_go_back: true,
                            // `forward_stack` now also contains the outgoing
                            // blade, so entries beneath it were already its
                            // forward history.
                            can_go_forward: panel.navigator.forward_stack().len() > 1,
                        },
                        BladeTransform {
                            position: active_blade_transform(viewport).position
                                + egui::vec2(
                                    progress * (PANEL_WIDTH + ACTIVE_BLADE_INSET * 2.0),
                                    0.0,
                                ),
                            scale: 1.0,
                        },
                    );
                }
            }
            ResourceDetailTransition::Opening
            | ResourceDetailTransition::Forward
            | ResourceDetailTransition::Closing => {}
        }
    }
    let active_origin = active_blade_transform(viewport).position;
    let active_visual_transform = blade_visual_transform(active_origin, active_transform);

    let mut blade_result = BladeResult::default();
    egui::Area::new(panel_blade_id)
        .order(egui::Order::Foreground)
        .fixed_pos(active_origin)
        .fade_in(false)
        .interactable(
            !is_closing
                && !outgoing_foreground_blade_is_visible
                && active_visual_transform == egui::emath::TSTransform::IDENTITY,
        )
        .show(ctx, |ui| {
            ui.set_width(PANEL_WIDTH);
            ui.set_height(blade_height);
            ui.with_visual_transform(active_visual_transform, |ui| {
                show_blade_frame(ui, blade_height, panel.history_entry_id, |ui| {
                    let _navigation = BladeNavigation {
                        can_go_back: panel.navigator.can_go_back(),
                        can_go_forward: panel.navigator.can_go_forward(),
                    };
                    let entry = panel.navigator.current_mut();
                    let result = show_resource_detail_blade(
                        ui,
                        &entry.api_resource,
                        &entry.namespace,
                        &entry.resource_name,
                        &entry.resource_uid,
                        &entry.detail,
                        &entry.events,
                        entry.detail_error.as_deref(),
                        entry.events_error.as_deref(),
                        &entry.managed_resources,
                        entry.managed_resources_error.as_deref(),
                        entry.data_editor.as_mut(),
                    );
                    if !outgoing_foreground_blade_is_visible {
                        blade_result = result;
                    }
                });
            });
        });
    if should_promote_active_blade(has_history_layers, transition) {
        ctx.move_to_top(panel_layer_id);
    }

    // Register the input-only scrim after every blade layer. Its regions
    // deliberately exclude the active blade, so it captures history and
    // workspace clicks without blocking foreground controls or menus.
    close |= dismiss_on_outside_click && show_input_scrim(ctx, viewport, active_rect);

    close |= blade_result.close;
    if let Some(action) = blade_result.action {
        let mut log_to_open = None;
        let mut shell_to_open = None;
        let navigation_action = matches!(
            &action,
            ResourceAction::NavigateDetails { .. }
                | ResourceAction::NavigateBack
                | ResourceAction::NavigateForward
        );
        match action {
            ResourceAction::NavigateDetails {
                api_resource,
                name,
                namespace,
                uid,
            } => ui_state.navigate_resource_detail(
                cluster_key,
                api_resource,
                name,
                namespace,
                uid,
                commands_to_send,
            ),
            ResourceAction::NavigateBack => {
                ui_state.navigate_resource_detail_history(cluster_key, false, commands_to_send)
            }
            ResourceAction::NavigateForward => {
                ui_state.navigate_resource_detail_history(cluster_key, true, commands_to_send)
            }
            action => {
                if let Some(cluster) = ui_state.clusters.get_mut(&cluster_key) {
                    match action {
                        ResourceAction::EditYaml { name, namespace } => {
                            commands_to_send.push(WorkerCommand::GetResourceYaml {
                                cluster_key: cluster.cluster_key,
                                api_resource: panel_api_resource(cluster),
                                namespace,
                                resource_name: name,
                            });
                        }
                        ResourceAction::RequestDelete { name, namespace } => {
                            cluster.pending_delete = Some(super::state::PendingDelete {
                                resource_name: name,
                                namespace,
                            });
                        }
                        ResourceAction::SaveData {
                            expected_values,
                            updated_values,
                        } => {
                            let panel = cluster
                                .resource_detail_panel
                                .as_ref()
                                .expect("detail panel remains open while data is saved");
                            if let Some(namespace) = panel.namespace.clone() {
                                commands_to_send.push(WorkerCommand::UpdateResourceData {
                                    cluster_key: cluster.cluster_key,
                                    api_resource: panel.api_resource.clone(),
                                    namespace,
                                    resource_name: panel.resource_name.clone(),
                                    update: ResourceDataUpdate {
                                        expected_resource_version: panel
                                            .data_editor
                                            .as_ref()
                                            .expect(
                                                "data save action requires an initialized editor",
                                            )
                                            .resource_version
                                            .clone(),
                                        expected_values,
                                        updated_values,
                                    },
                                });
                            }
                        }
                        ResourceAction::ViewLogs {
                            name,
                            namespace,
                            container,
                        } => {
                            log_to_open = Some((cluster.cluster_key, name, namespace, container));
                        }
                        ResourceAction::Shell {
                            name,
                            namespace,
                            container,
                        } => {
                            shell_to_open = namespace.map(|namespace| PodShellRequest {
                                kube_context: cluster.name.clone(),
                                namespace,
                                pod_name: name,
                                container: container.name,
                            });
                        }
                        ResourceAction::OpenDetails { .. } => {
                            unreachable!("inspector actions cannot open detail")
                        }
                        ResourceAction::NavigateDetails { .. }
                        | ResourceAction::NavigateBack
                        | ResourceAction::NavigateForward => {
                            unreachable!("navigation actions were handled above")
                        }
                    }
                }
            }
        }
        if navigation_action {
            seed_detail_transition(ctx, ui_state, cluster_key);
        }
        if let Some((cluster_key, name, namespace, container)) = log_to_open {
            ui_state.open_pod_log_window(cluster_key, name, namespace, container, commands_to_send);
        }
        shell_requests.extend(shell_to_open);
    }
    if let Some(panel) = ui_state
        .clusters
        .get_mut(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
    {
        show_data_conflict_dialog(ctx, panel.data_editor.as_mut());
    }
    let closing_finished = matches!(
        transition,
        Some((ResourceDetailTransition::Closing, progress)) if progress >= 1.0
    );
    if close && ui_state.begin_close_resource_detail(cluster_key) {
        seed_detail_transition(ctx, ui_state, cluster_key);
    } else if closing_finished {
        ui_state.close_resource_detail(cluster_key, commands_to_send);
    }
}

fn show_shared_blade(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    commands_to_send: &mut Vec<WorkerCommand>,
    shell_requests: &mut Vec<PodShellRequest>,
) {
    let Some(cluster_key) = ui_state.selected_cluster else {
        return;
    };
    let Some(cluster) = ui_state.clusters.get_mut(&cluster_key) else {
        return;
    };
    let Some(panel) = cluster.resource_detail_panel.as_mut() else {
        return;
    };

    let dismiss_on_outside_click = panel.dismiss_on_outside_click;
    panel.dismiss_on_outside_click = true;
    let stack = components::BladeStack::new("resource-detail-blade");
    let response = stack.show(
        ctx,
        &mut panel.navigator,
        |entry| egui::Id::new(entry.history_entry_id),
        |ui, entry, layer| show_resource_detail_header(ui, entry, layer.is_foreground),
        |ui, entry, layer| {
            show_resource_detail_blade(
                ui,
                &entry.api_resource,
                &entry.namespace,
                &entry.resource_name,
                &entry.resource_uid,
                &entry.detail,
                &entry.events,
                entry.detail_error.as_deref(),
                entry.events_error.as_deref(),
                &entry.managed_resources,
                entry.managed_resources_error.as_deref(),
                if layer.is_foreground {
                    entry.data_editor.as_mut()
                } else {
                    None
                },
            )
        },
    );

    let mut close = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    close |= dismiss_on_outside_click && response.dismissed;
    close |= response.header.close || response.active.close;
    let action = response.header.action.or(response.active.action);
    if let Some(action) = action {
        match action {
            ResourceAction::NavigateDetails {
                api_resource,
                name,
                namespace,
                uid,
            } => {
                ui_state.navigate_resource_detail(
                    cluster_key,
                    api_resource,
                    name,
                    namespace,
                    uid,
                    commands_to_send,
                );
            }
            ResourceAction::NavigateBack => {
                ui_state.navigate_resource_detail_history(cluster_key, false, commands_to_send);
            }
            ResourceAction::NavigateForward => {
                ui_state.navigate_resource_detail_history(cluster_key, true, commands_to_send);
            }
            ResourceAction::EditYaml { name, namespace } => {
                if let Some(cluster) = ui_state.clusters.get_mut(&cluster_key) {
                    commands_to_send.push(WorkerCommand::GetResourceYaml {
                        cluster_key: cluster.cluster_key,
                        api_resource: panel_api_resource(cluster),
                        namespace,
                        resource_name: name,
                    });
                }
            }
            ResourceAction::RequestDelete { name, namespace } => {
                if let Some(cluster) = ui_state.clusters.get_mut(&cluster_key) {
                    cluster.pending_delete = Some(super::state::PendingDelete {
                        resource_name: name,
                        namespace,
                    });
                }
            }
            ResourceAction::SaveData {
                expected_values,
                updated_values,
            } => {
                if let Some(cluster) = ui_state.clusters.get_mut(&cluster_key)
                    && let Some(panel) = cluster.resource_detail_panel.as_ref()
                    && let (Some(namespace), Some(editor)) =
                        (panel.namespace.clone(), panel.data_editor.as_ref())
                {
                    commands_to_send.push(WorkerCommand::UpdateResourceData {
                        cluster_key: cluster.cluster_key,
                        api_resource: panel.api_resource.clone(),
                        namespace,
                        resource_name: panel.resource_name.clone(),
                        update: ResourceDataUpdate {
                            expected_resource_version: editor.resource_version.clone(),
                            expected_values,
                            updated_values,
                        },
                    });
                }
            }
            ResourceAction::ViewLogs {
                name,
                namespace,
                container,
            } => ui_state.open_pod_log_window(
                cluster_key,
                name,
                namespace,
                container,
                commands_to_send,
            ),
            ResourceAction::Shell {
                name,
                namespace,
                container,
            } => {
                if let (Some(namespace), Some(cluster)) =
                    (namespace, ui_state.clusters.get(&cluster_key))
                {
                    shell_requests.push(PodShellRequest {
                        kube_context: cluster.name.clone(),
                        namespace,
                        pod_name: name,
                        container: container.name,
                    });
                }
            }
            ResourceAction::OpenDetails { .. } => {
                unreachable!("inspector actions cannot open detail")
            }
        }
    }
    if let Some(panel) = ui_state
        .clusters
        .get_mut(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_mut())
    {
        show_data_conflict_dialog(ctx, panel.data_editor.as_mut());
    }
    if close {
        ui_state.begin_close_resource_detail(cluster_key);
    } else if response.close_finished {
        ui_state.close_resource_detail(cluster_key, commands_to_send);
    }
}

pub(super) fn seed_detail_transition(ctx: &egui::Context, ui_state: &UiState, cluster_key: i32) {
    if let Some(panel) = ui_state
        .clusters
        .get(&cluster_key)
        .and_then(|cluster| cluster.resource_detail_panel.as_ref())
        .filter(|panel| panel.navigator.transition().is_some())
    {
        ctx.animate_value_with_time(
            detail_transition_id(panel),
            0.0,
            blade_transition_duration(ctx),
        );
    }
}

fn blade_transition_duration(ctx: &egui::Context) -> f32 {
    if ctx.style().animation_time == 0.0 {
        0.0
    } else {
        BLADE_TRANSITION_DURATION
    }
}

fn animated_blade_transition_progress(
    ctx: &egui::Context,
    panel: &ResourceDetailPanelState,
) -> f32 {
    let linear_progress = ctx.animate_value_with_time(
        detail_transition_id(panel),
        1.0,
        blade_transition_duration(ctx),
    );
    ease_blade_transition(linear_progress)
}

fn should_promote_active_blade(
    has_history_layers: bool,
    transition: Option<(ResourceDetailTransition, f32)>,
) -> bool {
    has_history_layers
        && !matches!(
            transition,
            Some((ResourceDetailTransition::Back, progress)) if progress < 1.0
        )
}

fn show_input_scrim(ctx: &egui::Context, viewport: egui::Rect, active_rect: egui::Rect) -> bool {
    let regions = if active_rect.intersects(viewport) {
        let active_rect = active_rect.intersect(viewport);
        [
            (
                "left",
                egui::Rect::from_min_max(
                    viewport.min,
                    egui::pos2(active_rect.min.x, viewport.max.y),
                ),
            ),
            (
                "top",
                egui::Rect::from_min_max(
                    egui::pos2(active_rect.min.x, viewport.min.y),
                    active_rect.min,
                ),
            ),
            (
                "bottom",
                egui::Rect::from_min_max(
                    egui::pos2(active_rect.min.x, active_rect.max.y),
                    egui::pos2(active_rect.max.x, viewport.max.y),
                ),
            ),
            (
                "right",
                egui::Rect::from_min_max(
                    egui::pos2(active_rect.max.x, viewport.min.y),
                    viewport.max,
                ),
            ),
        ]
        .into_iter()
        .collect::<Vec<_>>()
    } else {
        vec![("full", viewport)]
    };

    let mut clicked = false;
    for (name, region) in regions {
        if !region.is_positive() {
            continue;
        }
        let id = egui::Id::new(("resource-detail-input-scrim", name));
        let layer_id = egui::LayerId::new(egui::Order::Foreground, id);
        clicked |= egui::Area::new(id)
            .order(egui::Order::Foreground)
            .fixed_pos(region.min)
            .movable(false)
            .show(ctx, |ui| {
                ui.set_min_size(region.size());
                ui.interact(ui.max_rect(), ui.id().with("dismiss"), egui::Sense::click())
                    .clicked()
            })
            .inner;
        // Input-only regions must be above the history visuals. They do not
        // overlap the active blade, so its controls and menus remain usable.
        ctx.move_to_top(layer_id);
    }
    clicked
}

fn detail_transition_id(panel: &ResourceDetailPanelState) -> egui::Id {
    let transition_kind = match panel.navigator.transition() {
        Some(ResourceDetailTransition::Opening) => "opening",
        Some(ResourceDetailTransition::Forward) => "forward",
        Some(ResourceDetailTransition::Back) => "back",
        Some(ResourceDetailTransition::Closing) => "closing",
        None => "none",
    };
    egui::Id::new((
        "resource-detail-transition",
        panel.selection_generation,
        transition_kind,
    ))
}

fn show_history_layers(
    ctx: &egui::Context,
    viewport: egui::Rect,
    layers: &[&ResourceDetailHistoryEntry],
    history_offset: usize,
    transition: Option<(ResourceDetailTransition, f32)>,
) {
    // Paint oldest to newest so each newer blade naturally occludes the one
    // directly behind it.
    for index in 0..layers.len() {
        let entry = layers[index];
        let depth = layers.len() - index - 1;
        let target_transform = history_blade_transform(viewport, depth);
        let start_transform = match transition {
            Some((ResourceDetailTransition::Forward, _)) if depth == 0 => {
                active_blade_transform(viewport)
            }
            Some((ResourceDetailTransition::Forward, _)) => {
                history_blade_transform(viewport, depth - 1)
            }
            Some((ResourceDetailTransition::Back, _)) => {
                history_blade_transform(viewport, depth + 1)
            }
            Some((ResourceDetailTransition::Opening | ResourceDetailTransition::Closing, _)) => {
                target_transform
            }
            None => target_transform,
        };
        let transform = match transition {
            Some((ResourceDetailTransition::Closing, progress)) => {
                closing_blade_transform(viewport, target_transform, progress)
            }
            Some((_, progress)) => {
                interpolate_blade_transform(start_transform, target_transform, progress)
            }
            None => target_transform,
        };
        let untransformed_position = active_blade_transform(viewport).position;
        let visual_transform = blade_visual_transform(untransformed_position, transform);

        egui::Area::new(history_blade_id(entry))
            // Keep every blade in the same layer class as it moves between
            // active and history. Foreground keeps application tooltips above
            // the blade while avoiding a handoff between layer classes.
            .order(egui::Order::Foreground)
            .fixed_pos(untransformed_position)
            .fade_in(false)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_width(PANEL_WIDTH);
                let blade_height = viewport.height() - ACTIVE_BLADE_INSET * 2.0;
                ui.set_height(blade_height);
                ui.with_visual_transform(visual_transform, |ui| {
                    show_entry_blade_frame(
                        ui,
                        blade_height,
                        entry,
                        BladeNavigation {
                            can_go_back: history_offset + index > 0,
                            // The current foreground blade is always the
                            // immediate next entry from a history blade.
                            can_go_forward: true,
                        },
                        false,
                    );
                });
            });
        // History blades retain the layer order established while they were
        // foreground. Promoting several layers with `move_to_top` in one
        // frame is intentionally unordered in egui, so it cannot represent a
        // deterministic back-to-front stack.
    }
}

fn history_blade_id(entry: &ResourceDetailHistoryEntry) -> egui::Id {
    blade_id(entry.history_entry_id)
}

fn blade_layer_id(blade_id: egui::Id) -> egui::LayerId {
    egui::LayerId::new(egui::Order::Foreground, blade_id)
}

fn active_blade_transform(viewport: egui::Rect) -> BladeTransform {
    BladeTransform {
        position: egui::pos2(
            viewport.max.x - PANEL_WIDTH - ACTIVE_BLADE_INSET,
            viewport.min.y + ACTIVE_BLADE_INSET,
        ),
        scale: 1.0,
    }
}

fn blade_visual_transform(
    untransformed_position: egui::Pos2,
    transform: BladeTransform,
) -> egui::emath::TSTransform {
    egui::emath::TSTransform::new(
        transform.position.to_vec2() - untransformed_position.to_vec2() * transform.scale,
        transform.scale,
    )
}

fn history_blade_transform(viewport: egui::Rect, index: usize) -> BladeTransform {
    let depth = index.min(HISTORY_BLADE_SCALES.len() - 1);
    let scale = HISTORY_BLADE_SCALES[depth];
    let blade_height = viewport.height() - ACTIVE_BLADE_INSET * 2.0;
    let horizontal_offset = PANEL_WIDTH * HISTORY_BLADE_X_TRANSLATIONS[depth];
    BladeTransform {
        position: egui::pos2(
            active_blade_transform(viewport).position.x - horizontal_offset,
            viewport.min.y + ACTIVE_BLADE_INSET + (blade_height - blade_height * scale) / 2.0,
        ),
        scale,
    }
}

fn closing_blade_transform(
    viewport: egui::Rect,
    transform: BladeTransform,
    progress: f32,
) -> BladeTransform {
    BladeTransform {
        position: egui::pos2(
            egui::lerp(
                transform.position.x..=viewport.max.x + ACTIVE_BLADE_INSET,
                progress,
            ),
            transform.position.y,
        ),
        scale: transform.scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_transform_moves_active_and_history_blades_beyond_the_right_viewport_edge() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1536.0, 1024.0));
        let active = active_blade_transform(viewport);
        let history = history_blade_transform(viewport, 1);

        for blade in [active, history] {
            let closing = closing_blade_transform(viewport, blade, 1.0);
            assert!(closing.position.x > viewport.max.x);
            assert_eq!(closing.scale, blade.scale);
        }
    }

    #[test]
    fn historical_blade_recession_scales_its_second_step_with_the_blade_width() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1536.0, 1024.0));
        let active = active_blade_transform(viewport);
        let nearer = history_blade_transform(viewport, 0);
        let older = history_blade_transform(viewport, 1);

        let nearer_recession = active.position.x - nearer.position.x;
        let older_recession = active.position.x - older.position.x;
        let older_step = older_recession - nearer_recession;

        assert!((older_step - nearer_recession * older.scale / nearer.scale).abs() < 0.001);
    }

    #[test]
    fn active_blade_waits_for_the_outgoing_back_blade_to_leave_before_promotion() {
        assert!(!should_promote_active_blade(
            true,
            Some((ResourceDetailTransition::Back, 0.0)),
        ));
        assert!(!should_promote_active_blade(
            true,
            Some((ResourceDetailTransition::Back, 0.99)),
        ));
        assert!(should_promote_active_blade(
            true,
            Some((ResourceDetailTransition::Back, 1.0)),
        ));
    }

    #[test]
    fn blade_transition_easing_starts_and_ends_gently() {
        assert_eq!(ease_blade_transition(0.0), 0.0);
        assert!(ease_blade_transition(0.25) < 0.25);
        assert_eq!(ease_blade_transition(0.5), 0.5);
        assert!(ease_blade_transition(0.75) > 0.75);
        assert_eq!(ease_blade_transition(1.0), 1.0);
    }
}

fn ease_blade_transition(progress: f32) -> f32 {
    egui::emath::easing::cubic_in_out(progress)
}

fn interpolate_blade_transform(
    from: BladeTransform,
    to: BladeTransform,
    progress: f32,
) -> BladeTransform {
    BladeTransform {
        position: from.position + (to.position - from.position) * progress,
        scale: from.scale + (to.scale - from.scale) * progress,
    }
}

fn show_outgoing_blade(
    ctx: &egui::Context,
    viewport: egui::Rect,
    entry: &ResourceDetailHistoryEntry,
    navigation: BladeNavigation,
    transform: BladeTransform,
) {
    let origin = active_blade_transform(viewport).position;
    let visual_transform = blade_visual_transform(origin, transform);

    let blade_height = viewport.height() - ACTIVE_BLADE_INSET * 2.0;
    egui::Area::new(history_blade_id(entry))
        .order(egui::Order::Foreground)
        .fixed_pos(origin)
        .fade_in(false)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_width(PANEL_WIDTH);
            ui.set_height(blade_height);
            ui.with_visual_transform(visual_transform, |ui| {
                show_entry_blade_frame(ui, blade_height, entry, navigation, true);
            });
        });
}

fn blade_id(history_entry_id: u64) -> egui::Id {
    egui::Id::new(("resource-detail-blade", history_entry_id))
}

#[allow(dead_code)]
fn show_blade_navigation_controls(
    ui: &mut egui::Ui,
    navigation: BladeNavigation,
    is_foreground: bool,
) -> Option<ResourceAction> {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(80.0, 36.0), egui::Sense::hover());
    let mut controls_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );

    let mut action = None;
    let back_label = if is_foreground {
        "Back"
    } else {
        "Back in background blade"
    };
    let back_clicked = controls_ui
        .add_enabled_ui(navigation.can_go_back, |ui| {
            TailwindButton::icon(
                icons::arrow_left_icon()
                    .fit_to_exact_size(egui::Vec2::splat(16.0))
                    .tint(gray::_700),
            )
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Sm)
            .accessibility_label(back_label)
            .show(ui)
            .clicked()
        })
        .inner;
    if back_clicked {
        action = Some(ResourceAction::NavigateBack);
    }
    let forward_label = if is_foreground {
        "Forward"
    } else {
        "Forward in background blade"
    };
    let forward_clicked = controls_ui
        .add_enabled_ui(navigation.can_go_forward, |ui| {
            TailwindButton::icon(
                icons::arrow_right_icon()
                    .fit_to_exact_size(egui::Vec2::splat(16.0))
                    .tint(gray::_700),
            )
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Sm)
            .accessibility_label(forward_label)
            .show(ui)
            .clicked()
        })
        .inner;
    if forward_clicked {
        action = Some(ResourceAction::NavigateForward);
    }
    action
}

fn panel_api_resource(cluster: &super::state::ClusterState) -> crate::api_resource::ApiResource {
    cluster
        .resource_detail_panel
        .as_ref()
        .expect("detail panel remains open while an action is handled")
        .api_resource
        .clone()
}

fn show_entry_blade_frame(
    ui: &mut egui::Ui,
    blade_height: f32,
    entry: &ResourceDetailHistoryEntry,
    _navigation: BladeNavigation,
    _is_foreground: bool,
) {
    let mut data_editor = entry.data_editor.clone();
    show_blade_frame(ui, blade_height, entry.history_entry_id, |ui| {
        let _ = show_resource_detail_blade(
            ui,
            &entry.api_resource,
            &entry.namespace,
            &entry.resource_name,
            &entry.resource_uid,
            &entry.detail,
            &entry.events,
            entry.detail_error.as_deref(),
            entry.events_error.as_deref(),
            &entry.managed_resources,
            entry.managed_resources_error.as_deref(),
            data_editor.as_mut(),
        );
    });
}

fn show_blade_frame(
    ui: &mut egui::Ui,
    blade_height: f32,
    history_entry_id: u64,
    add_content: impl FnOnce(&mut egui::Ui),
) {
    egui::ScrollArea::vertical()
        .id_salt(("resource-detail-panel-scroll", history_entry_id))
        .auto_shrink([false, false])
        .min_scrolled_height(blade_height)
        .max_height(blade_height)
        .show(ui, |ui| {
            ui.set_width(PANEL_WIDTH);
            egui::Frame::new()
                .fill(WHITE)
                .stroke(egui::Stroke::new(1.0, gray::_200))
                .shadow(egui::Shadow {
                    offset: [-4, 0],
                    blur: 16,
                    spread: 0,
                    color: egui::Color32::BLACK.gamma_multiply(0.12),
                })
                .inner_margin(egui::Margin::same(PANEL_PADDING))
                .show(ui, |ui| {
                    ui.set_min_height(blade_height - f32::from(PANEL_PADDING) * 2.0);
                    add_content(ui);
                });
        });
}

fn show_resource_detail_header(
    ui: &mut egui::Ui,
    entry: &ResourceDetailHistoryEntry,
    is_foreground: bool,
) -> BladeResult {
    let mut result = BladeResult::default();
    let log_containers = entry
        .detail
        .as_ref()
        .and_then(|detail| match &detail.payload {
            ResourceDetailPayload::Pod(pod) => Some(pod.log_containers.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let resource = MinimalResource {
        uid: entry.resource_uid.clone(),
        name: entry.resource_name.clone(),
        namespace: entry.namespace.clone(),
        creation_timestamp: None,
        cells: Default::default(),
        log_containers,
    };
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let more_label = if is_foreground {
            format!("More actions for {}", resource.name)
        } else {
            format!("More actions for {} in background blade", resource.name)
        };
        MoreButton::new(more_label).show(ui, |menu| {
            show_resource_action_items(
                menu,
                &resource,
                &resource.log_containers,
                &mut result.action,
            );
        });
        ui.add_space(spacing::MD);
        ui.label(
            egui::RichText::new(&entry.resource_name)
                .font(typography::page_title())
                .color(gray::_900),
        );
        ui.add_space(spacing::MD);
        egui::Frame::new()
            .fill(indigo::_50)
            .corner_radius(radius::control())
            .inner_margin(egui::Margin::symmetric(
                spacing::SM as i8,
                spacing::XS as i8,
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(&entry.api_resource.kind)
                        .font(typography::body())
                        .color(indigo::_600),
                );
            });
    });
    result
}

#[allow(clippy::too_many_arguments)]
fn show_resource_detail_blade(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    _namespace: &Option<String>,
    _resource_name: &str,
    resource_uid: &str,
    detail: &Option<ResourceDetail>,
    events: &[ResourceEvent],
    detail_error: Option<&str>,
    events_error: Option<&str>,
    managed_resources: &[ManagedResource],
    managed_resources_error: Option<&str>,
    mut data_editor: Option<&mut super::state::ResourceDataEditorState>,
) -> BladeResult {
    let mut result = BladeResult::default();
    ui.set_max_width(ui.available_width() - 9.0);
    if let Some(error) = detail_error {
        error_card(ui, "Unable to load resource details", error);
    } else if let Some(detail) = detail {
        show_detail(ui, detail, &mut result.action);
        ui.add_space(16.0);
        show_resource_data(ui, detail, data_editor.as_deref_mut(), &mut result.action);
        metadata_maps(ui, detail);
    } else {
        ui.spinner();
        ui.label(egui::RichText::new("Loading resource details…").color(gray::_500));
    }
    ui.add_space(20.0);
    show_managed_resources_for(
        ui,
        api_resource,
        resource_uid,
        managed_resources,
        managed_resources_error,
        &mut result.action,
    );
    ui.add_space(16.0);
    show_events(ui, events, events_error);
    ui.add_space(16.0);
    if let Some(detail) = detail {
        show_additional_sections(ui, detail);
        ui.add_space(16.0);
    }
    result
}

fn show_managed_resources_for(
    ui: &mut egui::Ui,
    api_resource: &crate::api_resource::ApiResource,
    resource_uid: &str,
    managed_resources: &[ManagedResource],
    managed_resources_error: Option<&str>,
    pending_action: &mut Option<ResourceAction>,
) {
    let table_kinds = managed_resource_table_kinds(api_resource);
    if table_kinds.is_empty() {
        return;
    }

    for (index, (title, kind)) in table_kinds.iter().enumerate() {
        if index > 0 {
            ui.add_space(16.0);
        }
        let rows = managed_resource_rows(managed_resources, kind);
        section_header(ui, title, Some(format!("{} resources", rows.len())));
        show_managed_resource_table(
            ui,
            resource_uid,
            kind,
            &rows,
            api_resource.kind == "Node",
            pending_action,
        );
        if rows.is_empty() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(format!("No {title} found.")).color(gray::_500));
        }
    }

    if let Some(error) = managed_resources_error {
        ui.add_space(8.0);
        error_card(ui, "Unable to load all managed resources", error);
    }
}

fn show_managed_resource_table(
    ui: &mut egui::Ui,
    resource_uid: &str,
    kind: &str,
    rows: &[ManagedResourceRow],
    show_namespace_column: bool,
    pending_action: &mut Option<ResourceAction>,
) {
    let definition = managed_resource_table_definition(kind, show_namespace_column);
    let mut table = TailwindTable::new(format!("managed-resource-table-{resource_uid}-{kind}",))
        .roomy()
        .column("name", "Name", |column| column.fill_remaining());
    if show_namespace_column {
        table = table.column("namespace", "Namespace", |column| {
            column.initial_width(150.0)
        });
    }
    for column in &definition.columns {
        table = table.column(column.id.clone(), column.label.clone(), |table_column| {
            table_column.initial_width(column.initial_width)
        });
    }
    table = table.column("age", "Age", |column| column.initial_width(77.0));
    table.show_with_row_response(
        ui,
        rows,
        |ui, row, column_index| {
            let type_specific_start = 1 + usize::from(show_namespace_column);
            match column_index {
                0 => TableRowBuilder::text(ui, &row.name, true),
                1 if show_namespace_column => {
                    TableRowBuilder::text(ui, row.namespace.as_deref().unwrap_or("-"), false)
                }
                index
                    if index >= type_specific_start
                        && index < type_specific_start + definition.columns.len() =>
                {
                    let column = &definition.columns[index - type_specific_start];
                    show_resource_cell(ui, row.cells.get(&column.id));
                }
                _ => TableRowBuilder::text(ui, &format_age(row.creation_timestamp), false),
            }
        },
        |row_response, row, column_index| {
            let name_cell_clicked = column_index == 0
                && row_response.ctx.input(|input| {
                    input.pointer.button_clicked(egui::PointerButton::Primary)
                        && input
                            .pointer
                            .latest_pos()
                            .is_some_and(|position| row_response.interact_rect.contains(position))
                });
            if name_cell_clicked && pending_action.is_none() {
                *pending_action = Some(ResourceAction::NavigateDetails {
                    api_resource: row.api_resource.clone(),
                    name: row.name.clone(),
                    namespace: row.namespace.clone(),
                    uid: row.uid.clone(),
                });
            }
        },
    );
}

#[derive(Clone)]
struct ManagedResourceRow {
    api_resource: crate::api_resource::ApiResource,
    name: String,
    namespace: Option<String>,
    uid: String,
    creation_timestamp: Option<time::OffsetDateTime>,
    cells: BTreeMap<String, crate::resource_table::CellValue>,
}

fn managed_resource_rows(resources: &[ManagedResource], kind: &str) -> Vec<ManagedResourceRow> {
    let mut rows = resources
        .iter()
        .filter(|resource| resource.api_resource.kind == kind)
        .map(ManagedResourceRow::from)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    rows
}

impl From<&ManagedResource> for ManagedResourceRow {
    fn from(resource: &ManagedResource) -> Self {
        Self {
            api_resource: resource.api_resource.clone(),
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
            uid: resource.uid.clone(),
            creation_timestamp: resource.creation_timestamp,
            cells: resource.cells.clone(),
        }
    }
}

fn managed_resource_table_definition(
    kind: &str,
    omit_contextual_node_column: bool,
) -> ResourceTableDefinition {
    let mut definition = table_definition(&managed_resource_api_resource(kind), &[]);
    if kind == "Pod" {
        // The inspector panel is substantially narrower than the workspace.
        // Container indicators are useful in the primary list, but in this
        // context they crowd out the Pod name while status and restart counts
        // remain directly actionable.
        definition
            .columns
            .retain(|column| column.id != CONTAINERS_COLUMN);
        if omit_contextual_node_column {
            // All listed Pods are scheduled to the inspected Node, so repeating
            // its name consumes scarce inspector width without adding context.
            definition.columns.retain(|column| column.id != NODE_COLUMN);
        }
    }
    definition
}

fn managed_resource_api_resource(kind: &str) -> crate::api_resource::ApiResource {
    let (group, name) = match kind {
        "ReplicaSet" => ("apps", "replicasets"),
        "Job" => ("batch", "jobs"),
        "Pod" => ("core", "pods"),
        _ => unreachable!("managed resource table kind must be supported"),
    };
    crate::api_resource::ApiResource {
        group: group.to_owned(),
        version: "v1".to_owned(),
        kind: kind.to_owned(),
        name: name.to_owned(),
        namespaced: true,
    }
}

fn managed_resource_table_kinds(
    api_resource: &crate::api_resource::ApiResource,
) -> &'static [(&'static str, &'static str)] {
    match (api_resource.group.as_str(), api_resource.kind.as_str()) {
        ("apps", "Deployment") => &[("ReplicaSets", "ReplicaSet"), ("Pods", "Pod")],
        ("batch", "CronJob") => &[("Jobs", "Job"), ("Pods", "Pod")],
        ("apps", "ReplicaSet")
        | ("apps", "StatefulSet")
        | ("apps", "DaemonSet")
        | ("core", "ReplicationController")
        | ("batch", "Job") => &[("Pods", "Pod")],
        ("core", "Node") => &[("Pods", "Pod")],
        _ => &[],
    }
}

fn show_detail(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    pending_action: &mut Option<ResourceAction>,
) {
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        show_pod_summary(ui, detail, pod, pending_action);
        ui.add_space(13.0);
        show_pod_detail(ui, pod);
    } else if let ResourceDetailPayload::Node(node) = &detail.payload {
        show_generic_summary(ui, detail);
        ui.add_space(13.0);
        show_node_detail(ui, node);
    } else {
        show_generic_summary(ui, detail);
    }
}

fn show_node_detail(ui: &mut egui::Ui, node: &NodeDetail) {
    section_header(ui, "Spec", None);
    let pod_cidrs = if node.pod_cidrs.is_empty() {
        "-".to_owned()
    } else {
        node.pod_cidrs.join(", ")
    };
    let taints = if node.taints.is_empty() {
        "None".to_owned()
    } else {
        node.taints.join(", ")
    };
    detail_item_card(
        ui,
        |_| {},
        |ui| {
            detail_row(
                ui,
                "Scheduling",
                if node.unschedulable {
                    "Scheduling disabled"
                } else {
                    "Schedulable"
                },
            );
            detail_row(
                ui,
                "Provider ID",
                node.provider_id.as_deref().unwrap_or("-"),
            );
            detail_row(ui, "Pod CIDRs", &pod_cidrs);
            detail_row(ui, "Taints", &taints);
        },
    );
}

fn show_generic_summary(ui: &mut egui::Ui, detail: &ResourceDetail) {
    detail_summary_card(ui, |ui| {
        detail_grid(ui, |ui, column| match column {
            0 => detail_value(
                ui,
                "Namespace",
                detail.namespace.as_deref().unwrap_or("Cluster-wide"),
            ),
            1 => detail_value(ui, "Age", &format_age(detail.creation_timestamp)),
            _ => detail_value(ui, "UID", &detail.uid),
        });
    });
}

fn show_resource_data(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    editor: Option<&mut super::state::ResourceDataEditorState>,
    pending_action: &mut Option<ResourceAction>,
) {
    let Some(editor) = editor else {
        return;
    };
    match &detail.payload {
        ResourceDetailPayload::ConfigMap(config_map) => {
            show_config_map_data(ui, config_map, editor, pending_action)
        }
        ResourceDetailPayload::Secret(secret) => {
            show_secret_data(ui, secret, editor, pending_action)
        }
        ResourceDetailPayload::Generic
        | ResourceDetailPayload::Pod(_)
        | ResourceDetailPayload::Node(_) => {}
    }
}

fn show_config_map_data(
    ui: &mut egui::Ui,
    config_map: &ConfigMapDetail,
    editor: &mut super::state::ResourceDataEditorState,
    pending_action: &mut Option<ResourceAction>,
) {
    section_header(
        ui,
        "Data",
        Some(format!(
            "{} entries · {}",
            config_map.data.len(),
            if config_map.immutable {
                "Immutable"
            } else {
                "Mutable"
            }
        )),
    );
    if config_map.data.is_empty() {
        detail_message_card(ui, |ui| {
            ui.label(egui::RichText::new("No text data entries.").color(gray::_500));
        });
    }
    for key in config_map.data.keys() {
        data_entry(
            ui,
            key,
            None,
            |_| {},
            |ui| data_value_editor(ui, key, editor, config_map.immutable),
        );
    }
    data_save_controls(ui, editor, config_map.immutable, pending_action);
    ui.add_space(16.0);
}

fn show_secret_data(
    ui: &mut egui::Ui,
    secret: &SecretDetail,
    editor: &mut super::state::ResourceDataEditorState,
    pending_action: &mut Option<ResourceAction>,
) {
    section_header(
        ui,
        "Data",
        Some(format!(
            "{} entries · {} · {}",
            secret.data.len(),
            secret.type_,
            if secret.immutable {
                "Immutable"
            } else {
                "Mutable"
            }
        )),
    );
    if secret.data.is_empty() {
        detail_message_card(ui, |ui| {
            ui.label(egui::RichText::new("No data entries.").color(gray::_500));
        });
    }
    for (key, value) in &secret.data {
        let revealed = editor.revealed_secret_keys.contains(key);
        let mut visibility_toggled = false;
        data_entry(
            ui,
            key,
            Some(value.byte_len),
            |ui| {
                if value.text.is_some()
                    && TailwindButton::secondary(if revealed { "Hide" } else { "Reveal" })
                        .size(ButtonSize::Sm)
                        .show(ui)
                        .clicked()
                {
                    visibility_toggled = true;
                }
            },
            |ui| match value.text.as_ref() {
                Some(_) if revealed => data_value_editor(ui, key, editor, secret.immutable),
                Some(_) => secret_value_mask(ui),
                None => unavailable_secret_value(ui),
            },
        );
        if visibility_toggled {
            if revealed {
                editor.revealed_secret_keys.remove(key);
            } else {
                editor.revealed_secret_keys.insert(key.clone());
            }
        }
    }
    data_save_controls(ui, editor, secret.immutable, pending_action);
    ui.add_space(16.0);
}

fn detail_summary_card(ui: &mut egui::Ui, add_content: impl FnOnce(&mut egui::Ui)) {
    WorkspaceCard::new().padding(18).show(ui, add_content);
}

fn detail_item_card(
    ui: &mut egui::Ui,
    add_header: impl FnOnce(&mut egui::Ui),
    add_content: impl FnOnce(&mut egui::Ui),
) {
    WorkspaceCard::new()
        .padding(CARD_CONTENT_PADDING)
        .show(ui, |ui| {
            add_header(ui);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            add_content(ui);
        });
}

fn detail_message_card(ui: &mut egui::Ui, add_content: impl FnOnce(&mut egui::Ui)) {
    WorkspaceCard::new()
        .padding(CARD_CONTENT_PADDING)
        .show(ui, add_content);
}

fn data_entry(
    ui: &mut egui::Ui,
    key: &str,
    byte_len: Option<usize>,
    add_action: impl FnOnce(&mut egui::Ui),
    add_value: impl FnOnce(&mut egui::Ui),
) {
    detail_item_card(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(key)
                        .monospace()
                        .strong()
                        .color(gray::_800),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    add_action(ui);
                    if let Some(byte_len) = byte_len {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("{byte_len} bytes"))
                                .font(typography::metadata())
                                .color(gray::_500),
                        );
                    }
                });
            });
        },
        add_value,
    );
    ui.add_space(8.0);
}

fn data_value_editor(
    ui: &mut egui::Ui,
    key: &str,
    editor: &mut super::state::ResourceDataEditorState,
    immutable: bool,
) {
    let value = editor
        .draft_values
        .get_mut(key)
        .expect("typed data detail and editor keys remain in sync");
    let response = TailwindTextArea::new(value)
        .id_salt(("resource-data-value", key))
        .monospace()
        .desired_rows(3)
        .enabled(!immutable && !editor.saving)
        .show(ui);
    if response.hovered() && immutable {
        response.on_hover_text("This resource's data is immutable.");
    }
}

fn secret_value_mask(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(gray::_50)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM + spacing::XS) as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("••••••••••••")
                    .monospace()
                    .color(gray::_700),
            );
        });
}

fn unavailable_secret_value(ui: &mut egui::Ui) {
    egui::Frame::new()
        .fill(gray::_50)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::control())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM + spacing::XS) as i8,
            spacing::SM as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Binary data")
                    .strong()
                    .color(gray::_700),
            );
            ui.label(
                egui::RichText::new("This value cannot be edited in the inspector.")
                    .font(typography::metadata())
                    .color(gray::_600),
            );
        });
}

fn data_save_controls(
    ui: &mut egui::Ui,
    editor: &mut super::state::ResourceDataEditorState,
    immutable: bool,
    pending_action: &mut Option<ResourceAction>,
) {
    if let Some(error) = &editor.save_error {
        ui.colored_label(status::DANGER, error);
        ui.add_space(spacing::SM);
    }
    if immutable {
        ui.label(egui::RichText::new("Data is immutable and cannot be edited.").color(gray::_500));
        return;
    }
    let (expected_values, updated_values) = editor.changed_values();
    let save_clicked = ui
        .horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!editor.saving && !updated_values.is_empty(), |ui| {
                    TailwindButton::primary(if editor.saving {
                        "Saving…"
                    } else {
                        "Save data"
                    })
                    .size(ButtonSize::Sm)
                    .show(ui)
                })
                .inner
            })
            .inner
        })
        .inner
        .clicked();
    if save_clicked && pending_action.is_none() {
        editor.saving = true;
        editor.save_error = None;
        *pending_action = Some(ResourceAction::SaveData {
            expected_values,
            updated_values,
        });
    }
}

fn show_data_conflict_dialog(
    ctx: &egui::Context,
    editor: Option<&mut super::state::ResourceDataEditorState>,
) {
    let Some(editor) = editor else {
        return;
    };
    if editor.pending_external_values.is_none() {
        return;
    }
    let mut use_external = false;
    let mut keep_local = false;
    egui::Window::new("Data changed on cluster")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label("This resource changed on the cluster while you have unsaved data edits.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Use cluster version")
                    .with_pointing_hand()
                    .clicked()
                {
                    use_external = true;
                }
                if ui.button("Keep my edits").with_pointing_hand().clicked() {
                    keep_local = true;
                }
            });
        });
    if use_external {
        editor.use_external_values();
    } else if keep_local {
        editor.keep_local_edits();
    }
}

fn show_pod_summary(
    ui: &mut egui::Ui,
    detail: &ResourceDetail,
    pod: &PodDetail,
    pending_action: &mut Option<ResourceAction>,
) {
    let ready = format!(
        "{}/{}",
        pod.containers
            .iter()
            .filter(|container| container.ready)
            .count(),
        pod.containers.len()
    );
    detail_summary_card(ui, |ui| {
        detail_grid(ui, |ui, column| match column {
            0 => detail_value(
                ui,
                "Namespace",
                detail.namespace.as_deref().unwrap_or("Cluster-wide"),
            ),
            1 => status_value(ui, "Status", &pod.phase),
            _ => detail_node_value(ui, pod.node_name.as_deref(), pending_action),
        });
        ui.separator();
        detail_grid(ui, |ui, column| match column {
            0 => detail_value(ui, "Pod IP", pod.pod_ip.as_deref().unwrap_or("-")),
            1 => detail_value(ui, "Host IP", pod.host_ip.as_deref().unwrap_or("-")),
            _ => detail_value(ui, "QoS class", pod.qos_class.as_deref().unwrap_or("-")),
        });
        ui.separator();
        detail_grid(ui, |ui, column| match column {
            0 => detail_value(ui, "Ready", &ready),
            1 => detail_value(ui, "Age", &format_age(detail.creation_timestamp)),
            _ => {}
        });
        // Keep the final, two-column row balanced with the full rows above.
        ui.add_space(8.0);
    });
}

fn detail_node_value(
    ui: &mut egui::Ui,
    node_name: Option<&str>,
    pending_action: &mut Option<ResourceAction>,
) {
    ui.label(
        egui::RichText::new("Node")
            .font(typography::metadata())
            .color(gray::_500),
    );
    let Some(node_name) = node_name else {
        ui.label(
            egui::RichText::new("-")
                .font(typography::metadata())
                .color(gray::_900),
        )
        .on_hover_text("Kubernetes has not assigned this Pod to a Node.");
        return;
    };
    let response = ui.add(
        egui::Label::new(
            egui::RichText::new(node_name)
                .font(typography::metadata())
                .color(indigo::_600),
        )
        .sense(egui::Sense::click()),
    );
    response.clone().with_pointing_hand().widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            response.enabled(),
            format!("Open details for Node {node_name}"),
        )
    });
    if response.clicked() && pending_action.is_none() {
        *pending_action = Some(ResourceAction::NavigateDetails {
            api_resource: crate::resource_handlers::node::api_resource(),
            name: node_name.to_owned(),
            namespace: None,
            // The node watcher can load the detail by name. Until its first
            // detail update, use the name for the event selector as well.
            uid: node_name.to_owned(),
        });
    }
}

fn show_pod_detail(ui: &mut egui::Ui, pod: &PodDetail) {
    section_header(ui, "Containers", None);
    for container in &pod.containers {
        detail_item_card(
            ui,
            |ui| {
                ui.label(
                    egui::RichText::new(format!("⌃   {}", container.name))
                        .strong()
                        .color(gray::_800),
                );
            },
            |ui| {
                detail_grid_columns(ui, 2, |ui, column| match column {
                    0 => detail_value(ui, "Image", &container.image),
                    1 => detail_value(ui, "State", &container.state),
                    _ => {}
                });
                ui.add_space(6.0);
                detail_grid_columns(ui, 2, |ui, column| match column {
                    0 => detail_value(ui, "Ready", if container.ready { "Yes" } else { "No" }),
                    1 => detail_value(ui, "Restarts", &container.restart_count.to_string()),
                    _ => {}
                });
                if !container.command.is_empty() {
                    chip_row(ui, "Command", &container.command);
                }
                if !container.args.is_empty() {
                    chip_row(ui, "Args", &container.args);
                }
                if !container.ports.is_empty() {
                    chip_row(ui, "Ports", &container.ports);
                }
                if !container.environment_variables.is_empty() {
                    ui.add_space(10.0);
                    environment_variables(ui, &container.environment_variables);
                }
                if let Some(reason) = &container.reason {
                    detail_value(ui, "Reason", reason);
                }
                if let Some(message) = &container.message {
                    ui.label(
                        egui::RichText::new(message)
                            .font(typography::metadata())
                            .color(gray::_500),
                    );
                }
            },
        );
        ui.add_space(CARD_GAP);
    }
    if !pod.volumes.is_empty() {
        section_header(ui, "Volumes", None);
        for (index, volume) in pod.volumes.iter().enumerate() {
            detail_item_card(
                ui,
                |ui| {
                    ui.label(
                        egui::RichText::new(format!("⌄   {}", volume.name))
                            .strong()
                            .color(gray::_800),
                    );
                },
                |ui| volume_detail_row(ui, volume),
            );
            if index + 1 < pod.volumes.len() {
                ui.add_space(CARD_GAP);
            }
        }
    }
}

fn metadata_maps(ui: &mut egui::Ui, detail: &ResourceDetail) {
    disclosure_card(
        ui,
        "labels-and-annotations-open",
        "Labels & annotations",
        false,
        |ui| {
            ui.label(egui::RichText::new("Labels").strong().color(gray::_800));
            for (key, value) in &detail.labels {
                detail_row(ui, key, value);
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Annotations")
                    .strong()
                    .color(gray::_800),
            );
            for (key, value) in &detail.annotations {
                detail_row(ui, key, value);
            }
        },
    );
}

fn show_events(ui: &mut egui::Ui, events: &[ResourceEvent], error: Option<&str>) {
    WorkspaceCard::new().padding(0).show(ui, |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(CARD_HEADER_PADDING);
                ui.label(
                    egui::RichText::new("Events")
                        .strong()
                        .font(typography::section_heading())
                        .color(gray::_800),
                );
            },
        );
        ui.separator();
        egui::Frame::new()
            .inner_margin(egui::Margin::same(CARD_CONTENT_PADDING))
            .show(ui, |ui| {
                if let Some(error) = error {
                    error_card(ui, "Unable to load events", error);
                } else if events.is_empty() {
                    ui.label(egui::RichText::new("No events recorded.").color(gray::_500));
                } else {
                    detail_grid_columns(ui, 5, |ui, column| match column {
                        0 => event_header(ui, "Type"),
                        1 => event_header(ui, "Reason"),
                        2 => event_header(ui, "Message"),
                        3 => event_header(ui, "Source"),
                        _ => event_header(ui, "Time"),
                    });
                    ui.separator();
                    for event in events {
                        detail_grid_columns(ui, 5, |ui, column| match column {
                            0 => status_value(ui, "", &event.type_),
                            1 => detail_value(ui, "", &event.reason),
                            2 => detail_value(ui, "", &event.message),
                            3 => detail_value(
                                ui,
                                "",
                                event.source.as_deref().unwrap_or("Kubernetes"),
                            ),
                            _ => detail_value(
                                ui,
                                "",
                                &format!("{} ago", format_age(event.last_timestamp)),
                            ),
                        });
                    }
                }
            });
    });
}

fn show_additional_sections(ui: &mut egui::Ui, detail: &ResourceDetail) {
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        disclosure_card(ui, "conditions", "Conditions", false, |ui| {
            if pod.conditions.is_empty() {
                ui.label(egui::RichText::new("No conditions reported.").color(gray::_500));
            } else {
                detail_grid_columns(ui, 4, |ui, column| match column {
                    0 => event_header(ui, "Type"),
                    1 => event_header(ui, "Status"),
                    2 => event_header(ui, "Reason"),
                    _ => event_header(ui, "Message"),
                });
                ui.separator();
                for condition in &pod.conditions {
                    detail_grid_columns(ui, 4, |ui, column| match column {
                        0 => detail_value(ui, "", &condition.type_),
                        1 => status_value(ui, "", &condition.status),
                        2 => detail_value(ui, "", condition.reason.as_deref().unwrap_or("-")),
                        _ => detail_value(ui, "", condition.message.as_deref().unwrap_or("-")),
                    });
                }
            }
        });
        ui.add_space(16.0);
    }
    disclosure_card(ui, "owner-references", "Owner references", false, |ui| {
        if let Some(owner) = &detail.owner {
            detail_row(ui, "Kind", &owner.kind);
            detail_row(ui, "Name", &owner.name);
        } else {
            ui.label(egui::RichText::new("No owner references.").color(gray::_500));
        }
    });
    if let ResourceDetailPayload::Pod(pod) = &detail.payload {
        ui.add_space(16.0);
        disclosure_card(
            ui,
            "resource-configuration",
            "Resource configuration",
            true,
            |ui| {
                detail_row(
                    ui,
                    "Restart policy",
                    pod.restart_policy.as_deref().unwrap_or("-"),
                );
                detail_row(
                    ui,
                    "Service account",
                    pod.service_account_name.as_deref().unwrap_or("-"),
                );
                detail_row(ui, "DNS policy", pod.dns_policy.as_deref().unwrap_or("-"));
            },
        );
    }
}

fn disclosure_card(
    ui: &mut egui::Ui,
    id_source: &str,
    title: &str,
    default_open: bool,
    add_content: impl FnOnce(&mut egui::Ui),
) {
    WorkspaceCard::new().padding(0).show(ui, |ui| {
        let id = ui.id().with(id_source);
        let mut open = ui
            .data(|data| data.get_temp::<bool>(id))
            .unwrap_or(default_open);
        let (header, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), CARD_HEADER_HEIGHT),
            egui::Sense::click(),
        );
        let response = response.with_pointing_hand();
        if response.clicked() {
            open = !open;
            ui.data_mut(|data| data.insert_temp(id, open));
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::CollapsingHeader,
                ui.is_enabled(),
                open,
                title,
            )
        });
        let painter = ui.painter();
        painter.text(
            header.left_center() + egui::vec2(CARD_HEADER_PADDING, 0.0),
            egui::Align2::LEFT_CENTER,
            title,
            typography::body(),
            gray::_800,
        );
        painter.text(
            header.right_center() - egui::vec2(CARD_HEADER_PADDING, 0.0),
            egui::Align2::RIGHT_CENTER,
            if open { "⌃" } else { "⌄" },
            typography::section_heading(),
            gray::_700,
        );
        if open {
            ui.separator();
            egui::Frame::new()
                .inner_margin(egui::Margin::same(CARD_CONTENT_PADDING))
                .show(ui, add_content);
        }
    });
}

fn section_header(ui: &mut egui::Ui, title: &str, detail: Option<String>) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .strong()
                .font(typography::section_heading())
                .color(gray::_800),
        );
        if let Some(detail) = detail {
            ui.label(
                egui::RichText::new(detail)
                    .font(typography::body())
                    .color(gray::_600),
            );
        }
    });
    ui.add_space(6.0);
}

fn detail_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!("{label}: "))
                .font(typography::metadata())
                .color(gray::_500),
        );
        ui.label(
            egui::RichText::new(value)
                .font(typography::metadata())
                .color(gray::_800),
        );
    });
}

fn environment_variables(
    ui: &mut egui::Ui,
    variables: &[crate::resource_detail::PodEnvironmentVariableDetail],
) {
    ui.label(
        egui::RichText::new("Environment variables")
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(gray::_100)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            spacing::SM as i8,
            (spacing::SM + 2.0) as i8,
        ))
        .show(ui, |ui| {
            environment_variable_header(ui);
            for variable in variables {
                ui.add_space(2.0);
                environment_variable_row(ui, variable);
            }
        });
}

fn environment_variable_header(ui: &mut egui::Ui) {
    ui.columns(3, |columns| {
        environment_variable_cell(&mut columns[0], "Key", true);
        environment_variable_cell(&mut columns[1], "Value", true);
        environment_variable_cell(&mut columns[2], "Source", true);
    });
}

fn environment_variable_row(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    ui.columns(3, |columns| {
        environment_variable_cell(&mut columns[0], &variable.name, false);
        environment_variable_value_cell(&mut columns[1], variable);
        environment_variable_source_cell(&mut columns[2], variable);
    });
}

fn environment_variable_cell(ui: &mut egui::Ui, value: &str, header: bool) {
    let text = egui::RichText::new(value)
        .monospace()
        .font(typography::monospace())
        .color(if header { gray::_600 } else { gray::_800 });
    let text = if header { text.strong() } else { text };
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM - 2.0) as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width());
            ui.add(egui::Label::new(text).selectable(!header).wrap());
        });
}

fn environment_variable_value_cell(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let secret = matches!(
        variable.source,
        crate::resource_detail::PodEnvironmentVariableSource::SecretKey { .. }
    );
    let revealed = secret && environment_variable_secret_revealed(ui, variable);
    let value = if secret && !revealed {
        "••••••"
    } else {
        variable.value.as_deref().unwrap_or("Unavailable")
    };
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM - 2.0) as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                if secret && variable.value.is_some() {
                    let action = if revealed { "Hide" } else { "Reveal" };
                    let response = components::icons::eye_button(ui, 14.0, gray::_600, action);
                    if response.on_hover_text(action).clicked() {
                        ui.data_mut(|data| {
                            data.insert_temp(
                                environment_variable_secret_id(ui, variable),
                                !revealed,
                            )
                        });
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(value)
                                .monospace()
                                .font(typography::monospace())
                                .color(gray::_800),
                        )
                        .selectable(true)
                        .wrap(),
                    );
                });
            });
        });
}

fn environment_variable_source_cell(
    ui: &mut egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) {
    let source = environment_variable_source_label(variable);
    egui::Frame::new()
        .fill(WHITE)
        .stroke(egui::Stroke::new(1.0, gray::_200))
        .corner_radius(radius::subtle())
        .inner_margin(egui::Margin::symmetric(
            (spacing::SM - 2.0) as i8,
            spacing::XS as i8,
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(source)
                            .font(typography::metadata())
                            .color(gray::_600),
                    )
                    .wrap(),
                );
            });
        });
}

fn environment_variable_secret_revealed(
    ui: &egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> bool {
    ui.data(|data| {
        data.get_temp::<bool>(environment_variable_secret_id(ui, variable))
            .unwrap_or(false)
    })
}

fn environment_variable_secret_id(
    _ui: &egui::Ui,
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> egui::Id {
    egui::Id::new((
        "environment-variable-secret",
        &variable.name,
        environment_variable_source_label(variable),
    ))
}

fn environment_variable_source_label(
    variable: &crate::resource_detail::PodEnvironmentVariableDetail,
) -> String {
    use crate::resource_detail::PodEnvironmentVariableSource;

    let resolved = if variable.value.is_some() {
        "resolved"
    } else {
        "unavailable"
    };
    match &variable.source {
        PodEnvironmentVariableSource::Literal => "Literal".to_owned(),
        PodEnvironmentVariableSource::ConfigMapKey {
            name,
            key,
            optional,
        } => {
            format!(
                "ConfigMap {name}/{key}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::SecretKey {
            name,
            key,
            optional,
        } => {
            format!(
                "Secret {name}/{key}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::Field { path } => format!("Field {path} · {resolved}"),
        PodEnvironmentVariableSource::ResourceField {
            resource,
            container_name,
        } => format!(
            "Resource field {resource}{} · {resolved}",
            container_name
                .as_deref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default()
        ),
        PodEnvironmentVariableSource::ConfigMapImport { name, optional, .. } => {
            format!(
                "ConfigMap import {name}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::SecretImport { name, optional, .. } => {
            format!(
                "Secret import {name}{} · {resolved}",
                optional_label(*optional)
            )
        }
        PodEnvironmentVariableSource::Unspecified => "Unspecified source".to_owned(),
    }
}

fn optional_label(optional: bool) -> &'static str {
    if optional { " (optional)" } else { "" }
}

fn detail_grid(ui: &mut egui::Ui, add_column: impl FnMut(&mut egui::Ui, usize)) {
    detail_grid_columns(ui, 3, add_column);
}

fn detail_grid_columns(
    ui: &mut egui::Ui,
    column_count: usize,
    mut add_column: impl FnMut(&mut egui::Ui, usize),
) {
    ui.columns(column_count, |columns| {
        for (index, column) in columns.iter_mut().enumerate() {
            add_column(column, index);
        }
    });
    ui.add_space(10.0);
}

fn detail_value(ui: &mut egui::Ui, label: &str, value: &str) {
    if !label.is_empty() {
        ui.label(
            egui::RichText::new(label)
                .font(typography::metadata())
                .color(gray::_500),
        );
    }
    ui.label(
        egui::RichText::new(value)
            .font(typography::metadata())
            .color(gray::_900),
    );
}

fn status_value(ui: &mut egui::Ui, label: &str, value: &str) {
    if !label.is_empty() {
        ui.label(
            egui::RichText::new(label)
                .font(typography::metadata())
                .color(gray::_500),
        );
    }
    ui.horizontal(|ui| {
        ui.colored_label(status::SUCCESS, "●");
        ui.label(
            egui::RichText::new(value)
                .font(typography::metadata())
                .color(gray::_900),
        );
    });
}

fn event_header(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .font(typography::metadata())
            .color(gray::_500),
    );
}

fn chip_row(ui: &mut egui::Ui, label: &str, values: &[String]) {
    ui.label(
        egui::RichText::new(label)
            .font(typography::metadata())
            .color(gray::_500),
    );
    ui.with_layout(
        egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true),
        |ui| {
            for value in values {
                let chip_width = (value.chars().count() as f32 * 6.7 + 10.0).clamp(36.0, 320.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(chip_width, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        egui::Frame::new()
                            .fill(gray::_100)
                            .stroke(egui::Stroke::new(1.0, gray::_200))
                            .corner_radius(radius::subtle())
                            .inner_margin(egui::Margin::symmetric((spacing::SM - 3.0) as i8, 0))
                            .show(ui, |ui| {
                                ui.set_max_width(chip_width - 10.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(value)
                                            .monospace()
                                            .font(typography::monospace())
                                            .color(gray::_800),
                                    )
                                    .wrap(),
                                );
                            });
                    },
                );
            }
        },
    );
}

fn volume_detail_row(ui: &mut egui::Ui, volume: &crate::resource_detail::PodVolumeDetail) {
    const COLUMN_WIDTHS: [f32; 3] = [117.0, 159.0, 307.0];
    ui.horizontal_top(|ui| {
        for (width, label, value) in [
            (COLUMN_WIDTHS[0], "Type", volume.kind.as_str()),
            (COLUMN_WIDTHS[1], "Source", volume.source.as_str()),
            (
                COLUMN_WIDTHS[2],
                "Mount path",
                volume.mount_path.as_deref().unwrap_or("-"),
            ),
        ] {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| detail_value(ui, label, value),
            );
        }
        detail_value(
            ui,
            "Read-only",
            if volume.read_only { "true" } else { "false" },
        );
    });
}

fn error_card(ui: &mut egui::Ui, title: &str, error: &str) {
    ui.label(egui::RichText::new(title).strong().color(status::DANGER));
    ui.label(
        egui::RichText::new(error)
            .font(typography::metadata())
            .color(gray::_600),
    );
}
