use super::*;

fn request() -> ShellRequest {
    ShellRequest::Pod {
        kube_context: "team dev".into(),
        namespace: "default".into(),
        pod_name: "api".into(),
        container: "server".into(),
    }
}

#[test]
fn kubectl_command_targets_the_selected_pod_container_and_context() {
    assert_eq!(
        kubectl_arguments(&request()),
        [
            "kubectl",
            "--context",
            "team dev",
            "--namespace",
            "default",
            "exec",
            "--stdin",
            "--tty",
            "api",
            "--container",
            "server",
            "--",
            "sh",
        ]
    );
}

#[test]
fn kubectl_command_starts_a_node_debug_shell_with_the_selected_preset() {
    let request = ShellRequest::Node {
        kube_context: "team dev".into(),
        node_name: "worker-1".into(),
        preset: DebugImagePreset {
            name: "Network tools".into(),
            image: "nicolaka/netshoot".into(),
            profile: DebugProfile::Netadmin,
        },
    };

    assert_eq!(
        kubectl_arguments(&request),
        [
            "kubectl",
            "--context",
            "team dev",
            "debug",
            "node/worker-1",
            "--stdin",
            "--tty",
            "--image",
            "nicolaka/netshoot",
            "--profile",
            "netadmin",
        ]
    );
}

#[test]
fn kubectl_command_starts_a_pod_debug_shell_with_the_selected_target_and_image() {
    let request = ShellRequest::PodDebug {
        kube_context: "team dev".into(),
        namespace: "default".into(),
        pod_name: "api".into(),
        target_container: "server".into(),
        preset: DebugImagePreset {
            name: "Network tools".into(),
            image: "nicolaka/netshoot".into(),
            profile: DebugProfile::Netadmin,
        },
    };

    assert_eq!(
        kubectl_arguments(&request),
        [
            "kubectl",
            "--context",
            "team dev",
            "--namespace",
            "default",
            "debug",
            "api",
            "--stdin",
            "--tty",
            "--image",
            "nicolaka/netshoot",
            "--profile",
            "netadmin",
            "--target",
            "server",
        ]
    );
}

#[test]
fn pod_debug_shell_uses_the_general_profile_for_a_pod_declared_image() {
    let request = ShellRequest::PodDebug {
        kube_context: "team dev".into(),
        namespace: "default".into(),
        pod_name: "api".into(),
        target_container: "server".into(),
        preset: DebugImagePreset {
            name: "example/api:v1".into(),
            image: "example/api:v1".into(),
            profile: DebugProfile::General,
        },
    };

    assert_eq!(
        kubectl_arguments(&request),
        [
            "kubectl",
            "--context",
            "team dev",
            "--namespace",
            "default",
            "debug",
            "api",
            "--stdin",
            "--tty",
            "--image",
            "example/api:v1",
            "--profile",
            "general",
            "--target",
            "server",
        ]
    );
}

#[test]
fn custom_template_requires_exactly_one_command_placeholder() {
    assert!(
        TerminalLaunchSettings {
            custom_template: Some("alacritty -e {command}".into()),
            ..Default::default()
        }
        .validate()
        .is_ok()
    );
    assert!(
        TerminalLaunchSettings {
            custom_template: Some("alacritty".into()),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        TerminalLaunchSettings {
            custom_template: Some("{command} {command}".into()),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn debug_image_presets_require_unique_names_and_images() {
    let mut settings = TerminalLaunchSettings::default();
    settings.debug_image_presets.push(DebugImagePreset {
        name: " busybox ".into(),
        image: "other".into(),
        profile: DebugProfile::General,
    });
    assert_eq!(
        settings.validate(),
        Err("Debug image names must be unique.".into())
    );

    settings.debug_image_presets.pop();
    settings.debug_image_presets[0].image.clear();
    assert_eq!(
        settings.validate(),
        Err("Each debug image must have a name and image.".into())
    );
}

#[test]
fn debug_image_preset_images_reject_surrounding_whitespace() {
    let settings = TerminalLaunchSettings {
        custom_template: None,
        debug_image_presets: vec![DebugImagePreset {
            name: "Ubuntu".into(),
            image: " ubuntu ".into(),
            profile: DebugProfile::General,
        }],
    };

    assert_eq!(
        settings.validate(),
        Err("Debug images cannot start or end with whitespace.".into())
    );
}

#[test]
fn saved_settings_without_debug_image_presets_receive_the_defaults() {
    let settings = serde_yaml::from_str::<TerminalLaunchSettings>(
        "custom_template: 'alacritty -e {command}'\n",
    )
    .expect("legacy settings deserialize");

    assert_eq!(settings.debug_image_presets, default_debug_image_presets());
}

#[test]
fn saved_empty_debug_image_preset_list_stays_empty() {
    let settings = serde_yaml::from_str::<TerminalLaunchSettings>(
        "custom_template: null\ndebug_image_presets: []\n",
    )
    .expect("settings deserialize");

    assert!(settings.debug_image_presets.is_empty());
}

#[test]
fn custom_template_keeps_dynamic_values_shell_quoted() {
    let command = shell_command(&kubectl_arguments(&request()), ShellDialect::Posix);
    assert!(command.contains("'team dev'"));
    assert!(command.contains("'kubectl'"));
}

#[test]
fn custom_template_bypasses_automatic_launcher_candidates() {
    let plans = LaunchPlan::for_current_platform(
        &request(),
        &TerminalLaunchSettings {
            custom_template: Some("alacritty -e {command}".into()),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(plans.len(), 1);
    #[cfg(windows)]
    assert_eq!(plans[0].program, "cmd.exe");
    #[cfg(not(windows))]
    assert_eq!(plans[0].program, "/bin/sh");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_automatic_launcher_prefers_terminal_environment_then_desktop_fallbacks() {
    let kubectl = kubectl_arguments(&request());
    let plans = linux_launch_plans(
        &request(),
        &kubectl,
        Some("alacritty --working-directory ~"),
    );

    assert_eq!(
        plans
            .iter()
            .map(|plan| plan.program.as_str())
            .collect::<Vec<_>>(),
        [
            "/bin/sh",
            "xdg-terminal-exec",
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal",
            "xterm",
        ]
    );
    assert_eq!(plans[0].args[0], "-lc");
    assert!(plans[0].args[1].starts_with("exec alacritty --working-directory ~ -e "));
    assert!(plans[0].args[1].contains("'team dev'"));
    assert_eq!(plans[1].args[0], "--title=Shell: api");
    assert_eq!(plans[1].args[1], "--");
    assert_eq!(plans[2].args[0], "-e");
    assert_eq!(plans[3].args[0], "--title=Shell: api");
    assert_eq!(
        plans[4].args[..4],
        ["--new-tab", "-p", "tabtitle=Shell: api", "-e"]
    );
    assert_eq!(plans[5].args[0], "--title=Shell: api");
    assert!(plans[5].args[1].starts_with("--command='kubectl'"));
    assert_eq!(plans[6].args[..3], ["-T", "Shell: api", "-e"]);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_automatic_launcher_starts_with_xdg_when_terminal_environment_is_missing() {
    let kubectl = kubectl_arguments(&request());
    let plans = linux_launch_plans(&request(), &kubectl, None);

    assert_eq!(plans[0].program, "xdg-terminal-exec");
}
