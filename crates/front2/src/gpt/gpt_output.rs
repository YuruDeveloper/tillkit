use crate::gpt::gpt_copy_kind::GptCopyKind;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GptOutput {
    pub lun: u8,
    pub copy: GptCopyKind,
    pub start_sector: u64,
    pub sectors: u64,
    pub bytes: u64,
    pub sha256: String,
    pub restore_sha256: String,
    #[serde(skip_serializing)]
    pub data: Vec<u8>,
}
