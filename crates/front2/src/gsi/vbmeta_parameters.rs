#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VbmetaParameters {
    pub algorithm_name: String,
    pub key_spec: String,
    pub rollback_index: u64,
    pub rollback_index_location: u32,
    pub flags: u32,
    pub required_libavb_version_minor: u32,
    pub release_string: String,
}
