use super::state::ResourceAction;
use crate::api_resource::ApiResource;
use crate::minimal_resource::{MinimalResource, PodLogContainer};
use crate::resource_table::ContainerKind;
use crate::terminal_launcher::DebugImagePreset;
use components::icons::Icon;
use components::{Menu, MenuItem};

/// Queue `action` when `response` was clicked and no other action is queued
/// this frame.
fn queue(
    pending_action: &mut Option<ResourceAction>,
    response: &egui::Response,
    action: impl FnOnce() -> Option<ResourceAction>,
) {
    if response.clicked() && pending_action.is_none() {
        *pending_action = action();
    }
}

/// Puts a separator between the groups of a menu that have items (S07).
#[derive(Default)]
struct MenuGroups {
    started: bool,
}

impl MenuGroups {
    /// Start the next group; call it only when the group has items.
    fn next(&mut self, menu: &mut Menu<'_>) {
        if self.started {
            menu.separator();
        }
        self.started = true;
    }
}

/// Render the shared resource-level actions used by table rows and
/// inspectors, in the S07 order: edit · inspect (logs, shells) · copy, then
/// the lifecycle actions and the destructive ones, each group set off by a
/// separator (A04).
#[allow(clippy::too_many_arguments)]
pub(super) fn show_resource_action_items(
    menu: &mut Menu<'_>,
    api_resource: &ApiResource,
    resource: &MinimalResource,
    log_containers: &[PodLogContainer],
    debug_image_presets: &[DebugImagePreset],
    supports_scale: bool,
    pending_action: &mut Option<ResourceAction>,
) {
    let mut groups = MenuGroups::default();

    groups.next(menu);
    let edit = menu.item(MenuItem::new("Edit YAML").icon(Icon::PencilSquare));
    queue(pending_action, &edit, || {
        Some(ResourceAction::EditYaml {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
        })
    });

    let shell_containers = log_containers
        .iter()
        .filter(|container| matches!(container.kind, ContainerKind::App))
        .collect::<Vec<_>>();
    let pod_image_presets = pod_image_presets(log_containers, debug_image_presets);
    let node_shell = api_resource.is_node() && !debug_image_presets.is_empty();
    let pod_debug_shell = api_resource.is_pod()
        && !shell_containers.is_empty()
        && (!debug_image_presets.is_empty() || !pod_image_presets.is_empty());
    show_log_actions(menu, resource, log_containers, pending_action);
    show_shell_actions(menu, resource, &shell_containers, pending_action);
    if pod_debug_shell {
        menu.submenu(
            MenuItem::new("Debug shell").icon(Icon::WrenchScrewdriver),
            |menu: &mut Menu<'_>| {
                for target in &shell_containers {
                    menu.submenu(&target.name, |menu: &mut Menu<'_>| {
                        show_debug_images(
                            menu,
                            debug_image_presets,
                            &pod_image_presets,
                            pending_action,
                            |preset| ResourceAction::PodDebugShell {
                                name: resource.name.clone(),
                                namespace: resource.namespace.clone(),
                                target_container: target.name.clone(),
                                preset: preset.clone(),
                            },
                        );
                    });
                }
            },
        );
    }
    if node_shell {
        menu.submenu(
            MenuItem::new("Shell").icon(Icon::CommandLine),
            |menu: &mut Menu<'_>| {
                show_debug_images(menu, debug_image_presets, &[], pending_action, |preset| {
                    ResourceAction::NodeShell {
                        name: resource.name.clone(),
                        preset: preset.clone(),
                    }
                });
            },
        );
    }

    let copy = menu.item(MenuItem::new("Copy name").icon(Icon::ClipboardDocument));
    if copy.clicked() {
        copy.ctx.copy_text(resource.name.clone());
    }

    show_lifecycle_actions(
        menu,
        &mut groups,
        api_resource,
        resource,
        supports_scale,
        pending_action,
    );

    groups.next(menu);
    let delete = menu.item(MenuItem::new("Delete").icon(Icon::Trash).danger());
    queue(pending_action, &delete, || {
        Some(ResourceAction::RequestDelete {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
        })
    });
    if resource.can_force_delete() {
        let force_delete = menu.item(
            MenuItem::new("Force delete (remove finalizers)")
                .icon(Icon::Trash)
                .danger(),
        );
        queue(pending_action, &force_delete, || {
            Some(ResourceAction::RequestForceDelete {
                name: resource.name.clone(),
                uid: resource.uid.clone(),
                namespace: resource.namespace.clone(),
                finalizers: resource.finalizers().to_vec(),
            })
        });
    }
}

/// "View logs": one item for a single container, otherwise a submenu that
/// can also interleave every container.
fn show_log_actions(
    menu: &mut Menu<'_>,
    resource: &MinimalResource,
    log_containers: &[PodLogContainer],
    pending_action: &mut Option<ResourceAction>,
) {
    let view_logs = |containers: &[PodLogContainer]| {
        let targets = log_stream_targets(resource, containers);
        (!targets.is_empty()).then_some(ResourceAction::ViewLogs { targets })
    };
    let item = MenuItem::new("View logs").icon(Icon::DocumentText);
    match log_containers {
        [] => {}
        [container] => {
            let response = menu.item(item);
            queue(pending_action, &response, || {
                view_logs(std::slice::from_ref(container))
            });
        }
        containers => {
            menu.submenu(item, |menu: &mut Menu<'_>| {
                let all = menu.action("All containers (interleaved)");
                queue(pending_action, &all, || view_logs(containers));
                menu.separator();
                for container in containers {
                    let label = format!("{} — {}", container.name, container.kind.label());
                    let response = menu.action(label);
                    queue(pending_action, &response, || {
                        view_logs(std::slice::from_ref(container))
                    });
                }
            });
        }
    }
}

/// "Shell" into an app container: one item for a single container,
/// otherwise a submenu of containers.
fn show_shell_actions(
    menu: &mut Menu<'_>,
    resource: &MinimalResource,
    shell_containers: &[&PodLogContainer],
    pending_action: &mut Option<ResourceAction>,
) {
    let shell = |container: &PodLogContainer| {
        Some(ResourceAction::Shell {
            name: resource.name.clone(),
            namespace: resource.namespace.clone(),
            container: container.clone(),
        })
    };
    let item = MenuItem::new("Shell").icon(Icon::CommandLine);
    match shell_containers {
        [] => {}
        [container] => {
            let response = menu.item(item);
            queue(pending_action, &response, || shell(container));
        }
        containers => {
            menu.submenu(item, |menu: &mut Menu<'_>| {
                for container in containers {
                    let response = menu.action(&container.name);
                    queue(pending_action, &response, || shell(container));
                }
            });
        }
    }
}

/// The "Debug image" list of a debug shell submenu: the configured presets,
/// then the pod's own images, then a way to configure the presets.
fn show_debug_images(
    menu: &mut Menu<'_>,
    configured_presets: &[DebugImagePreset],
    pod_image_presets: &[DebugImagePreset],
    pending_action: &mut Option<ResourceAction>,
    action: impl Fn(&DebugImagePreset) -> ResourceAction,
) {
    menu.heading("Debug image");
    for (index, preset) in configured_presets
        .iter()
        .chain(pod_image_presets)
        .enumerate()
    {
        if index == configured_presets.len() && index > 0 {
            menu.separator();
        }
        let profile = preset.profile_label();
        let response = menu.item(MenuItem::new(&preset.name).detail(&profile));
        queue(pending_action, &response, || Some(action(preset)));
    }
    menu.separator();
    let configure = menu.item(MenuItem::new("Configure presets…").icon(Icon::Cog6Tooth));
    queue(pending_action, &configure, || {
        Some(ResourceAction::ConfigureDebugImages)
    });
}

/// Restart, roll back, scale and run, and a Node's cordon and drain: the
/// actions that change what runs.
fn show_lifecycle_actions(
    menu: &mut Menu<'_>,
    groups: &mut MenuGroups,
    api_resource: &ApiResource,
    resource: &MinimalResource,
    supports_scale: bool,
    pending_action: &mut Option<ResourceAction>,
) {
    let rollout = crate::resource_handlers::deployment::supports_rollout_restart(api_resource);
    let cron_job_namespace = crate::resource_handlers::cron_job::supports_manual_run(api_resource)
        .then(|| resource.namespace.clone())
        .flatten();
    let node = api_resource.is_node();
    if !(rollout || supports_scale || cron_job_namespace.is_some() || node) {
        return;
    }
    groups.next(menu);
    if node {
        show_node_maintenance_actions(menu, resource, pending_action);
    }
    if rollout {
        let restart = menu.item(MenuItem::new("Restart rollout").icon(Icon::ArrowPath));
        queue(pending_action, &restart, || {
            Some(ResourceAction::RequestDeploymentRestart {
                name: resource.name.clone(),
                namespace: resource.namespace.clone()?,
            })
        });
        let roll_back = menu.item(MenuItem::new("Roll back…").icon(Icon::ArrowUturnLeft));
        queue(pending_action, &roll_back, || {
            Some(ResourceAction::RequestDeploymentRollback {
                name: resource.name.clone(),
                namespace: resource.namespace.clone()?,
            })
        });
    }
    if supports_scale {
        let scale = menu.item(MenuItem::new("Scale…").icon(Icon::ArrowsUpDown));
        queue(pending_action, &scale, || {
            Some(ResourceAction::RequestScale {
                name: resource.name.clone(),
                namespace: resource.namespace.clone(),
            })
        });
    }
    if let Some(namespace) = cron_job_namespace {
        let run = menu.item(MenuItem::new("Run now").icon(Icon::Play));
        queue(pending_action, &run, || {
            Some(ResourceAction::RequestCronJobRun {
                name: resource.name.clone(),
                namespace,
            })
        });
    }
}

/// Cordon or Uncordon, whichever changes the Node, then Drain…
fn show_node_maintenance_actions(
    menu: &mut Menu<'_>,
    resource: &MinimalResource,
    pending_action: &mut Option<ResourceAction>,
) {
    let cordoned = crate::resource_handlers::node::is_cordoned(resource);
    let cordon = menu.item(if cordoned {
        MenuItem::new("Uncordon").icon(Icon::LockOpen)
    } else {
        MenuItem::new("Cordon").icon(Icon::LockClosed)
    });
    queue(pending_action, &cordon, || {
        Some(ResourceAction::RequestNodeCordon {
            name: resource.name.clone(),
            cordon: !cordoned,
        })
    });
    let drain = menu.item(MenuItem::new("Drain…").icon(Icon::ArrowDownTray));
    queue(pending_action, &drain, || {
        Some(ResourceAction::RequestNodeDrain {
            name: resource.name.clone(),
        })
    });
}

pub(super) fn log_stream_targets(
    resource: &MinimalResource,
    containers: &[PodLogContainer],
) -> Vec<crate::worker::PodLogStreamTarget> {
    let Some(namespace) = resource.namespace.as_ref() else {
        return Vec::new();
    };
    containers
        .iter()
        .map(|container| crate::worker::PodLogStreamTarget {
            namespace: namespace.clone(),
            pod_name: resource.name.clone(),
            container: container.name.clone(),
            kind: container.kind,
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use components::test_support::setup_egui;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    fn api_resource(group: &str, kind: &str, name: &str) -> ApiResource {
        ApiResource {
            group: group.into(),
            version: "v1".into(),
            kind: kind.into(),
            name: name.into(),
            namespaced: true,
        }
    }

    fn resource(name: &str) -> MinimalResource {
        MinimalResource {
            uid: format!("{name}-uid"),
            name: name.into(),
            namespace: Some("default".into()),
            creation_timestamp: None,
            controller_owner: None,
            labels: Default::default(),
            annotations: Default::default(),
            cells: Default::default(),
            log_containers: Vec::new(),
        }
    }

    /// The menu's item labels in their groups, top to bottom: a separator
    /// shows as a gap taller than an item between two items.
    fn menu_groups(
        api_resource: ApiResource,
        resource: MinimalResource,
        log_containers: Vec<PodLogContainer>,
        supports_scale: bool,
    ) -> Vec<Vec<String>> {
        let presets = vec![DebugImagePreset {
            name: "Busybox".into(),
            image: "busybox".into(),
            profile: crate::terminal_launcher::DebugProfile::General,
        }];
        let mut harness = Harness::new_ui(move |ui| {
            components::menu::preview(ui, 240.0, "Actions", |menu| {
                show_resource_action_items(
                    menu,
                    &api_resource,
                    &resource,
                    &log_containers,
                    &presets,
                    supports_scale,
                    &mut None,
                );
            });
        });
        setup_egui(&mut harness);
        harness.run();
        let mut items = harness
            .query_all_by_role(egui::accesskit::Role::MenuItem)
            .map(|node| {
                (
                    node.rect(),
                    node.accesskit_node().label().unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.0.top().total_cmp(&right.0.top()));
        let mut groups: Vec<Vec<String>> = Vec::new();
        let mut previous_bottom = None;
        for (rect, label) in items {
            let separated = previous_bottom.is_some_and(|bottom: f32| rect.top() - bottom > 1.0);
            if groups.is_empty() || separated {
                groups.push(Vec::new());
            }
            groups.last_mut().expect("a group").push(label);
            previous_bottom = Some(rect.bottom());
        }
        groups
    }

    #[test]
    fn pod_actions_group_edit_inspect_and_copy_before_delete_in_menu_order() {
        let containers = vec![
            container("api", ContainerKind::App, Some("registry.example/api:v1")),
            container("sidecar", ContainerKind::App, None),
        ];
        assert_eq!(
            menu_groups(
                api_resource("core", "Pod", "pods"),
                resource("api-0"),
                containers,
                false,
            ),
            [
                vec![
                    "Edit YAML",
                    "View logs",
                    "Shell",
                    "Debug shell",
                    "Copy name"
                ],
                vec!["Delete"],
            ]
        );
    }

    #[test]
    fn deployment_lifecycle_actions_form_their_own_group_before_delete() {
        assert_eq!(
            menu_groups(
                api_resource("apps", "Deployment", "deployments"),
                resource("api"),
                Vec::new(),
                true,
            ),
            [
                vec!["Edit YAML", "Copy name"],
                vec!["Restart rollout", "Roll back…", "Scale…"],
                vec!["Delete"],
            ]
        );
    }

    #[test]
    fn node_maintenance_forms_the_lifecycle_group_and_toggles_cordon() {
        let node = ApiResource {
            namespaced: false,
            ..api_resource("core", "Node", "nodes")
        };
        let mut schedulable = resource("worker");
        schedulable.namespace = None;
        let mut cordoned = schedulable.clone();
        crate::resource_handlers::node::mark_cordoned(&mut cordoned);
        assert_eq!(
            menu_groups(node.clone(), schedulable, Vec::new(), false),
            [
                vec!["Edit YAML", "Shell", "Copy name"],
                vec!["Cordon", "Drain…"],
                vec!["Delete"],
            ]
        );
        assert_eq!(
            menu_groups(node, cordoned, Vec::new(), false)[1],
            ["Uncordon", "Drain…"]
        );
    }

    #[test]
    fn force_delete_follows_delete_in_the_destructive_group() {
        let terminating =
            resource("stuck").with_lifecycle_metadata(true, vec!["example.com/cleanup".into()]);
        assert_eq!(
            menu_groups(
                api_resource("", "ConfigMap", "configmaps"),
                terminating,
                Vec::new(),
                false,
            )
            .last()
            .expect("a destructive group"),
            &vec!["Delete", "Force delete (remove finalizers)"]
        );
    }

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
