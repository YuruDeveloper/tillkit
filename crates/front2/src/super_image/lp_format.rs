use crate::error::{Front2Error, Result};

pub const SECTOR_BYTES: u64 = 512;
pub const GEOMETRY_OFFSET: u64 = 4096;
pub const GEOMETRY_BACKUP_OFFSET: u64 = 8192;
pub const GEOMETRY_BLOCK_BYTES: usize = 4096;
pub const GEOMETRY_BYTES: usize = 52;
pub const GEOMETRY_MAGIC: u32 = 0x616C_4467;
pub const GEOMETRY_CHECKSUM_OFFSET: usize = 8;
pub const GEOMETRY_FIELDS_OFFSET: usize = 40;
pub const METADATA_OFFSET: u64 = 12288;
pub const LOGICAL_BLOCK_BYTES: u32 = 4096;
pub const HEADER_MAGIC: u32 = 0x414C_5030;
pub const MAJOR_VERSION: u16 = 10;
pub const MINOR_VERSION: u16 = 2;
pub const HEADER_BYTES: usize = 256;
pub const HEADER_CHECKSUM_OFFSET: usize = 12;
pub const TABLES_BYTES_OFFSET: usize = 44;
pub const TABLES_CHECKSUM_OFFSET: usize = 48;
pub const PARTITION_TABLE_OFFSET: usize = 80;
pub const EXTENT_TABLE_OFFSET: usize = 92;
pub const GROUP_TABLE_OFFSET: usize = 104;
pub const BLOCK_DEVICE_TABLE_OFFSET: usize = 116;
pub const HEADER_PRESERVED_OFFSET: usize = 128;
pub const CHECKSUM_BYTES: usize = 32;
pub const NAME_BYTES: usize = 36;
pub const PARTITION_ENTRY_BYTES: usize = 52;
pub const EXTENT_ENTRY_BYTES: usize = 24;
pub const GROUP_ENTRY_BYTES: usize = 48;
pub const BLOCK_DEVICE_ENTRY_BYTES: usize = 64;
pub const TARGET_TYPE_LINEAR: u32 = 0;
pub const TARGET_TYPE_ZERO: u32 = 1;

pub fn bytes_at<const LENGTH: usize>(data: &[u8], offset: usize) -> Result<[u8; LENGTH]> {
    offset
        .checked_add(LENGTH)
        .and_then(|end| data.get(offset..end))
        .and_then(|slice| <[u8; LENGTH]>::try_from(slice).ok())
        .ok_or_else(|| {
            Front2Error::Image(format!(
                "super metadata: {LENGTH} bytes at offset {offset} are outside a record of {} bytes",
                data.len()
            ))
        })
}

pub fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes_at(data, offset)?))
}

pub fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes_at(data, offset)?))
}

pub fn u64_at(data: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(bytes_at(data, offset)?))
}

pub fn put(data: &mut [u8], offset: usize, value: &[u8]) -> Result<()> {
    let length = data.len();
    let target = offset
        .checked_add(value.len())
        .and_then(|end| data.get_mut(offset..end))
        .ok_or_else(|| {
            Front2Error::Image(format!(
                "super metadata: {} bytes at offset {offset} are outside a record of {length} bytes",
                value.len()
            ))
        })?;
    target.copy_from_slice(value);
    Ok(())
}

pub fn sectors_to_bytes(sectors: u64) -> Result<u64> {
    sectors.checked_mul(SECTOR_BYTES).ok_or_else(|| {
        Front2Error::Image(format!(
            "super metadata: {sectors} sectors do not fit a 64-bit byte count"
        ))
    })
}
