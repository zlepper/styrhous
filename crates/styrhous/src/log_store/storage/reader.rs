use super::core::LogStore;
use super::io::{read_line_at, read_u64_at};

impl LogStore {
    pub(crate) fn read_line(&self, line_index: usize) -> anyhow::Result<String> {
        if let Some(rebase) = self.rebase {
            if line_index < rebase.history_lines {
                return self
                    .backfill
                    .as_ref()
                    .expect("rebased store retains its history segment")
                    .read_line(line_index);
            }
            return self.read_live_line(line_index - rebase.history_lines + rebase.live_start);
        }
        self.read_live_line(line_index)
    }

    pub(crate) fn read_live_line(&self, line_index: usize) -> anyhow::Result<String> {
        let mut data = self.data()?.reopen()?;
        let offset = read_u64_at(&mut self.offsets()?.reopen()?, line_index)?;
        read_line_at(&mut data, offset)
    }
}
