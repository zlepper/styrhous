use super::state::ResourceAction;
use crate::api_resource::ApiResource;
use crate::minimal_resource::{MinimalResource, PodLogContainer};
use crate::resource_table::ContainerKind;
use crate::terminal_launcher::DebugImagePreset;
use components::colors::gray;
use components::design::status;
use components::{MoreMenu, icons};

/// Render the shared resource-level actions used by table rows and inspectors.
pub(super) fn show_resource_action_items(
    menu: &mut MoreMenu<'_>,
    api_resource: &ApiResource,
    resource: &MinimalResource,
    log_containers: &[PodLogContainer],
    debug_image_presets: &[DebugImagePreset],
    supports_scale: bool,
    pending_action: &mut Option<ResourceAction>,
) {
    if api_resource.kind == "Node" && !debug_image_presets.is_empty() {
        menu.submenu("Shell", |menu: &mut MoreMenu<'_>| {
            for preset in debug_image_presets {
                if menu.action(preset.menu_label()).clicked() && pending_action.is_none() {
                    *pending_action = Some(ResourceAction::NodeShell {
                        name: resource.name.clone(),
                        preset: preset.clone(),
                    });
                    menu.close();
                }
            }
        });
        menu.separator();
    }
    let shell_containers = log_containers
        .iter()
        .filter(|container| matches!(container.kind, ContainerKind::App))
        .collect::<Vec<_>>();
    let pod_image_presets = pod_image_presets(log_containers, debug_image_presets);
    if api_resource.kind == "Pod"
        && !shell_containers.is_empty()
        && (!debug_image_presets.is_empty() || !pod_image_presets.is_empty())
    {
        menu.submenu("Debug shell", |menu: &mut MoreMenu<'_>| {
            for target in &shell_containers {
                menu.submenu(&target.name, |menu: &mut MoreMenu<'_>| {
                    for preset in debug_image_presets {
                        add_pod_debug_shell_action(menu, resource, target, preset, pending_action);
                    }
                    if !debug_image_presets.is_empty() && !pod_image_presets.is_empty() {
                        menu.separator();
                    }
                    for preset in &pod_image_presets {
                        add_pod_debug_shell_action(menu, resource, target, preset, pending_action);
                    }
                });
            }
        });
        menu.separator();
    }
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
            menu.submenu("Shell", |menu: &mut MoreMenu<'_>| {
                for container in containers {
                    if menu.action(&container.name).clicked() && pending_action.is_none() {
                        *pending_action = Some(ResourceAction::Shell {
                            name: resource.name.clone(),
                            namespace: resource.namespace.clone(),
                            container: (*container).clone(),
                        });
                    }
                }
            });
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
            menu.submenu("View logs", |menu: &mut MoreMenu<'_>| {
                for container in containers {
                    let label = format!("{} — {}", container.name, container.kind.label());
                    if menu.action(label).clicked() && pending_action.is_none() {
                        *pending_action = Some(ResourceAction::ViewLogs {
                            name: resource.name.clone(),
                            namespace: resource.namespace.clone(),
                            container: container.clone(),
                        });
                    }
                }
            });
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

fn pod_image_presets(
    log_containers: &[PodLogContainer],
    configured_presets: &[DebugImagePreset],
) -> Vec<DebugImagePreset> {
    let mut pod_image_presets = Vec::new();
    for image in log_containers
        .iter()
        .filter_map(|container| container.image.as_ref())
        .filter(|image| !image.trim().is_empty())
    {
        let preset = DebugImagePreset {
            name: image.clone(),
            image: image.clone(),
            profile: crate::terminal_launcher::DebugProfile::General,
        };
        if !configured_presets
            .iter()
            .any(|existing| existing.image == preset.image && existing.profile == preset.profile)
            && !pod_image_presets
                .iter()
                .any(|existing: &DebugImagePreset| existing.image == preset.image)
        {
            pod_image_presets.push(preset);
        }
    }
    pod_image_presets
}

fn add_pod_debug_shell_action(
    menu: &mut MoreMenu<'_>,
    resource: &MinimalResource,
    target: &PodLogContainer,
    preset: &DebugImagePreset,
    pending_action: &mut Option<ResourceAction>,
) {
    if menu.action(preset.menu_label()).clicked() && pending_action.is_none() {
        *pending_action = Some(ResourceAction::PodDebugShell {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
            target_container: target.name.clone(),
            preset: preset.clone(),
        });
        menu.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(name: &str, kind: ContainerKind, image: Option<&str>) -> PodLogContainer {
        PodLogContainer {
            name: name.into(),
            kind,
            image: image.map(str::to_owned),
        }
    }

    #[test]
    fn pod_image_presets_collect_distinct_declared_images_not_offered_by_configured_presets() {
        let configured = vec![DebugImagePreset {
            name: "Busybox".into(),
            image: "busybox".into(),
            profile: crate::terminal_launcher::DebugProfile::General,
        }];
        let containers = vec![
            container(
                "setup",
                ContainerKind::Init,
                Some("registry.example/setup:v1"),
            ),
            container("api", ContainerKind::App, Some("busybox")),
            container(
                "sidecar",
                ContainerKind::App,
                Some("registry.example/api:v1"),
            ),
            container(
                "debugger",
                ContainerKind::Ephemeral,
                Some("registry.example/api:v1"),
            ),
            container("missing", ContainerKind::Ephemeral, None),
            container("blank", ContainerKind::Ephemeral, Some("  ")),
        ];

        assert_eq!(
            pod_image_presets(&containers, &configured),
            vec![
                DebugImagePreset {
                    name: "registry.example/setup:v1".into(),
                    image: "registry.example/setup:v1".into(),
                    profile: crate::terminal_launcher::DebugProfile::General,
                },
                DebugImagePreset {
                    name: "registry.example/api:v1".into(),
                    image: "registry.example/api:v1".into(),
                    profile: crate::terminal_launcher::DebugProfile::General,
                },
            ]
        );
    }
}
