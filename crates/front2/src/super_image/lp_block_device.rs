use super::lp_format::{BLOCK_DEVICE_ENTRY_BYTES, NAME_BYTES, SECTOR_BYTES, u32_at, u64_at};
use super::lp_name::LpName;
use crate::error::{Front2Error, Result};

const NAME_OFFSET: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LpBlockDevice {
    pub first_logical_sector: u64,
    pub alignment: u32,
    pub alignment_offset: u32,
    pub size: u64,
    pub name: LpName,
    pub flags: u32,
}

impl LpBlockDevice {
    pub fn decode(record: &[u8]) -> Result<Self> {
        if record.len() != BLOCK_DEVICE_ENTRY_BYTES {
            return Err(Front2Error::Image(format!(
                "super metadata: block device record is {} bytes",
                record.len()
            )));
        }
        Ok(Self {
            first_logical_sector: u64_at(record, 0)?,
            alignment: u32_at(record, 8)?,
            alignment_offset: u32_at(record, 12)?,
            size: u64_at(record, 16)?,
            name: LpName::decode(record, NAME_OFFSET)?,
            flags: u32_at(record, NAME_OFFSET + NAME_BYTES)?,
        })
    }

    pub fn encode(&self) -> [u8; BLOCK_DEVICE_ENTRY_BYTES] {
        let mut record = [0u8; BLOCK_DEVICE_ENTRY_BYTES];
        record[..8].copy_from_slice(&self.first_logical_sector.to_le_bytes());
        record[8..12].copy_from_slice(&self.alignment.to_le_bytes());
        record[12..16].copy_from_slice(&self.alignment_offset.to_le_bytes());
        record[16..NAME_OFFSET].copy_from_slice(&self.size.to_le_bytes());
        record[NAME_OFFSET..NAME_OFFSET + NAME_BYTES].copy_from_slice(self.name.bytes());
        record[NAME_OFFSET + NAME_BYTES..].copy_from_slice(&self.flags.to_le_bytes());
        record
    }

    pub fn last_sector(&self) -> u64 {
        self.size / SECTOR_BYTES
    }

    pub fn alignment_sectors(&self) -> Result<u64> {
        let alignment = u64::from(self.alignment);
        if alignment == 0 || !alignment.is_multiple_of(SECTOR_BYTES) {
            return Err(Front2Error::Image(format!(
                "super metadata: block device alignment of {alignment} bytes is not a positive multiple of {SECTOR_BYTES}"
            )));
        }
        Ok(alignment / SECTOR_BYTES)
    }
}
