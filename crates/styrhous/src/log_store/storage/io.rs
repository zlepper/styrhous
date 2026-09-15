use crate::log_store::{LogRecord, read_log_record, write_log_record};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub(super) fn floor_char_boundary(text: &str, byte_offset: usize) -> usize {
    let mut byte_offset = byte_offset.min(text.len());
    while byte_offset > 0 && !text.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }
    byte_offset
}

pub(super) fn read_record_from(
    data: &mut File,
    offsets: &mut File,
    line_index: usize,
) -> anyhow::Result<LogRecord> {
    let offset = read_u64_at(offsets, line_index)?;
    read_record_at(data, offset)
}

pub(super) fn read_record_at(data: &mut File, offset: u64) -> anyhow::Result<LogRecord> {
    data.seek(SeekFrom::Start(offset))?;
    read_log_record(data)?.ok_or_else(|| anyhow::anyhow!("Missing log record in store"))
}

pub(super) fn write_record(file: &mut File, record: &LogRecord) -> anyhow::Result<u64> {
    let offset = file.seek(SeekFrom::End(0))?;
    write_log_record(file, record)?;
    Ok(offset)
}

pub(super) fn read_u64_at(file: &mut File, index: usize) -> anyhow::Result<u64> {
    file.seek(SeekFrom::Start((index * std::mem::size_of::<u64>()) as u64))?;
    let mut bytes = [0_u8; 8];
    file.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}
