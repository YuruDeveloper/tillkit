use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreEntry {
    pub name: String,
    pub lun: u8,
    pub start_sector: u64,
    pub sectors: u64,
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}
