use super::EXTENT_ALIGNMENT_SECTORS;
use super::lp_format::{EXTENT_ENTRY_BYTES, TARGET_TYPE_LINEAR, sectors_to_bytes, u32_at, u64_at};
use crate::error::{Front2Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LpExtent {
    pub sectors: u64,
    pub target_type: u32,
    pub target_sector: u64,
    pub target_source: u32,
}

impl LpExtent {
    pub fn linear(target_sector: u64, sectors: u64) -> Self {
        Self {
            sectors,
            target_type: TARGET_TYPE_LINEAR,
            target_sector,
            target_source: 0,
        }
    }

    pub fn decode(record: &[u8]) -> Result<Self> {
        if record.len() != EXTENT_ENTRY_BYTES {
            return Err(Front2Error::Image(format!(
                "super metadata: extent record is {} bytes",
                record.len()
            )));
        }
        Ok(Self {
            sectors: u64_at(record, 0)?,
            target_type: u32_at(record, 8)?,
            target_sector: u64_at(record, 12)?,
            target_source: u32_at(record, 20)?,
        })
    }

    pub fn encode(&self) -> [u8; EXTENT_ENTRY_BYTES] {
        let mut record = [0u8; EXTENT_ENTRY_BYTES];
        record[..8].copy_from_slice(&self.sectors.to_le_bytes());
        record[8..12].copy_from_slice(&self.target_type.to_le_bytes());
        record[12..20].copy_from_slice(&self.target_sector.to_le_bytes());
        record[20..].copy_from_slice(&self.target_source.to_le_bytes());
        record
    }

    pub fn is_linear_on_first_device(&self) -> bool {
        self.target_type == TARGET_TYPE_LINEAR && self.target_source == 0
    }

    pub fn end_sector(&self) -> Result<u64> {
        self.target_sector
            .checked_add(self.sectors)
            .ok_or_else(|| Front2Error::Image(format!("super metadata: {self:?} overflows")))
    }

    pub fn bytes(&self) -> Result<u64> {
        sectors_to_bytes(self.sectors)
    }

    pub fn require_data_extent(&self, first_sector: u64, last_sector: u64) -> Result<()> {
        let end_sector = self.end_sector()?;
        let aligned = self.sectors.is_multiple_of(EXTENT_ALIGNMENT_SECTORS)
            && self.target_sector.is_multiple_of(EXTENT_ALIGNMENT_SECTORS);
        let inside = first_sector <= self.target_sector && end_sector <= last_sector;
        if !self.is_linear_on_first_device() || self.sectors == 0 || !aligned || !inside {
            return Err(Front2Error::Verification(format!(
                "super metadata: {self:?} is not a linear, 4096-byte aligned extent inside sectors {first_sector}..{last_sector}"
            )));
        }
        Ok(())
    }
}
