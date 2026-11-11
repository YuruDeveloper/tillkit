use crate::gpt::GptCopyKind;
use crate::image_record::ImageRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GptBackup {
    pub lun: u8,
    pub copy: GptCopyKind,
    pub record: ImageRecord,
}
