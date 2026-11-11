use crate::image_record::ImageRecord;
use crate::partition_target::PartitionTarget;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedWrite {
    pub target: PartitionTarget,
    pub new_image: ImageRecord,
    pub old_image: ImageRecord,
    pub staged_new: String,
    pub staged_old: String,
    pub state_while_writing: String,
}
