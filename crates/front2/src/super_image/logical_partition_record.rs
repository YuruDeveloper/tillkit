use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogicalPartitionRecord {
    pub partition: String,
    pub bytes: u64,
    pub sha256: String,
    pub original_preserved: bool,
}
