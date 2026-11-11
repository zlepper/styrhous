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

/// Runs `script` in `/bin/sh` and splits stdout on NUL; the script must print its
/// arguments with `printf '%s\0' ...` so they can be compared exactly.
#[cfg(unix)]
fn run_in_posix_shell(script: &str) -> Vec<String> {
    let output = std::process::Command::new("/bin/sh")
        .args(["-c", script])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "sh failed for {script:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut fields: Vec<String> = output
        .stdout
        .split(|byte| *byte == 0)
        .map(|field| String::from_utf8(field.to_vec()).unwrap())
        .collect();
    assert_eq!(fields.pop().as_deref(), Some(""));
    fields
}

#[cfg(unix)]
#[test]
fn posix_shell_command_round_trips_awkward_arguments_through_sh() {
    let arguments: Vec<String> = [
        "plain",
        "with space",
        "bob's-cluster",
        "'",
        "''",
        "trailing'",
        "'leading",
        "$HOME",
        "`echo hi`",
        "$(echo hi)",
        "say \"hi\"",
        "back\\slash",
        "back\\'slash",
        "new\nline",
        "semi; echo nope",
        "",
    ]
    .map(String::from)
    .to_vec();

    let command = shell_command(&arguments, ShellDialect::Posix);

    assert_eq!(
        run_in_posix_shell(&format!("printf '%s\\0' {command}")),
        arguments
    );
}

#[cfg(unix)]
#[test]
fn custom_template_does_not_execute_commands_embedded_in_a_context_name() {
    let directory = tempfile::tempdir().unwrap();
    let pwned = directory.path().join("pwned");
    let context = format!("x' ; touch '{}' ; '", pwned.display());
    let request = ShellRequest::Pod {
        kube_context: context.clone(),
        namespace: "default".into(),
        pod_name: "api".into(),
        container: "server".into(),
    };

    let plan = custom_template_plan("printf '%s\\0' {command}", &kubectl_arguments(&request));

    assert_eq!(plan.program, "/bin/sh");
    assert_eq!(
        run_in_posix_shell(&plan.args[1]),
        kubectl_arguments(&request)
    );
    assert!(!pwned.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_terminal_environment_launcher_keeps_quotes_in_arguments_intact() {
    let request = ShellRequest::Pod {
        kube_context: "bob's-cluster".into(),
        namespace: "default".into(),
        pod_name: "api".into(),
        container: "server".into(),
    };
    let kubectl = kubectl_arguments(&request);

    let plans = linux_launch_plans(&request, &kubectl, Some("printf '%s\\0'"));

    let script = plans[0].args[1].replacen("exec ", "", 1);
    let mut expected = vec!["-e".to_string()];
    expected.extend(kubectl);
    assert_eq!(run_in_posix_shell(&script), expected);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_xfce4_terminal_command_keeps_quotes_in_arguments_intact() {
    let request = ShellRequest::Pod {
        kube_context: "bob's-cluster".into(),
        namespace: "default".into(),
        pod_name: "api".into(),
        container: "server".into(),
    };
    let kubectl = kubectl_arguments(&request);

    let plans = linux_launch_plans(&request, &kubectl, None);

    let plan = plans
        .iter()
        .find(|plan| plan.program == "xfce4-terminal")
        .unwrap();
    let command = plan
        .args
        .iter()
        .find_map(|argument| argument.strip_prefix("--command="))
        .unwrap();
    assert_eq!(
        run_in_posix_shell(&format!("printf '%s\\0' {command}")),
        kubectl
    );
}

#[test]
fn windows_quote_applies_msvc_then_cmd_escaping() {
    let cases = [
        ("plain", r#"^"plain^""#),
        ("", r#"^"^""#),
        ("two words", r#"^"two words^""#),
        (r#"say "hi""#, r#"^"say \^"hi\^"^""#),
        (r"C:\dir\", r#"^"C:\dir\\^""#),
        (r"C:\dir with space\", r#"^"C:\dir with space\\^""#),
        (r"a\b\c", r#"^"a\b\c^""#),
        (r#"a\"b"#, r#"^"a\\\^"b^""#),
        (r#"a\\"b"#, r#"^"a\\\\\^"b^""#),
        ("100%", r#"^"100^%^""#),
        ("%PATH%", r#"^"^%PATH^%^""#),
        ("%%", r#"^"^%^%^""#),
        (r"\", r#"^"\\^""#),
        (r"\\", r#"^"\\\\^""#),
        (r"\\\", r#"^"\\\\\\^""#),
        (r#"""#, r#"^"\^"^""#),
        ("a^b", r#"^"a^^b^""#),
        ("a&b", r#"^"a^&b^""#),
        ("a|b", r#"^"a^|b^""#),
        ("a<b>c", r#"^"a^<b^>c^""#),
        ("(a)!", r#"^"^(a^)^!^""#),
    ];
    for (argument, expected) in cases {
        assert_eq!(windows_quote(argument), expected, "quoting {argument:?}");
    }
}

#[test]
fn windows_command_line_joins_quoted_arguments_inside_one_outer_quote_pair() {
    let command = shell_command(
        &["kubectl".to_string(), "team dev".to_string()],
        ShellDialect::Windows,
    );
    assert_eq!(command, r#"^"kubectl^" ^"team dev^""#);

    let plan = cmd_exe_plan(&format!("wt.exe {command}"));
    assert_eq!(plan.program, "cmd.exe");
    assert_eq!(
        plan.args,
        [
            "/d",
            "/v:off",
            "/s",
            "/c",
            r#""wt.exe ^"kubectl^" ^"team dev^"""#
        ]
    );
}

#[test]
fn start_templates_render_the_command_after_an_empty_title() {
    let command = shell_command(
        &["kubectl".to_string(), "a&b".to_string()],
        ShellDialect::Windows,
    );
    let plan = cmd_exe_plan(&format!(r#"start "" /wait {command}"#));
    assert_eq!(plan.args[4], r#""start "" /wait ^"kubectl^" ^"a^&b^"""#);
}

#[test]
fn line_breaks_in_arguments_are_rejected_for_cmd_exe() {
    assert!(reject_line_breaks(&["a b".into(), "tab\t".into()]).is_ok());
    for bad in ["one\ntwo", "one\rtwo"] {
        let error = reject_line_breaks(&["kubectl".into(), bad.into()]).unwrap_err();
        assert!(error.contains("line break"), "{error}");
    }
}

#[cfg(windows)]
mod windows_cmd_round_trip {
    use super::*;

    fn print_argv_helper() -> String {
        let deps = std::env::current_exe().expect("test executable path");
        let examples = deps
            .parent()
            .and_then(std::path::Path::parent)
            .expect("target profile directory")
            .join("examples")
            .join("print_argv.exe");
        assert!(examples.exists(), "missing helper {}", examples.display());
        examples.to_string_lossy().into_owned()
    }

    fn run_through_cmd(template: &str, arguments: &[&str]) -> Vec<String> {
        let mut kubectl = vec![print_argv_helper()];
        kubectl.extend(arguments.iter().map(|argument| argument.to_string()));
        let plan = custom_template_plan(template, &kubectl);
        assert_eq!(plan.program, "cmd.exe");
        let output = plan.command().output().expect("cmd.exe starts");
        assert!(
            output.status.success(),
            "cmd.exe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("utf-8 output")
            .lines()
            .map(str::to_owned)
            .collect()
    }

    const TRICKY: &[&str] = &[
        "plain",
        "",
        "two words",
        r#"say "hi""#,
        r#"""#,
        r"C:\dir\",
        r"C:\dir with space\",
        r"a\b\c",
        r"\",
        r"\\",
        r"\\\",
        r#"a\"b"#,
        r#"a\\"b"#,
        "100%",
        "%PATH%",
        "%%",
        "%UNDEFINED_VARIABLE_FOR_TEST%",
        "a^b",
        "^",
        "a&b&echo injected",
        "a|b",
        "a<b>c",
        "(a)!",
        "!PATH!",
        "tab\there",
        "unicode \u{e5}\u{4e16}",
        "--flag=\"quoted value\"",
    ];

    #[test]
    fn arguments_round_trip_exactly_through_cmd_exe() {
        let expected: Vec<String> = TRICKY
            .iter()
            .map(|argument| format!("{argument:?}"))
            .collect();
        assert_eq!(run_through_cmd("{command}", TRICKY), expected);
    }

    #[test]
    fn template_content_after_the_command_is_run_by_cmd_exe() {
        let output = run_through_cmd("{command} & echo done", &["a&b", r"c\"]);
        assert_eq!(
            output,
            [
                format!("{:?}", "a&b"),
                format!("{:?}", r"c\"),
                "done".to_string()
            ]
        );
    }

    #[test]
    fn long_arguments_with_escaped_characters_round_trip() {
        let long = r#"a&b%c^d"e\f|g<h>i (j)!k "#.repeat(130);
        assert!(long.len() > 3000);
        assert_eq!(
            run_through_cmd("{command}", &[&long]),
            [format!("{long:?}")]
        );
    }

    #[test]
    fn arguments_round_trip_when_the_template_has_its_own_quoted_program() {
        let helper = print_argv_helper();
        let template = format!(r#""{helper}" first {{command}}"#);
        let output = run_through_cmd(&template, &["a&b", r"c\"]);
        let helper_debug = format!("{helper:?}");
        assert_eq!(
            output,
            [
                r#""first""#.to_string(),
                format!("{helper_debug}"),
                r#""a&b""#.to_string(),
                format!("{:?}", r"c\"),
            ]
        );
    }
}
