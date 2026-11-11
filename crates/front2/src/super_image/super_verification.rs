use super::byte_range::ByteRange;
use super::logical_partition_record::LogicalPartitionRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuperVerification {
    pub metadata_sha256: String,
    pub logical_partitions: Vec<LogicalPartitionRecord>,
    pub allowed_super_ranges: Vec<ByteRange>,
    pub outside_ranges_equal_bytes: u64,
    pub candidate_bytes: u64,
    pub candidate_sha256: String,
}
