use super::state::ResourceAction;
use crate::minimal_resource::{MinimalResource, PodLogContainer};
use components::colors::gray;
use components::design::status;
use components::{MoreMenu, icons};

/// Render the shared resource-level actions used by table rows and inspectors.
pub(super) fn show_resource_action_items(
    menu: &mut MoreMenu<'_>,
    resource: &MinimalResource,
    log_containers: &[PodLogContainer],
    pending_action: &mut Option<ResourceAction>,
) {
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
                    if ui.button(label).clicked() && pending_action.is_none() {
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
            "Edit YAML",
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
}
