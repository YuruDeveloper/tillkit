use super::lp_format::{GROUP_ENTRY_BYTES, NAME_BYTES, u32_at, u64_at};
use super::lp_name::LpName;
use crate::error::{Front2Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LpGroup {
    pub name: LpName,
    pub flags: u32,
    pub maximum_size: u64,
}

impl LpGroup {
    pub fn decode(record: &[u8]) -> Result<Self> {
        if record.len() != GROUP_ENTRY_BYTES {
            return Err(Front2Error::Image(format!(
                "super metadata: group record is {} bytes",
                record.len()
            )));
        }
        Ok(Self {
            name: LpName::decode(record, 0)?,
            flags: u32_at(record, NAME_BYTES)?,
            maximum_size: u64_at(record, NAME_BYTES + 4)?,
        })
    }

    pub fn encode(&self) -> [u8; GROUP_ENTRY_BYTES] {
        let mut record = [0u8; GROUP_ENTRY_BYTES];
        record[..NAME_BYTES].copy_from_slice(self.name.bytes());
        record[NAME_BYTES..NAME_BYTES + 4].copy_from_slice(&self.flags.to_le_bytes());
        record[NAME_BYTES + 4..].copy_from_slice(&self.maximum_size.to_le_bytes());
        record
    }
}
