use super::SLOT_COUNT;
use super::file_ranges::read_at;
use super::lp_format::{
    CHECKSUM_BYTES, GEOMETRY_BACKUP_OFFSET, GEOMETRY_BLOCK_BYTES, GEOMETRY_BYTES,
    GEOMETRY_CHECKSUM_OFFSET, GEOMETRY_FIELDS_OFFSET, GEOMETRY_MAGIC, GEOMETRY_OFFSET,
    LOGICAL_BLOCK_BYTES, METADATA_OFFSET, bytes_at, u32_at,
};
use crate::digest::{hex, sha256_bytes};
use crate::error::{Front2Error, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LpGeometry {
    pub maximum_size: u32,
    pub slot_count: u32,
    pub logical_block_size: u32,
}

impl LpGeometry {
    pub fn decode(block: &[u8]) -> Result<Self> {
        let failure = |reason: String| Front2Error::Image(format!("super geometry: {reason}"));
        let mut zeroed: [u8; GEOMETRY_BYTES] = bytes_at(block, 0)?;
        let stored: [u8; CHECKSUM_BYTES] = bytes_at(block, GEOMETRY_CHECKSUM_OFFSET)?;
        zeroed[GEOMETRY_CHECKSUM_OFFSET..GEOMETRY_FIELDS_OFFSET].fill(0);
        if u32_at(block, 0)? != GEOMETRY_MAGIC || u32_at(block, 4)? as usize != GEOMETRY_BYTES {
            return Err(failure("the magic or the structure size is wrong".into()));
        }
        if sha256_bytes(&zeroed) != hex(&stored) {
            return Err(failure("the checksum does not match".into()));
        }
        let geometry = Self {
            maximum_size: u32_at(block, GEOMETRY_FIELDS_OFFSET)?,
            slot_count: u32_at(block, GEOMETRY_FIELDS_OFFSET + 4)?,
            logical_block_size: u32_at(block, GEOMETRY_FIELDS_OFFSET + 8)?,
        };
        let supported = geometry.logical_block_size == LOGICAL_BLOCK_BYTES
            && geometry.slot_count == SLOT_COUNT
            && geometry.maximum_size != 0;
        if !supported {
            return Err(failure(format!("{geometry:?} is not the Front 2 layout")));
        }
        Ok(geometry)
    }

    pub fn read<Source: Read + Seek>(source: &mut Source) -> Result<Self> {
        let primary = read_at(source, GEOMETRY_OFFSET, GEOMETRY_BLOCK_BYTES)?;
        let backup = read_at(source, GEOMETRY_BACKUP_OFFSET, GEOMETRY_BLOCK_BYTES)?;
        let geometry = Self::decode(&primary)?;
        if primary != backup {
            return Err(Front2Error::Image(
                "super geometry: the primary and backup copies differ".into(),
            ));
        }
        Ok(geometry)
    }

    pub fn primary_offset(&self, slot: u32) -> Result<u64> {
        self.slot_offset(slot, 0)
    }

    pub fn backup_offset(&self, slot: u32) -> Result<u64> {
        self.slot_offset(slot, self.slot_count)
    }

    fn slot_offset(&self, slot: u32, first_slot_of_copy: u32) -> Result<u64> {
        if slot >= self.slot_count {
            return Err(Front2Error::Image(format!(
                "super geometry: slot {slot} is outside {} metadata slots",
                self.slot_count
            )));
        }
        let index = u64::from(first_slot_of_copy) + u64::from(slot);
        Ok(METADATA_OFFSET + index * u64::from(self.maximum_size))
    }
}
