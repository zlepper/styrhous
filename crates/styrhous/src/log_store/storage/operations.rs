use super::super::scan::{SearchScan, scan_records};
use super::super::*;
use super::core::{BackfillStore, LogRebase, LogStore, LogicalReader, SearchState};
use super::io::{floor_char_boundary, read_u64_at};
use crate::ansi::parse_kubernetes_log_line;
use std::io::{Seek, SeekFrom, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tempfile::NamedTempFile;

impl LogStore {
    pub(crate) fn new() -> Self {
        match (NamedTempFile::new(), NamedTempFile::new()) {
            (Ok(data), Ok(offsets)) => Self {
                data: Some(data),
                offsets: Some(offsets),
                backfill: None,
                rebase: None,
                total_lines: 0,
                search: None,
                initialization_error: None,
            },
            (data, offsets) => Self {
                data: None,
                offsets: None,
                backfill: None,
                rebase: None,
                total_lines: 0,
                search: None,
                initialization_error: Some(format!(
                    "Unable to create temporary log storage: {}",
                    data.err()
                        .or_else(|| offsets.err())
                        .expect("one tempfile creation must fail")
                )),
            },
        }
    }

    pub(crate) fn init_error(&self) -> Result<(), &str> {
        self.initialization_error.as_deref().map_or(Ok(()), Err)
    }

    pub(crate) fn data(&self) -> Result<&NamedTempFile, anyhow::Error> {
        self.data.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                self.initialization_error
                    .clone()
                    .unwrap_or_else(|| "Log store is unavailable".to_owned())
            )
        })
    }

    pub(crate) fn offsets(&self) -> Result<&NamedTempFile, anyhow::Error> {
        self.offsets.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                self.initialization_error
                    .clone()
                    .unwrap_or_else(|| "Log store is unavailable".to_owned())
            )
        })
    }

    pub(crate) fn visible_total_lines(&self) -> usize {
        self.rebase.map_or(self.total_lines, |rebase| {
            rebase.history_lines + self.total_lines.saturating_sub(rebase.live_start)
        })
    }

    pub(crate) fn append_backfill(&mut self, lines: Vec<String>) -> anyhow::Result<()> {
        if self.backfill.is_none() {
            self.backfill = Some(BackfillStore::new()?);
        }
        self.backfill
            .as_mut()
            .expect("backfill store was initialized")
            .append(lines)
    }

    pub(crate) fn logical_reader(&self) -> anyhow::Result<LogicalReader> {
        Ok(LogicalReader {
            live_data: self.data()?.reopen()?,
            live_offsets: self.offsets()?.reopen()?,
            backfill: self
                .backfill
                .as_ref()
                .map(|backfill| {
                    Ok::<_, anyhow::Error>((backfill.data.reopen()?, backfill.offsets.reopen()?))
                })
                .transpose()?,
            rebase: self.rebase,
        })
    }

    pub(crate) fn complete_backfill(&mut self) -> anyhow::Result<Option<LogRebase>> {
        let Some(backfill) = &self.backfill else {
            return Ok(None);
        };
        if backfill.total_lines == 0 {
            return Ok(None);
        }

        let overlap = self.find_backfill_overlap(backfill)?;
        let rebase = LogRebase {
            history_lines: backfill.total_lines,
            live_start: overlap,
        };
        if let Some(search) = &self.search {
            search.cancellation.store(true, Ordering::Relaxed);
        }
        self.search = None;
        self.rebase = Some(rebase);
        Ok(Some(rebase))
    }

    pub(crate) fn find_backfill_overlap(&self, backfill: &BackfillStore) -> anyhow::Result<usize> {
        // The first live records are the requested tail. Comparing ordered raw
        // records (which include Kubernetes timestamps) avoids treating
        // repeated messages with the same timestamp as an overlap.
        const MAX_OVERLAP_LINES: usize = 4 * LOG_PAGE_SIZE;
        let max_overlap = self
            .total_lines
            .min(backfill.total_lines)
            .min(MAX_OVERLAP_LINES);
        for overlap in (1..=max_overlap).rev() {
            let history_start = backfill.total_lines - overlap;
            let mut matches = true;
            for offset in 0..overlap {
                if backfill.read_line(history_start + offset)? != self.read_live_line(offset)? {
                    matches = false;
                    break;
                }
            }
            if matches {
                return Ok(overlap);
            }
        }
        // The fallback deliberately keeps both segments. It may duplicate a
        // small boundary, but it cannot discard recent live output.
        Ok(0)
    }

    pub(crate) fn append(&mut self, lines: Vec<String>) -> anyhow::Result<AppendSummary> {
        self.init_error()
            .map_err(|error| anyhow::Error::msg(error.to_owned()))?;
        let mut data = self.data()?.reopen()?;
        let mut offsets = self.offsets()?.reopen()?;
        let mut next_offset = data.seek(SeekFrom::End(0))?;
        let first_line_index = self.visible_total_lines();
        let completed_matcher = self
            .search
            .as_ref()
            .filter(|search| search.complete)
            .map(|search| search.matcher.clone());
        let mut line_offsets = Vec::with_capacity(lines.len());
        let mut matching_line_indices = Vec::new();
        let mut appended_rows = Vec::with_capacity(lines.len());

        for (relative_line_index, line) in lines.iter().enumerate() {
            let bytes = line.as_bytes();
            let length = u32::try_from(bytes.len())
                .map_err(|_| anyhow::anyhow!("A log line exceeds 4 GiB"))?;
            line_offsets.push(next_offset);
            data.write_all(&length.to_le_bytes())?;
            data.write_all(bytes)?;
            next_offset += u64::from(length) + 4;
            let visible_line = parse_kubernetes_log_line(line);
            let match_ranges = completed_matcher.as_ref().map_or_else(Vec::new, |matcher| {
                matcher
                    .find_iter(&visible_line.line.text)
                    .map(|matched| (matched.start(), matched.end()))
                    .collect()
            });
            if completed_matcher
                .as_ref()
                .is_some_and(|_| !match_ranges.is_empty())
            {
                matching_line_indices.push(first_line_index + relative_line_index);
            }
            appended_rows.push(LogPageRow {
                display_row: first_line_index + relative_line_index,
                line_index: first_line_index + relative_line_index,
                timestamp: visible_line.timestamp,
                text: visible_line.line.text,
                style_spans: visible_line.line.style_spans,
                match_ranges,
            });
        }
        data.flush()?;
        // The index is published only after the complete batch of records, so
        // readers can never observe a partially spooled logical line.
        offsets.seek(SeekFrom::End(0))?;
        for offset in line_offsets {
            offsets.write_all(&offset.to_le_bytes())?;
        }
        offsets.flush()?;
        self.total_lines += lines.len();
        if let Some(search) = &mut self.search
            && search.complete
            && !matching_line_indices.is_empty()
        {
            let mut matches = search
                .match_offsets
                .as_ref()
                .expect("completed search has an index")
                .reopen()?;
            matches.seek(SeekFrom::End(0))?;
            for line_index in matching_line_indices {
                matches.write_all(&line_index.to_le_bytes())?;
                search.match_count += 1;
            }
            matches.flush()?;
        }
        Ok(AppendSummary {
            total_lines: self.visible_total_lines(),
            completed_search: self
                .search
                .as_ref()
                .filter(|search| search.complete)
                .map(|search| (search.generation, search.match_count)),
            appended_rows,
        })
    }

    pub(crate) fn start_search(
        &mut self,
        window_id: u64,
        generation: u64,
        query: String,
        regex_mode: bool,
        sender: StoreCommandSender,
        search_progress_interval: usize,
    ) -> anyhow::Result<()> {
        if let Some(search) = &self.search {
            search.cancellation.store(true, Ordering::Relaxed);
        }
        if query.is_empty() {
            self.search = None;
            return Ok(());
        }
        let pattern = if regex_mode {
            query
        } else {
            regex::escape(&query)
        };
        let matcher = regex::RegexBuilder::new(&pattern)
            .case_insensitive(true)
            .build()?;
        let cancellation = Arc::new(AtomicBool::new(false));
        let scan_lines = self.visible_total_lines();
        let reader = self.logical_reader()?;
        self.search = Some(SearchState {
            generation,
            matcher: matcher.clone(),
            cancellation: cancellation.clone(),
            match_offsets: Some(NamedTempFile::new()?),
            match_count: 0,
            complete: false,
        });
        thread::Builder::new()
            .name("pod-log-search".to_owned())
            .spawn(move || {
                scan_records(SearchScan {
                    reader,
                    scan_lines,
                    matcher,
                    cancellation,
                    window_id,
                    generation,
                    sender,
                    search_progress_interval,
                })
            })
            .map_err(anyhow::Error::from)?;
        Ok(())
    }

    pub(crate) fn append_search_matches(
        &mut self,
        line_indices: Vec<usize>,
    ) -> anyhow::Result<usize> {
        let Some(search) = &mut self.search else {
            return Ok(0);
        };
        let mut file = search
            .match_offsets
            .as_ref()
            .expect("search index exists")
            .reopen()?;
        file.seek(SeekFrom::End(0))?;
        for line_index in line_indices {
            file.write_all(&line_index.to_le_bytes())?;
            search.match_count += 1;
        }
        file.flush()?;
        Ok(search.match_count)
    }

    pub(crate) fn finish_search(&mut self, scanned_lines: usize) -> anyhow::Result<usize> {
        let Some(search) = &self.search else {
            return Ok(0);
        };
        let matcher = search.matcher.clone();
        let mut tail_matches = Vec::new();
        // Any lines appended while the background scan ran are searched once
        // here before the index becomes visible.
        for line_index in scanned_lines..self.visible_total_lines() {
            let line = self.read_line(line_index)?;
            if matcher.is_match(&parse_kubernetes_log_line(&line).line.text) {
                tail_matches.push(line_index);
            }
        }
        let search = self.search.as_mut().expect("search state must still exist");
        let matches = search.match_offsets.as_ref().expect("search index exists");
        let mut file = matches.reopen()?;
        file.seek(SeekFrom::End(0))?;
        for line_index in tail_matches {
            file.write_all(&line_index.to_le_bytes())?;
            search.match_count += 1;
        }
        file.flush()?;
        search.complete = true;
        Ok(search.match_count)
    }

    pub(crate) fn page(
        &mut self,
        generation: u64,
        filter_matches: bool,
        page_start: usize,
        page_size: usize,
    ) -> anyhow::Result<(usize, Vec<LogPageRow>)> {
        let (total_rows, matcher, match_offsets) = if filter_matches {
            let Some(search) = &self.search else {
                return Ok((0, Vec::new()));
            };
            if search.generation != generation {
                return Ok((0, Vec::new()));
            }
            (
                search.match_count,
                Some(search.matcher.clone()),
                Some(
                    search
                        .match_offsets
                        .as_ref()
                        .expect("search has index")
                        .reopen()?,
                ),
            )
        } else {
            (
                self.visible_total_lines(),
                self.search
                    .as_ref()
                    .filter(|search| search.generation == generation)
                    .map(|search| search.matcher.clone()),
                None,
            )
        };
        let end = (page_start + page_size).min(total_rows);
        let mut rows = Vec::with_capacity(end.saturating_sub(page_start));
        let mut matching_offsets = match_offsets;
        for display_row in page_start..end {
            let line_index = if filter_matches {
                read_u64_at(
                    matching_offsets
                        .as_mut()
                        .expect("filtered log pages have a match index"),
                    display_row,
                )? as usize
            } else {
                display_row
            };
            let parsed = parse_kubernetes_log_line(&self.read_line(line_index)?);
            let match_ranges = matcher
                .as_ref()
                .map(|matcher| {
                    matcher
                        .find_iter(&parsed.line.text)
                        .map(|range| (range.start(), range.end()))
                        .collect()
                })
                .unwrap_or_default();
            rows.push(LogPageRow {
                display_row,
                line_index,
                timestamp: parsed.timestamp,
                text: parsed.line.text,
                style_spans: parsed.line.style_spans,
                match_ranges,
            });
        }
        Ok((total_rows, rows))
    }

    pub(crate) fn search_generation(&self) -> Option<u64> {
        self.search.as_ref().map(|search| search.generation)
    }

    pub(crate) fn match_line(
        &self,
        generation: u64,
        match_row: usize,
    ) -> anyhow::Result<Option<usize>> {
        let Some(search) = &self.search else {
            return Ok(None);
        };
        if search.generation != generation || match_row >= search.match_count {
            return Ok(None);
        }
        let mut offsets = search
            .match_offsets
            .as_ref()
            .expect("search has index")
            .reopen()?;
        Ok(Some(read_u64_at(&mut offsets, match_row)? as usize))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn copy_range(
        &mut self,
        generation: u64,
        filter_matches: bool,
        start_row: usize,
        start_byte: usize,
        end_row: usize,
        end_byte: usize,
    ) -> anyhow::Result<String> {
        let total_rows = if filter_matches {
            let Some(search) = &self.search else {
                return Ok(String::new());
            };
            if search.generation != generation {
                return Ok(String::new());
            }
            search.match_count
        } else {
            self.visible_total_lines()
        };
        if start_row >= total_rows || start_row > end_row {
            return Ok(String::new());
        }
        let end_row = end_row.min(total_rows - 1);
        let mut text = String::new();
        let mut match_offsets = filter_matches
            .then(|| {
                self.search
                    .as_ref()
                    .expect("filtered copy has a search")
                    .match_offsets
                    .as_ref()
                    .expect("filtered copy has an index")
                    .reopen()
            })
            .transpose()?;
        for display_row in start_row..=end_row {
            let line_index = if let Some(offsets) = &mut match_offsets {
                read_u64_at(offsets, display_row)? as usize
            } else {
                display_row
            };
            let parsed = parse_kubernetes_log_line(&self.read_line(line_index)?);
            let line = parsed.line.text;
            let start = if display_row == start_row {
                floor_char_boundary(&line, start_byte)
            } else {
                0
            };
            let end = if display_row == end_row {
                floor_char_boundary(&line, end_byte)
            } else {
                line.len()
            };
            if display_row != start_row {
                text.push('\n');
            }
            if start < end {
                text.push_str(&line[start..end]);
            }
        }
        Ok(text)
    }
}
