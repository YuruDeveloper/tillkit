use super::lp_format::sectors_to_bytes;
use crate::error::{Front2Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub fn from_length(start: u64, bytes: u64) -> Result<Self> {
        let end = start.checked_add(bytes).ok_or_else(|| {
            Front2Error::Image(format!(
                "a range of {bytes} bytes at offset {start} overflows"
            ))
        })?;
        Ok(Self { start, end })
    }

    pub fn from_sectors(target_sector: u64, sectors: u64) -> Result<Self> {
        Self::from_length(sectors_to_bytes(target_sector)?, sectors_to_bytes(sectors)?)
    }

    pub fn bytes(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn sort_disjoint(ranges: &mut [Self], limit: u64, subject: &str) -> Result<()> {
        ranges.sort();
        let ordered = ranges.iter().all(|range| range.start <= range.end);
        let disjoint = ranges.windows(2).all(|pair| pair[0].end <= pair[1].start);
        let inside = ranges.last().is_none_or(|range| range.end <= limit);
        if !ordered || !disjoint || !inside {
            return Err(Front2Error::Verification(format!(
                "{subject} overlap each other or leave the first {limit} bytes: {ranges:?}"
            )));
        }
        Ok(())
    }
}
