use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::process::Command;

/// Everything required to start an interactive shell from a local terminal.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum ShellRequest {
    Pod {
        kube_context: String,
        namespace: String,
        pod_name: String,
        container: String,
    },
    PodDebug {
        kube_context: String,
        namespace: String,
        pod_name: String,
        target_container: String,
        preset: DebugImagePreset,
    },
    Node {
        kube_context: String,
        node_name: String,
        preset: DebugImagePreset,
    },
}

impl ShellRequest {
    fn title_target(&self) -> &str {
        match self {
            Self::Pod { pod_name, .. } | Self::PodDebug { pod_name, .. } => pod_name,
            Self::Node { node_name, .. } => node_name,
        }
    }
}

/// A named `kubectl debug` image and security profile choice.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct DebugImagePreset {
    pub(crate) name: String,
    pub(crate) image: String,
    pub(crate) profile: DebugProfile,
}

impl DebugImagePreset {
    pub(crate) fn menu_label(&self) -> String {
        format!("{} — {}", self.name, self.profile.label())
    }
}

/// The stable `kubectl debug --profile` options exposed by the UI.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DebugProfile {
    General,
    Baseline,
    Restricted,
    Netadmin,
    Sysadmin,
}

impl DebugProfile {
    pub(crate) const ALL: [Self; 5] = [
        Self::General,
        Self::Baseline,
        Self::Restricted,
        Self::Netadmin,
        Self::Sysadmin,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Baseline => "Baseline",
            Self::Restricted => "Restricted",
            Self::Netadmin => "Network admin",
            Self::Sysadmin => "System admin",
        }
    }

    fn kubectl_value(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Baseline => "baseline",
            Self::Restricted => "restricted",
            Self::Netadmin => "netadmin",
            Self::Sysadmin => "sysadmin",
        }
    }
}

pub(crate) fn default_debug_image_presets() -> Vec<DebugImagePreset> {
    vec![
        DebugImagePreset {
            name: "Busybox".into(),
            image: "busybox".into(),
            profile: DebugProfile::General,
        },
        DebugImagePreset {
            name: "Ubuntu".into(),
            image: "ubuntu".into(),
            profile: DebugProfile::General,
        },
        DebugImagePreset {
            name: "Netshoot".into(),
            image: "nicolaka/netshoot".into(),
            profile: DebugProfile::Netadmin,
        },
    ]
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct TerminalLaunchSettings {
    /// `None` delegates to the platform default; a custom value must contain
    /// exactly one `{command}` placeholder.
    pub(crate) custom_template: Option<String>,
    #[serde(default = "default_debug_image_presets")]
    pub(crate) debug_image_presets: Vec<DebugImagePreset>,
}

impl Default for TerminalLaunchSettings {
    fn default() -> Self {
        Self {
            custom_template: None,
            debug_image_presets: default_debug_image_presets(),
        }
    }
}

impl TerminalLaunchSettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(template) = &self.custom_template {
            let placeholders = template.match_indices("{command}").count();
            if placeholders != 1 {
                return Err(
                    "The launcher template must contain exactly one {command} placeholder.".into(),
                );
            }
        }
        let mut names = std::collections::HashSet::new();
        for preset in &self.debug_image_presets {
            let name = preset.name.trim();
            if name.is_empty() || preset.image.trim().is_empty() {
                return Err("Each debug image must have a name and image.".into());
            }
            if preset.image != preset.image.trim() {
                return Err("Debug images cannot start or end with whitespace.".into());
            }
            if !names.insert(name.to_lowercase()) {
                return Err("Debug image names must be unique.".into());
            }
        }
        Ok(())
    }
}

pub(crate) trait TerminalLauncher: Default {
    fn launch(
        &mut self,
        request: &ShellRequest,
        settings: &TerminalLaunchSettings,
    ) -> Result<(), String>;
}

#[derive(Default)]
pub(crate) struct SystemTerminalLauncher;

impl TerminalLauncher for SystemTerminalLauncher {
    fn launch(
        &mut self,
        request: &ShellRequest,
        settings: &TerminalLaunchSettings,
    ) -> Result<(), String> {
        let plans = LaunchPlan::for_current_platform(request, settings)?;
        let mut unavailable = Vec::new();

        for plan in plans {
            match Command::new(&plan.program).args(&plan.args).spawn() {
                Ok(_) => return Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    unavailable.push(format!("{} ({error})", plan.program));
                }
                Err(error) => return Err(format!("Unable to start {}: {error}", plan.program)),
            }
        }

        Err(format!(
            "No supported terminal launcher was found. Tried: {}.",
            unavailable.join(", ")
        ))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct LaunchPlan {
    program: String,
    args: Vec<String>,
}

impl LaunchPlan {
    fn for_current_platform(
        request: &ShellRequest,
        settings: &TerminalLaunchSettings,
    ) -> Result<Vec<Self>, String> {
        settings.validate()?;
        let kubectl = kubectl_arguments(request);
        if let Some(template) = &settings.custom_template {
            return Ok(vec![custom_template_plan(template, &kubectl)]);
        }

        #[cfg(target_os = "linux")]
        {
            return Ok(linux_launch_plans(
                request,
                &kubectl,
                std::env::var("TERMINAL").ok().as_deref(),
            ));
        }
        #[cfg(target_os = "macos")]
        {
            let command = shell_command(&kubectl, ShellDialect::Posix);
            return Ok(vec![Self {
                program: "osascript".into(),
                args: vec![
                    "-e".into(),
                    "on run argv\ntell application \"Terminal\"\ndo script (item 1 of argv)\nactivate\nend tell\nend run".into(),
                    command,
                ],
            }]);
        }
        #[cfg(target_os = "windows")]
        {
            let mut args = vec!["-w".into(), "new".into()];
            args.extend(kubectl);
            return Ok(vec![Self {
                program: "wt.exe".into(),
                args,
            }]);
        }
        #[allow(unreachable_code)]
        Err("Opening an external terminal is not supported on this operating system.".into())
    }
}

#[cfg(target_os = "linux")]
fn linux_launch_plans(
    request: &ShellRequest,
    kubectl: &[String],
    terminal_environment: Option<&str>,
) -> Vec<LaunchPlan> {
    let title = format!("Shell: {}", request.title_target());
    let command = shell_command(kubectl, ShellDialect::Posix);
    let mut plans = Vec::new();

    if let Some(terminal) = terminal_environment.filter(|terminal| !terminal.trim().is_empty()) {
        plans.push(LaunchPlan {
            program: "/bin/sh".into(),
            args: vec!["-lc".into(), format!("exec {terminal} -e {command}")],
        });
    }

    plans.push(LaunchPlan {
        program: "xdg-terminal-exec".into(),
        args: {
            let mut args = vec![format!("--title={title}"), "--".into()];
            args.extend(kubectl.iter().cloned());
            args
        },
    });
    plans.push(LaunchPlan {
        program: "x-terminal-emulator".into(),
        args: {
            let mut args = vec!["-e".into()];
            args.extend(kubectl.iter().cloned());
            args
        },
    });
    plans.push(LaunchPlan {
        program: "gnome-terminal".into(),
        args: {
            let mut args = vec![format!("--title={title}"), "--".into()];
            args.extend(kubectl.iter().cloned());
            args
        },
    });
    plans.push(LaunchPlan {
        program: "konsole".into(),
        args: {
            let mut args = vec![
                "--new-tab".into(),
                "-p".into(),
                format!("tabtitle={title}"),
                "-e".into(),
            ];
            args.extend(kubectl.iter().cloned());
            args
        },
    });
    plans.push(LaunchPlan {
        program: "xfce4-terminal".into(),
        args: vec![format!("--title={title}"), format!("--command={command}")],
    });
    plans.push(LaunchPlan {
        program: "xterm".into(),
        args: {
            let mut args = vec!["-T".into(), title, "-e".into()];
            args.extend(kubectl.iter().cloned());
            args
        },
    });
    plans
}

fn kubectl_arguments(request: &ShellRequest) -> Vec<String> {
    match request {
        ShellRequest::Pod {
            kube_context,
            namespace,
            pod_name,
            container,
        } => vec![
            "kubectl".into(),
            "--context".into(),
            kube_context.clone(),
            "--namespace".into(),
            namespace.clone(),
            "exec".into(),
            "--stdin".into(),
            "--tty".into(),
            pod_name.clone(),
            "--container".into(),
            container.clone(),
            "--".into(),
            "sh".into(),
        ],
        ShellRequest::PodDebug {
            kube_context,
            namespace,
            pod_name,
            target_container,
            preset,
        } => vec![
            "kubectl".into(),
            "--context".into(),
            kube_context.clone(),
            "--namespace".into(),
            namespace.clone(),
            "debug".into(),
            pod_name.clone(),
            "--stdin".into(),
            "--tty".into(),
            "--image".into(),
            preset.image.clone(),
            "--profile".into(),
            preset.profile.kubectl_value().into(),
            "--target".into(),
            target_container.clone(),
        ],
        ShellRequest::Node {
            kube_context,
            node_name,
            preset,
        } => vec![
            "kubectl".into(),
            "--context".into(),
            kube_context.clone(),
            "debug".into(),
            format!("node/{node_name}"),
            "--stdin".into(),
            "--tty".into(),
            "--image".into(),
            preset.image.clone(),
            "--profile".into(),
            preset.profile.kubectl_value().into(),
        ],
    }
}

fn custom_template_plan(template: &str, kubectl: &[String]) -> LaunchPlan {
    #[cfg(windows)]
    let dialect = ShellDialect::Windows;
    #[cfg(not(windows))]
    let dialect = ShellDialect::Posix;
    let command = shell_command(kubectl, dialect);
    let rendered = template.replacen("{command}", &command, 1);

    #[cfg(windows)]
    {
        LaunchPlan {
            program: "cmd.exe".into(),
            args: vec!["/d".into(), "/s".into(), "/c".into(), rendered],
        }
    }
    #[cfg(not(windows))]
    {
        LaunchPlan {
            program: "/bin/sh".into(),
            args: vec!["-lc".into(), rendered],
        }
    }
}

#[derive(Clone, Copy)]
enum ShellDialect {
    Posix,
    #[cfg(windows)]
    Windows,
}

fn shell_command(arguments: &[String], dialect: ShellDialect) -> String {
    arguments
        .iter()
        .map(|argument| match dialect {
            ShellDialect::Posix => format!("'{}'", argument.replace('\'', "'\\\"'\\\"'")),
            #[cfg(windows)]
            ShellDialect::Windows => windows_quote(argument),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
fn windows_quote(argument: &str) -> String {
    format!("\"{}\"", argument.replace('"', "\\\"").replace('%', "%%"))
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    #[derive(Default)]
    pub(crate) struct MockTerminalLauncher {
        pub(crate) requests: Vec<ShellRequest>,
        pub(crate) failure: Option<String>,
    }

    impl TerminalLauncher for MockTerminalLauncher {
        fn launch(
            &mut self,
            request: &ShellRequest,
            _settings: &TerminalLaunchSettings,
        ) -> Result<(), String> {
            self.requests.push(request.clone());
            self.failure.clone().map_or(Ok(()), Err)
        }
    }
}

#[cfg(test)]
mod tests;
