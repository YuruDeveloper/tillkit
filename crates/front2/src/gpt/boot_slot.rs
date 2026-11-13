use crate::error::{Front2Error, Result};
use crate::gpt::gpt_copy::GptCopy;
use crate::gpt::slot_activation::{ACTIVE_FLAG, BOOT_A_NAME, BOOT_B_NAME};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootSlot {
    A,
    B,
}

impl BootSlot {
    pub fn read(lun: u8, copy: &GptCopy) -> Result<Self> {
        let active_a = is_active(lun, copy, BOOT_A_NAME)?;
        let active_b = is_active(lun, copy, BOOT_B_NAME)?;
        match (active_a, active_b) {
            (true, false) => Ok(Self::A),
            (false, true) => Ok(Self::B),
            _ => Err(Front2Error::Unsupported(format!(
                "LUN {lun}: the active flag does not show one boot slot, {BOOT_A_NAME} active {active_a}, {BOOT_B_NAME} active {active_b}"
            ))),
        }
    }
}

fn is_active(lun: u8, copy: &GptCopy, name: &str) -> Result<bool> {
    let entry = copy
        .entry(name)
        .ok_or_else(|| Front2Error::Unsupported(format!("LUN {lun}: the GPT holds no {name}")))?;
    Ok(entry.flags & ACTIVE_FLAG != 0)
}
