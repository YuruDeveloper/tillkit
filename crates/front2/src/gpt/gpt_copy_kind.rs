use crate::gpt::layout::{
    CHECKSUM_BYTES, HEADER_CHECKSUM_OFFSET, HEADER_TABLE_CHECKSUM_OFFSET, SECTOR_BYTES,
    TABLE_AREA_BYTES,
};
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GptCopyKind {
    Main,
    Backup,
}

impl GptCopyKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Backup => "backup",
        }
    }

    pub fn header_offset(self) -> usize {
        match self {
            Self::Main => SECTOR_BYTES,
            Self::Backup => TABLE_AREA_BYTES,
        }
    }

    pub fn table_offset(self) -> usize {
        match self {
            Self::Main => SECTOR_BYTES * 2,
            Self::Backup => 0,
        }
    }

    pub fn checksum_ranges(self) -> [Range<usize>; 2] {
        let header_checksum = self.header_offset() + HEADER_CHECKSUM_OFFSET;
        let table_checksum = self.header_offset() + HEADER_TABLE_CHECKSUM_OFFSET;
        [
            header_checksum..header_checksum + CHECKSUM_BYTES,
            table_checksum..table_checksum + CHECKSUM_BYTES,
        ]
    }

    pub fn dump_bytes(self) -> usize {
        match self {
            Self::Main => SECTOR_BYTES * 2 + TABLE_AREA_BYTES,
            Self::Backup => TABLE_AREA_BYTES + SECTOR_BYTES,
        }
    }
}
