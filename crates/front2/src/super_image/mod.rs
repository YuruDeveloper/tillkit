pub mod byte_range;
pub mod file_ranges;
pub mod logical_partition_record;
pub mod lp_block_device;
pub mod lp_extent;
pub mod lp_format;
pub mod lp_geometry;
pub mod lp_group;
pub mod lp_metadata;
pub mod lp_name;
pub mod lp_partition;
pub mod lp_table;
pub mod partition_overlap;
pub mod partition_summary;
pub mod planned_extent;
pub mod rebuilt_metadata;
pub mod retained_partition;
pub mod super_candidate;
pub mod super_candidate_report;
pub mod super_candidate_request;
pub mod super_layout_plan;
pub mod super_verification;
pub mod super_verification_request;
pub mod super_verifier;

pub use byte_range::ByteRange;
pub use logical_partition_record::LogicalPartitionRecord;
pub use lp_block_device::LpBlockDevice;
pub use lp_extent::LpExtent;
pub use lp_geometry::LpGeometry;
pub use lp_group::LpGroup;
pub use lp_metadata::LpMetadata;
pub use lp_name::LpName;
pub use lp_partition::LpPartition;
pub use lp_table::LpTable;
pub use partition_overlap::PartitionOverlap;
pub use partition_summary::PartitionSummary;
pub use planned_extent::PlannedExtent;
pub use rebuilt_metadata::RebuiltMetadata;
pub use retained_partition::RetainedPartition;
pub use super_candidate::build_super_candidate;
pub use super_candidate_report::SuperCandidateReport;
pub use super_candidate_request::SuperCandidateRequest;
pub use super_layout_plan::SuperLayoutPlan;
pub use super_verification::SuperVerification;
pub use super_verification_request::SuperVerificationRequest;
pub use super_verifier::verify_super_candidate;

pub const SLOT_COUNT: u32 = 3;
pub const ORIGINAL_SLOT: u32 = 0;
pub const TARGET_SLOT: u32 = 1;
pub const SYSTEM_PARTITION: &str = "system_b";
pub const RETAINED_PARTITIONS: [&str; 5] = [
    "odm_b",
    "product_b",
    "system_dlkm_b",
    "vendor_b",
    "vendor_dlkm_b",
];
pub const DEFAULT_GROUP: &str = "default";
pub const DYNAMIC_GROUP: &str = "qti_dynamic_partitions_b";
pub const REBUILT_GROUP_INDEX: u32 = 1;
pub const SYSTEM_ATTRIBUTES: u32 = 1;
pub const GSI_BLOCK_BYTES: u64 = 4096;
pub const EXTENT_ALIGNMENT_SECTORS: u64 = 8;
