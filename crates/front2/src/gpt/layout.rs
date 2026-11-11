use crate::error::{Front2Error, Result};

pub const SECTOR_BYTES: usize = 4096;
pub const TABLE_AREA_SECTORS: usize = 4;
pub const TABLE_AREA_BYTES: usize = TABLE_AREA_SECTORS * SECTOR_BYTES;
pub const SIGNATURE: &[u8; 8] = b"EFI PART";

pub const HEADER_SIZE_OFFSET: usize = 12;
pub const HEADER_CHECKSUM_OFFSET: usize = 16;
pub const HEADER_CURRENT_LBA_OFFSET: usize = 24;
pub const HEADER_ALTERNATE_LBA_OFFSET: usize = 32;
pub const HEADER_FIRST_USABLE_LBA_OFFSET: usize = 40;
pub const HEADER_LAST_USABLE_LBA_OFFSET: usize = 48;
pub const HEADER_DISK_GUID_OFFSET: usize = 56;
pub const DISK_GUID_BYTES: usize = 16;
pub const HEADER_ENTRIES_LBA_OFFSET: usize = 72;
pub const HEADER_ENTRY_COUNT_OFFSET: usize = 80;
pub const HEADER_ENTRY_SIZE_OFFSET: usize = 84;
pub const HEADER_TABLE_CHECKSUM_OFFSET: usize = 88;
pub const HEADER_MINIMUM_BYTES: usize = 92;
pub const CHECKSUM_BYTES: usize = 4;

pub const ENTRY_TYPE_GUID_BYTES: usize = 16;
pub const ENTRY_FIRST_LBA_OFFSET: usize = 32;
pub const ENTRY_LAST_LBA_OFFSET: usize = 40;
pub const ENTRY_FLAGS_OFFSET: usize = 48;
pub const ENTRY_FLAGS_BYTES: usize = 8;
pub const ENTRY_NAME_OFFSET: usize = 56;
pub const ENTRY_NAME_END: usize = 128;
pub const ENTRY_MINIMUM_BYTES: usize = 128;

pub fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    let field = data
        .get(offset..)
        .and_then(|rest| rest.first_chunk::<4>())
        .ok_or_else(|| out_of_range(data, offset, 4))?;
    Ok(u32::from_le_bytes(*field))
}

pub fn read_u64(data: &[u8], offset: usize) -> Result<u64> {
    let field = data
        .get(offset..)
        .and_then(|rest| rest.first_chunk::<8>())
        .ok_or_else(|| out_of_range(data, offset, 8))?;
    Ok(u64::from_le_bytes(*field))
}

pub fn write_u32(data: &mut [u8], offset: usize, value: u32) -> Result<()> {
    let length = data.len();
    let field = data
        .get_mut(offset..)
        .and_then(|rest| rest.first_chunk_mut::<4>())
        .ok_or_else(|| {
            Front2Error::Image(format!("GPT field at {offset} is outside {length} bytes"))
        })?;
    *field = value.to_le_bytes();
    Ok(())
}

pub fn write_u64(data: &mut [u8], offset: usize, value: u64) -> Result<()> {
    let length = data.len();
    let field = data
        .get_mut(offset..)
        .and_then(|rest| rest.first_chunk_mut::<8>())
        .ok_or_else(|| {
            Front2Error::Image(format!("GPT field at {offset} is outside {length} bytes"))
        })?;
    *field = value.to_le_bytes();
    Ok(())
}

pub fn slice(data: &[u8], start: usize, length: usize) -> Result<&[u8]> {
    start
        .checked_add(length)
        .and_then(|end| data.get(start..end))
        .ok_or_else(|| out_of_range(data, start, length))
}

fn out_of_range(data: &[u8], offset: usize, length: usize) -> Front2Error {
    Front2Error::Image(format!(
        "GPT field of {length} bytes at {offset} is outside {} bytes",
        data.len()
    ))
}
