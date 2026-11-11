use super::synthetic_partition::SyntheticPartition;

#[derive(Debug, Clone)]
pub struct SyntheticSlot {
    pub groups: Vec<(String, u64)>,
    pub block_device_count: usize,
    pub partitions: Vec<SyntheticPartition>,
}

impl SyntheticSlot {
    pub fn partition_mut(&mut self, name: &str) -> &mut SyntheticPartition {
        self.partitions
            .iter_mut()
            .find(|partition| partition.name == name)
            .expect("a synthetic partition with this name")
    }
}
