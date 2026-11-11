use crate::image_record::ImageRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateImage {
    pub partition: String,
    pub image: ImageRecord,
    pub backup_source: Option<String>,
}
