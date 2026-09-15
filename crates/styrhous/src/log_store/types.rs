use crate::ansi::AnsiStyleSpan;
use std::io::{Read, Write};

pub(crate) const LOG_PAGE_SIZE: usize = 256;

/// One Kubernetes log record and the source that produced it.
///
/// Source identity stays separate from the message so rendering can show or
/// hide it without changing search, selection, or copied text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogRecord {
    pub(crate) text: String,
    pub(crate) source: Option<String>,
}

impl LogRecord {
    pub(crate) fn from_source(text: String, source: String) -> Self {
        Self {
            text,
            source: Some(source),
        }
    }
}

impl From<String> for LogRecord {
    fn from(text: String) -> Self {
        Self { text, source: None }
    }
}

impl From<&str> for LogRecord {
    fn from(text: &str) -> Self {
        Self::from(text.to_owned())
    }
}

pub(crate) fn write_log_record(writer: &mut impl Write, record: &LogRecord) -> anyhow::Result<u64> {
    let source_length = record.source.as_ref().map_or(Ok(u32::MAX), |source| {
        u32::try_from(source.len()).map_err(|_| anyhow::anyhow!("A log source label exceeds 4 GiB"))
    })?;
    if source_length == u32::MAX && record.source.is_some() {
        anyhow::bail!("A log source label exceeds 4 GiB");
    }
    let text_length = u32::try_from(record.text.len())
        .map_err(|_| anyhow::anyhow!("A log line exceeds 4 GiB"))?;

    writer.write_all(&source_length.to_le_bytes())?;
    if let Some(source) = &record.source {
        writer.write_all(source.as_bytes())?;
    }
    writer.write_all(&text_length.to_le_bytes())?;
    writer.write_all(record.text.as_bytes())?;

    Ok(8 + record
        .source
        .as_ref()
        .map_or(0, |_| u64::from(source_length))
        + u64::from(text_length))
}

/// Reads a complete record, returning `None` only when the reader is already
/// at its end before the next record begins.
pub(crate) fn read_log_record(reader: &mut impl Read) -> anyhow::Result<Option<LogRecord>> {
    let mut source_length = [0_u8; 4];
    let read = reader.read(&mut source_length)?;
    if read == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut source_length[read..])?;
    let source_length = u32::from_le_bytes(source_length);
    let source = if source_length == u32::MAX {
        None
    } else {
        let mut bytes = vec![0; source_length as usize];
        reader.read_exact(&mut bytes)?;
        Some(String::from_utf8(bytes)?)
    };
    let mut text_length = [0_u8; 4];
    reader.read_exact(&mut text_length)?;
    let mut text = vec![0; u32::from_le_bytes(text_length) as usize];
    reader.read_exact(&mut text)?;
    Ok(Some(LogRecord {
        text: String::from_utf8(text)?,
        source,
    }))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LogStoreConfig {
    pub(crate) page_size: usize,
    pub(crate) command_channel_capacity: usize,
    pub(crate) result_channel_capacity: usize,
    pub(crate) search_progress_interval: usize,
}

impl Default for LogStoreConfig {
    fn default() -> Self {
        Self {
            page_size: LOG_PAGE_SIZE,
            command_channel_capacity: 128,
            result_channel_capacity: 128,
            search_progress_interval: 2048,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogPageRow {
    pub(crate) display_row: usize,
    pub(crate) line_index: usize,
    pub(crate) timestamp: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) text: String,
    pub(crate) style_spans: Vec<AnsiStyleSpan>,
    pub(crate) match_ranges: Vec<(usize, usize)>,
}

#[derive(Debug)]
pub(crate) enum LogStoreResult {
    Updated {
        window_id: u64,
        total_lines: usize,
        completed_search: Option<(u64, usize)>,
        /// Parsed rows from the live tail. The UI can draw these immediately
        /// while a normal page request catches up from the disk spool.
        appended_rows: Vec<LogPageRow>,
        /// Number of older records written to the history spool so far.
        backfill_lines: Option<usize>,
    },
    Rebased {
        window_id: u64,
        total_lines: usize,
        /// Maps a row from the pre-rebase live segment into the combined
        /// logical index, keeping the visible log record in place.
        live_start: usize,
        /// Number of records supplied by the completed history request.
        history_lines: usize,
    },
    SearchProgress {
        window_id: u64,
        generation: u64,
        scanned_lines: usize,
        total_lines: usize,
        match_count: usize,
    },
    SearchCompleted {
        window_id: u64,
        generation: u64,
        match_count: usize,
    },
    PageLoaded {
        window_id: u64,
        generation: u64,
        filter_matches: bool,
        page_start: usize,
        total_rows: usize,
        rows: Vec<LogPageRow>,
    },
    MatchResolved {
        window_id: u64,
        generation: u64,
        match_row: usize,
        line_index: usize,
    },
    Copied {
        window_id: u64,
        selection_generation: u64,
        text: String,
    },
    Failed {
        window_id: u64,
        error: String,
    },
}
