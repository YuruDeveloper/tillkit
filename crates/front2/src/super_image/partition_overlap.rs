use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionOverlap {
    pub partition: String,
    pub overlap_bytes: u64,
}
