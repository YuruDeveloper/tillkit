use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootLunRecord {
    pub requested_lun: u8,
    pub boot_wlun: u8,
    pub command_accepted: bool,
    pub readback_sha256: Option<String>,
    pub matches_new_lun_gpt: bool,
    pub matches_original_lun_gpt: bool,
    pub passed: bool,
}

impl BootLunRecord {
    pub fn requested(requested_lun: u8, boot_wlun: u8) -> Self {
        Self {
            requested_lun,
            boot_wlun,
            command_accepted: false,
            readback_sha256: None,
            matches_new_lun_gpt: false,
            matches_original_lun_gpt: false,
            passed: false,
        }
    }

    pub fn observed(requested_lun: u8, boot_wlun: u8, readback_sha256: &str) -> Self {
        Self {
            requested_lun,
            boot_wlun,
            command_accepted: false,
            readback_sha256: Some(readback_sha256.to_string()),
            matches_new_lun_gpt: true,
            matches_original_lun_gpt: false,
            passed: true,
        }
    }
}
