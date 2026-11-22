//! Streams a log window's spooled records to a file without loading them into memory.

use super::runner::{LogStoreResultSender, send_result};
use super::storage::LogicalReader;
use super::*;
use crate::ansi::split_kubernetes_timestamp;
use anyhow::Context;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// A snapshot of a window's records and how to write them.
pub(crate) struct LogExport {
    pub(super) reader: LogicalReader,
    /// Records appended after the export started are not included.
    pub(super) total_lines: usize,
    pub(super) matcher: Option<Regex>,
    pub(super) format: LogExportFormat,
}

impl LogExport {
    /// Writes the export on its own thread and always reports how it ended.
    pub(super) fn spawn(
        self,
        window_id: u64,
        export_id: u64,
        path: PathBuf,
        result_sender: LogStoreResultSender,
    ) {
        let thread_path = path.clone();
        let thread_sender = result_sender.clone();
        let spawned = thread::Builder::new()
            .name("pod-log-export".to_owned())
            .spawn(move || {
                // A panic must not leave the window waiting for a result.
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.write_to(&thread_path)
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("The export stopped unexpectedly")));
                send_result(
                    &thread_sender,
                    LogStoreResult::export_outcome(window_id, export_id, thread_path, outcome),
                );
            });
        if let Err(error) = spawned {
            send_result(
                &result_sender,
                LogStoreResult::export_outcome(
                    window_id,
                    export_id,
                    path,
                    Err(anyhow::Error::from(error).context("Could not start the export")),
                ),
            );
        }
    }

    /// Writes the matching records to `path`, returning how many were written.
    ///
    /// The file is written next to its destination and moved into place only
    /// when complete, so a failure never leaves a truncated export behind. It
    /// replaces an existing file and is created with the user's usual mode.
    fn write_to(mut self, path: &Path) -> anyhow::Result<usize> {
        let directory = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let builder = tempfile::Builder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::PermissionsExt;
            let mut builder = builder;
            // Masked by the umask like any newly created file, unlike
            // tempfile's private default.
            builder.permissions(std::fs::Permissions::from_mode(0o666));
            builder
        };
        let file = builder
            .tempfile_in(directory)
            .with_context(|| format!("Could not write to {}", directory.display()))?;
        let mut writer = BufWriter::new(file);
        let mut lines = 0;
        for line_index in 0..self.total_lines {
            let record = self.reader.read_record(line_index)?;
            if self.write_record(&mut writer, &record)? {
                lines += 1;
            }
        }
        writer
            .into_inner()
            .map_err(|error| error.into_error())?
            .persist(path)
            .with_context(|| format!("Could not save {}", path.display()))?;
        Ok(lines)
    }

    /// Writes `record` as one line, unless the filter excludes it.
    fn write_record(&self, writer: &mut impl Write, record: &LogRecord) -> anyhow::Result<bool> {
        let (timestamp, message) = split_kubernetes_timestamp(&record.text);
        let stripped = (!self.format.keep_ansi || self.matcher.is_some())
            .then(|| searchable_message(&record.text));
        if let (Some(matcher), Some(stripped)) = (&self.matcher, &stripped)
            && !matcher.is_match(stripped)
        {
            return Ok(false);
        }
        if self.format.include_timestamps
            && let Some(timestamp) = timestamp
        {
            write!(writer, "{timestamp} ")?;
        }
        if self.format.include_sources
            && let Some(source) = &record.source
        {
            write!(writer, "[{source}] ")?;
        }
        match stripped {
            Some(stripped) if !self.format.keep_ansi => writer.write_all(stripped.as_bytes())?,
            _ => writer.write_all(message.as_bytes())?,
        }
        writer.write_all(b"\n")?;
        Ok(true)
    }
}
