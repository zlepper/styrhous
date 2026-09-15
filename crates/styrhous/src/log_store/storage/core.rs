use super::super::*;
use super::io::{read_record_from, write_record};
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use tempfile::NamedTempFile;

pub(crate) struct LogStore {
    pub(crate) data: Option<NamedTempFile>,
    pub(crate) offsets: Option<NamedTempFile>,
    pub(crate) backfill: Option<BackfillStore>,
    pub(crate) rebase: Option<LogRebase>,
    pub(crate) total_lines: usize,
    pub(crate) search: Option<SearchState>,
    pub(crate) initialization_error: Option<String>,
}

/// The completed historical response stays in a separate pair of spool files.
/// A rebase joins it with the live segment logically, without copying either
/// log body through memory or rewriting the live spool.
pub(crate) struct BackfillStore {
    pub(crate) data: NamedTempFile,
    pub(crate) offsets: NamedTempFile,
    pub(crate) total_lines: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct LogRebase {
    pub(crate) history_lines: usize,
    pub(crate) live_start: usize,
}

pub(crate) struct LogicalReader {
    pub(crate) live_data: File,
    pub(crate) live_offsets: File,
    pub(crate) backfill: Option<(File, File)>,
    pub(crate) rebase: Option<LogRebase>,
}

impl LogicalReader {
    pub(crate) fn read_record(&mut self, line_index: usize) -> anyhow::Result<LogRecord> {
        if let Some(rebase) = self.rebase {
            if line_index < rebase.history_lines {
                let (data, offsets) = self
                    .backfill
                    .as_mut()
                    .expect("rebased reader retains a history segment");
                return read_record_from(data, offsets, line_index);
            }
            return read_record_from(
                &mut self.live_data,
                &mut self.live_offsets,
                line_index - rebase.history_lines + rebase.live_start,
            );
        }
        read_record_from(&mut self.live_data, &mut self.live_offsets, line_index)
    }
}

pub(crate) struct SearchState {
    pub(crate) generation: u64,
    pub(crate) matcher: Regex,
    pub(crate) cancellation: Arc<AtomicBool>,
    pub(crate) match_offsets: Option<NamedTempFile>,
    pub(crate) match_count: usize,
    pub(crate) complete: bool,
}

pub(crate) struct AppendSummary {
    pub(crate) total_lines: usize,
    pub(crate) completed_search: Option<(u64, usize)>,
    pub(crate) appended_rows: Vec<LogPageRow>,
}

impl BackfillStore {
    pub(crate) fn new() -> anyhow::Result<Self> {
        Ok(Self {
            data: NamedTempFile::new()?,
            offsets: NamedTempFile::new()?,
            total_lines: 0,
        })
    }

    pub(crate) fn append(&mut self, records: Vec<LogRecord>) -> anyhow::Result<()> {
        let appended_lines = records.len();
        let mut data = self.data.reopen()?;
        let mut offsets = self.offsets.reopen()?;
        let mut line_offsets = Vec::with_capacity(records.len());
        for record in records {
            line_offsets.push(write_record(&mut data, &record)?);
        }
        data.flush()?;
        offsets.seek(SeekFrom::End(0))?;
        for offset in line_offsets {
            offsets.write_all(&offset.to_le_bytes())?;
        }
        offsets.flush()?;
        self.total_lines += appended_lines;
        Ok(())
    }

    pub(crate) fn read_record(&self, line_index: usize) -> anyhow::Result<LogRecord> {
        let mut data = self.data.reopen()?;
        let mut offsets = self.offsets.reopen()?;
        read_record_from(&mut data, &mut offsets, line_index)
    }
}
