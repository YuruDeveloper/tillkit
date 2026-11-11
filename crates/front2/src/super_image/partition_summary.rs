use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionSummary {
    pub name: String,
    pub attributes: u32,
    pub bytes: u64,
    pub extent_count: u32,
}
