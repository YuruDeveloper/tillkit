use super::byte_range::ByteRange;
use super::rebuilt_metadata::RebuiltMetadata;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuperCandidateReport {
    pub original_super_bytes: u64,
    pub original_super_sha256: String,
    pub original_super_unchanged: bool,
    pub gsi_bytes: u64,
    pub gsi_sha256: String,
    pub metadata: RebuiltMetadata,
    pub allowed_super_ranges: Vec<ByteRange>,
    pub outside_ranges_equal_bytes: u64,
    pub candidate_bytes: u64,
    pub candidate_sha256: String,
}
