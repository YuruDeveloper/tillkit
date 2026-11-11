use crate::image_record::ImageRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallImage {
    pub partition: String,
    pub candidate: ImageRecord,
    pub original: ImageRecord,
}
