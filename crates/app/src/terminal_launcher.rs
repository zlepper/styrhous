use serde::{Deserialize, Serialize};
use std::process::Command;

/// Everything required to start an interactive shell in a Pod from a local terminal.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PodShellRequest {
    pub(crate) kube_context: String,
    pub(crate) namespace: String,
    pub(crate) pod_name: String,
    pub(crate) container: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub(crate) struct TerminalLaunchSettings {
    /// `None` delegates to the platform default; a custom value must contain
    /// exactly one `{command}` placeholder.
    pub(crate) custom_template: Option<String>,
}

impl TerminalLaunchSettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let Some(template) = &self.custom_template else {
            return Ok(());
        };
        let placeholders = template.match_indices("{command}").count();
        if placeholders == 1 {
            Ok(())
        } else {
            Err("The launcher template must contain exactly one {command} placeholder.".into())
        }
    }
}

pub(crate) trait TerminalLauncher: Default {
    fn launch(
        &mut self,
        request: &PodShellRequest,
        settings: &TerminalLaunchSettings,
    ) -> Result<(), String>;
}

#[derive(Default)]
pub(crate) struct SystemTerminalLauncher;

impl TerminalLauncher for SystemTerminalLauncher {
    fn launch(
        &mut self,
        request: &PodShellRequest,
        settings: &TerminalLaunchSettings,
    ) -> Result<(), String> {
        let plan = LaunchPlan::for_current_platform(request, settings)?;
        Command::new(&plan.program)
            .args(&plan.args)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Unable to start {}: {error}", plan.program))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct LaunchPlan {
    program: String,
    args: Vec<String>,
}

impl LaunchPlan {
    fn for_current_platform(
        request: &PodShellRequest,
        settings: &TerminalLaunchSettings,
    ) -> Result<Self, String> {
        settings.validate()?;
        let kubectl = kubectl_arguments(request);
        if let Some(template) = &settings.custom_template {
            return Ok(custom_template_plan(template, &kubectl));
        }

        #[cfg(target_os = "linux")]
        {
            let mut args = vec![format!("--title=Shell: {}", request.pod_name), "--".into()];
            args.extend(kubectl);
            return Ok(Self {
                program: "xdg-terminal-exec".into(),
                args,
            });
        }
        #[cfg(target_os = "macos")]
        {
            let command = shell_command(&kubectl, ShellDialect::Posix);
            return Ok(Self {
                program: "osascript".into(),
                args: vec![
                    "-e".into(),
                    "on run argv\ntell application \"Terminal\"\ndo script (item 1 of argv)\nactivate\nend tell\nend run".into(),
                    command,
                ],
            });
        }
        #[cfg(target_os = "windows")]
        {
            let mut args = vec!["-w".into(), "new".into()];
            args.extend(kubectl);
            return Ok(Self {
                program: "wt.exe".into(),
                args,
            });
        }
        #[allow(unreachable_code)]
        Err("Opening an external terminal is not supported on this operating system.".into())
    }
}

fn kubectl_arguments(request: &PodShellRequest) -> Vec<String> {
    vec![
        "kubectl".into(),
        "--context".into(),
        request.kube_context.clone(),
        "--namespace".into(),
        request.namespace.clone(),
        "exec".into(),
        "--stdin".into(),
        "--tty".into(),
        request.pod_name.clone(),
        "--container".into(),
        request.container.clone(),
        "--".into(),
        "sh".into(),
    ]
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
        pub(crate) requests: Vec<PodShellRequest>,
        pub(crate) failure: Option<String>,
    }

    impl TerminalLauncher for MockTerminalLauncher {
        fn launch(
            &mut self,
            request: &PodShellRequest,
            _settings: &TerminalLaunchSettings,
        ) -> Result<(), String> {
            self.requests.push(request.clone());
            self.failure.clone().map_or(Ok(()), Err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> PodShellRequest {
        PodShellRequest {
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
    fn custom_template_requires_exactly_one_command_placeholder() {
        assert!(
            TerminalLaunchSettings {
                custom_template: Some("alacritty -e {command}".into())
            }
            .validate()
            .is_ok()
        );
        assert!(
            TerminalLaunchSettings {
                custom_template: Some("alacritty".into())
            }
            .validate()
            .is_err()
        );
        assert!(
            TerminalLaunchSettings {
                custom_template: Some("{command} {command}".into())
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn custom_template_keeps_dynamic_values_shell_quoted() {
        let command = shell_command(&kubectl_arguments(&request()), ShellDialect::Posix);
        assert!(command.contains("'team dev'"));
        assert!(command.contains("'kubectl'"));
    }
}
