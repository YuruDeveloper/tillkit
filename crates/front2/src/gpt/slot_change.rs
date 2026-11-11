use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotChange {
    pub lun: u8,
    pub a: String,
    pub b: String,
    pub flags_before_a: String,
    pub flags_before_b: String,
    pub flags_after_a: String,
    pub flags_after_b: String,
    pub type_guids_swapped: bool,
}

impl SlotChange {
    pub fn flags_text(flags: u64) -> String {
        format!("{flags:016x}")
    }
}
