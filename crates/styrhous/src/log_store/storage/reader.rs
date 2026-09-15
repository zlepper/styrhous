use super::core::LogStore;
use super::io::{read_record_at, read_u64_at};
use crate::log_store::LogRecord;

impl LogStore {
    pub(crate) fn read_record(&self, line_index: usize) -> anyhow::Result<LogRecord> {
        if let Some(rebase) = self.rebase {
            if line_index < rebase.history_lines {
                return self
                    .backfill
                    .as_ref()
                    .expect("rebased store retains its history segment")
                    .read_record(line_index);
            }
            return self.read_live_record(line_index - rebase.history_lines + rebase.live_start);
        }
        self.read_live_record(line_index)
    }

    pub(crate) fn read_live_record(&self, line_index: usize) -> anyhow::Result<LogRecord> {
        let mut data = self.data()?.reopen()?;
        let offset = read_u64_at(&mut self.offsets()?.reopen()?, line_index)?;
        read_record_at(&mut data, offset)
    }
}
