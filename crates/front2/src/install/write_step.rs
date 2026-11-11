use crate::install::step_source::StepSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteStep {
    pub name: String,
    pub lun: u8,
    pub start_sector: u64,
    pub sectors: u64,
    pub source: StepSource,
    pub bytes: u64,
    pub sha256: String,
    pub expected_before_sha256: String,
    pub requires_data_wipe_approval: bool,
}

impl WriteStep {
    pub fn end_sector(&self) -> Option<u64> {
        self.start_sector.checked_add(self.sectors)
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        let end = self.start_sector.saturating_add(self.sectors);
        let other_end = other.start_sector.saturating_add(other.sectors);
        self.lun == other.lun && self.start_sector < other_end && other.start_sector < end
    }
}
