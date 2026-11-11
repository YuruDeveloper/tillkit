use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionLocation {
    pub lun: u8,
    pub start_sector: u64,
    pub sectors: u64,
}
