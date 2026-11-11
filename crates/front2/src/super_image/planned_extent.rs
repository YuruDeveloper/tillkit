use super::byte_range::ByteRange;
use super::lp_format::sectors_to_bytes;
use crate::error::{Front2Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedExtent {
    pub target_sector: u64,
    pub sectors: u64,
    pub bytes: u64,
}

impl PlannedExtent {
    pub fn from_sectors(target_sector: u64, sectors: u64) -> Result<Self> {
        Ok(Self {
            target_sector,
            sectors,
            bytes: sectors_to_bytes(sectors)?,
        })
    }

    pub fn byte_range(&self) -> Result<ByteRange> {
        let range = ByteRange::from_sectors(self.target_sector, self.sectors)?;
        if range.bytes() != self.bytes {
            return Err(Front2Error::Plan(format!(
                "{self:?} records a byte count that differs from its sector count"
            )));
        }
        Ok(range)
    }

    pub fn end_sector(&self) -> u64 {
        self.target_sector.saturating_add(self.sectors)
    }
}
