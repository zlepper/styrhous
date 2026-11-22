//! Runs locally installed command-line tools (provider CLIs, `helm`) from the worker.

use anyhow::{Context, Result, bail};
use std::time::Duration;
use tokio::process::Command;

pub(crate) const MAX_STDERR_LENGTH: usize = 2_000;

/// One invocation of a local program.
#[derive(Debug, Clone)]
pub(crate) struct CliCommand {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    /// Variables added to the inherited environment.
    pub(crate) env: Vec<(String, String)>,
    pub(crate) timeout: Duration,
}

impl CliCommand {
    pub(crate) fn new(
        program: &str,
        args: impl IntoIterator<Item = impl Into<String>>,
        timeout: Duration,
    ) -> Self {
        Self {
            program: program.to_owned(),
            args: args.into_iter().map(Into::into).collect(),
            env: Vec::new(),
            timeout,
        }
    }

    pub(crate) fn env(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.to_owned(), value.to_owned()));
        self
    }
}

#[derive(Debug)]
pub(crate) struct CliOutput {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: String,
    pub(crate) success: bool,
}

/// The program is not installed or not on `PATH`, so callers can show
/// installation guidance instead of a raw spawn error.
#[derive(Debug)]
pub(crate) struct CliNotInstalled {
    pub(crate) program: String,
}

impl std::fmt::Display for CliNotInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not installed or not on PATH", self.program)
    }
}

impl std::error::Error for CliNotInstalled {}

/// The program ran longer than its timeout and was killed.
#[derive(Debug)]
pub(crate) struct CliTimedOut {
    pub(crate) program: String,
    pub(crate) timeout: Duration,
}

impl std::fmt::Display for CliTimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} timed out after {:?}", self.program, self.timeout)
    }
}

impl std::error::Error for CliTimedOut {}

/// Whether `error` (or one of its causes) is a [`CliNotInstalled`].
pub(crate) fn is_cli_not_installed(error: &anyhow::Error) -> bool {
    error.downcast_ref::<CliNotInstalled>().is_some()
}

/// Runs the command, killing it when it exceeds its timeout.
pub(crate) async fn run_cli(command: &CliCommand) -> Result<CliOutput> {
    let mut process = Command::new(&command.program);
    process
        .args(&command.args)
        .envs(command.env.iter().map(|(name, value)| (name, value)))
        .kill_on_drop(true);
    let output = tokio::time::timeout(command.timeout, process.output())
        .await
        .map_err(|_| CliTimedOut {
            program: command.program.clone(),
            timeout: command.timeout,
        })?;
    let output = match output {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(CliNotInstalled {
                program: command.program.clone(),
            }
            .into());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("Could not start {}", command.program));
        }
    };
    Ok(CliOutput {
        stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        success: output.status.success(),
    })
}

/// Turns a non-zero exit into an error naming the command and its (bounded) stderr.
pub(crate) fn ensure_success(
    program: &str,
    args: &[String],
    output: CliOutput,
) -> Result<CliOutput> {
    if !output.success {
        let stderr = stderr_tail(output.stderr.trim(), MAX_STDERR_LENGTH);
        bail!(
            "{program} {} failed{}",
            args.join(" "),
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }
    Ok(output)
}

/// The last `max_bytes` of `text`: tools print warnings first and their actual
/// error last, so the end is the part worth keeping.
fn stderr_tail(text: &str, max_bytes: usize) -> &str {
    &text[text.ceil_char_boundary(text.len().saturating_sub(max_bytes))..]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(stderr: &str) -> CliOutput {
        CliOutput {
            stdout: Vec::new(),
            stderr: stderr.to_owned(),
            success: false,
        }
    }

    #[tokio::test]
    async fn a_missing_program_is_reported_as_not_installed() {
        let error = run_cli(&CliCommand::new(
            "styrhous-definitely-not-installed",
            Vec::<String>::new(),
            Duration::from_secs(5),
        ))
        .await
        .unwrap_err();

        assert!(is_cli_not_installed(&error));
        assert_eq!(
            error.to_string(),
            "styrhous-definitely-not-installed is not installed or not on PATH"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn added_environment_reaches_the_program_and_failures_include_stderr() {
        let command = CliCommand::new(
            "sh",
            [
                "-c",
                "printf \"$STYRHOUS_CLI_TEST\"; echo broken >&2; exit 3",
            ],
            Duration::from_secs(5),
        )
        .env("STYRHOUS_CLI_TEST", "from-env");
        let output = run_cli(&command).await.unwrap();
        assert_eq!(output.stdout, b"from-env");

        let error = ensure_success(&command.program, &command.args, output).unwrap_err();
        assert!(error.to_string().ends_with("failed: broken"), "{error}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_command_exceeding_its_timeout_is_killed_and_reported() {
        let started = std::time::Instant::now();
        let error = run_cli(&CliCommand::new(
            "sh",
            ["-c", "sleep 10"],
            Duration::from_millis(100),
        ))
        .await
        .unwrap_err();

        assert_eq!(error.to_string(), "sh timed out after 100ms");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn failures_keep_the_end_of_long_stderr_and_omit_empty_stderr() {
        let args = ["rollback".to_owned()];
        let stderr = format!("{}\nError: release not found", "W warning\n".repeat(400));
        let error = ensure_success("helm", &args, output(&stderr)).unwrap_err();
        assert!(
            error.to_string().ends_with("Error: release not found"),
            "{error}"
        );
        assert!(error.to_string().len() < MAX_STDERR_LENGTH + 100);

        let error = ensure_success("helm", &args, output("  ")).unwrap_err();
        assert_eq!(error.to_string(), "helm rollback failed");
    }

    #[test]
    fn stderr_tail_respects_character_boundaries() {
        assert_eq!(stderr_tail("aé", 1), "");
        assert_eq!(stderr_tail("aé", 2), "é");
        assert_eq!(stderr_tail("short", 100), "short");
    }
}
