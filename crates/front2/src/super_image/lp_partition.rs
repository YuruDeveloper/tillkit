use super::lp_format::{NAME_BYTES, PARTITION_ENTRY_BYTES, u32_at};
use super::lp_name::LpName;
use crate::error::{Front2Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LpPartition {
    pub name: LpName,
    pub attributes: u32,
    pub first_extent_index: u32,
    pub extent_count: u32,
    pub group_index: u32,
}

impl LpPartition {
    pub fn decode(record: &[u8]) -> Result<Self> {
        if record.len() != PARTITION_ENTRY_BYTES {
            return Err(Front2Error::Image(format!(
                "super metadata: partition record is {} bytes",
                record.len()
            )));
        }
        Ok(Self {
            name: LpName::decode(record, 0)?,
            attributes: u32_at(record, NAME_BYTES)?,
            first_extent_index: u32_at(record, NAME_BYTES + 4)?,
            extent_count: u32_at(record, NAME_BYTES + 8)?,
            group_index: u32_at(record, NAME_BYTES + 12)?,
        })
    }

    pub fn encode(&self) -> [u8; PARTITION_ENTRY_BYTES] {
        let mut record = [0u8; PARTITION_ENTRY_BYTES];
        record[..NAME_BYTES].copy_from_slice(self.name.bytes());
        record[NAME_BYTES..NAME_BYTES + 4].copy_from_slice(&self.attributes.to_le_bytes());
        record[NAME_BYTES + 4..NAME_BYTES + 8]
            .copy_from_slice(&self.first_extent_index.to_le_bytes());
        record[NAME_BYTES + 8..NAME_BYTES + 12].copy_from_slice(&self.extent_count.to_le_bytes());
        record[NAME_BYTES + 12..].copy_from_slice(&self.group_index.to_le_bytes());
        record
    }
}
