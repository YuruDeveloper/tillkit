use crate::gpt::GptCopyKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GptCopyRecord {
    pub lun: u8,
    pub copy: GptCopyKind,
    pub start_sector: u64,
    pub sectors: u64,
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}
