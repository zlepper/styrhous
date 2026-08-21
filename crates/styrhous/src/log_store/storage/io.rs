use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub(super) fn floor_char_boundary(text: &str, byte_offset: usize) -> usize {
    let mut byte_offset = byte_offset.min(text.len());
    while byte_offset > 0 && !text.is_char_boundary(byte_offset) {
        byte_offset -= 1;
    }
    byte_offset
}

pub(super) fn read_line_from(
    data: &mut File,
    offsets: &mut File,
    line_index: usize,
) -> anyhow::Result<String> {
    let offset = read_u64_at(offsets, line_index)?;
    read_line_at(data, offset)
}

pub(super) fn read_line_at(data: &mut File, offset: u64) -> anyhow::Result<String> {
    data.seek(SeekFrom::Start(offset))?;
    let mut length = [0_u8; 4];
    data.read_exact(&mut length)?;
    let mut bytes = vec![0; u32::from_le_bytes(length) as usize];
    data.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(anyhow::Error::from)
}

pub(super) fn read_u64_at(file: &mut File, index: usize) -> anyhow::Result<u64> {
    file.seek(SeekFrom::Start((index * std::mem::size_of::<u64>()) as u64))?;
    let mut bytes = [0_u8; 8];
    file.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}
