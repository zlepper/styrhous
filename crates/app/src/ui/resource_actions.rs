use super::state::ResourceAction;
use crate::api_resource::ApiResource;
use crate::minimal_resource::{MinimalResource, PodLogContainer};
use components::colors::gray;
use components::design::status;
use components::{MoreMenu, PointingHand, icons};

/// Render the shared resource-level actions used by table rows and inspectors.
pub(super) fn show_resource_action_items(
    menu: &mut MoreMenu<'_>,
    api_resource: &ApiResource,
    resource: &MinimalResource,
    log_containers: &[PodLogContainer],
    supports_scale: bool,
    pending_action: &mut Option<ResourceAction>,
) {
    let shell_containers = log_containers
        .iter()
        .filter(|container| matches!(container.kind, crate::resource_table::ContainerKind::App))
        .collect::<Vec<_>>();
    match shell_containers.as_slice() {
        [] => {}
        [container] => {
            if menu.action("Shell").clicked() && pending_action.is_none() {
                *pending_action = Some(ResourceAction::Shell {
                    name: resource.name.clone(),
                    namespace: resource.namespace.clone(),
                    container: (*container).clone(),
                });
            }
            menu.separator();
        }
        containers => {
            let mut selected = false;
            menu.submenu("Shell", |ui| {
                for container in containers {
                    if ui.button(&container.name).with_pointing_hand().clicked()
                        && pending_action.is_none()
                    {
                        *pending_action = Some(ResourceAction::Shell {
                            name: resource.name.clone(),
                            namespace: resource.namespace.clone(),
                            container: (*container).clone(),
                        });
                        selected = true;
                    }
                }
            });
            if selected {
                menu.close();
            }
            menu.separator();
        }
    }
    match log_containers {
        [] => {}
        [container] => {
            if menu.action("View logs").clicked() && pending_action.is_none() {
                *pending_action = Some(ResourceAction::ViewLogs {
                    name: resource.name.clone(),
                    namespace: resource.namespace.clone(),
                    container: container.clone(),
                });
            }
            menu.separator();
        }
        containers => {
            let mut selected = false;
            menu.submenu("View logs", |ui| {
                for container in containers {
                    let label = format!("{} — {}", container.name, container.kind.label());
                    if ui.button(label).with_pointing_hand().clicked() && pending_action.is_none() {
                        *pending_action = Some(ResourceAction::ViewLogs {
                            name: resource.name.clone(),
                            namespace: resource.namespace.clone(),
                            container: container.clone(),
                        });
                        selected = true;
                    }
                }
            });
            if selected {
                menu.close();
            }
            menu.separator();
        }
    }
    if menu
        .action_with_icon(
            "Edit",
            icons::document_icon()
                .fit_to_exact_size(egui::Vec2::splat(16.0))
                .tint(gray::_500),
        )
        .clicked()
        && pending_action.is_none()
    {
        *pending_action = Some(ResourceAction::EditYaml {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
        });
    }
    if supports_scale {
        menu.separator();
        if menu.action("Scale").clicked() && pending_action.is_none() {
            *pending_action = Some(ResourceAction::RequestScale {
                name: resource.name.clone(),
                namespace: resource.namespace.clone(),
            });
        }
    }
    if crate::resource_handlers::deployment::supports_rollout_restart(api_resource) {
        menu.separator();
        if menu.action("Restart rollout").clicked()
            && pending_action.is_none()
            && let Some(namespace) = resource.namespace.clone()
        {
            *pending_action = Some(ResourceAction::RequestDeploymentRestart {
                name: resource.name.clone(),
                namespace,
            });
        }
    }
    menu.separator();
    if menu
        .destructive_action_with_icon(
            "Delete",
            icons::trash_icon()
                .fit_to_exact_size(egui::Vec2::splat(16.0))
                .tint(status::DANGER),
        )
        .clicked()
        && pending_action.is_none()
    {
        *pending_action = Some(ResourceAction::RequestDelete {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
        });
    }
    if resource.can_force_delete()
        && menu
            .destructive_action("Force delete (remove finalizers)")
            .clicked()
        && pending_action.is_none()
    {
        *pending_action = Some(ResourceAction::RequestForceDelete {
            name: resource.name.clone(),
            uid: resource.uid.clone(),
            namespace: resource.namespace.clone(),
            finalizers: resource.finalizers().to_vec(),
        });
    }
}
