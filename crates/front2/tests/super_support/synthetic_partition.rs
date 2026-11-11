pub const TARGET_TYPE_LINEAR: u32 = 0;
pub const TARGET_TYPE_ZERO: u32 = 1;

#[derive(Debug, Clone)]
pub struct SyntheticPartition {
    pub name: String,
    pub attributes: u32,
    pub group_index: u32,
    pub extents: Vec<(u64, u64, u32)>,
}

impl SyntheticPartition {
    pub fn linear(name: &str, attributes: u32, group_index: u32, extents: &[(u64, u64)]) -> Self {
        Self {
            name: name.to_string(),
            attributes,
            group_index,
            extents: extents
                .iter()
                .map(|(target_sector, sectors)| (*target_sector, *sectors, TARGET_TYPE_LINEAR))
                .collect(),
        }
    }

    pub fn zero(name: &str, group_index: u32, target_sector: u64, sectors: u64) -> Self {
        Self {
            name: name.to_string(),
            attributes: 0,
            group_index,
            extents: vec![(target_sector, sectors, TARGET_TYPE_ZERO)],
        }
    }
}
