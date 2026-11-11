use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageInfo {
    pub lun: u8,
    pub total_sectors: u64,
    pub sector_bytes: u64,
}
