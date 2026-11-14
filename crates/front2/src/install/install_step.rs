use crate::install::layout::{KEEP_BOOT_LUN_STEP_NAME, SET_BOOT_LUN_STEP_NAME};
use crate::install::write_step::WriteStep;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstallStep {
    Write(WriteStep),
    SetBootLun { lun: u8 },
    KeepBootLun { lun: u8 },
    Restart,
}

impl InstallStep {
    pub fn as_write(&self) -> Option<&WriteStep> {
        match self {
            Self::Write(step) => Some(step),
            _ => None,
        }
    }

    pub fn boot_lun(&self) -> Option<u8> {
        boot_lun_of(self)
    }

    pub fn keeps_boot_lun(&self) -> bool {
        matches!(self, Self::KeepBootLun { .. })
    }

    pub fn label(&self) -> String {
        label_of(self)
    }
}

fn boot_lun_of(step: &InstallStep) -> Option<u8> {
    match step {
        InstallStep::SetBootLun { lun } | InstallStep::KeepBootLun { lun } => Some(*lun),
        _ => None,
    }
}

fn label_of(step: &InstallStep) -> String {
    match step {
        InstallStep::Write(write) => format!("write:{}", write.name),
        InstallStep::SetBootLun { lun } => format!("{SET_BOOT_LUN_STEP_NAME}:{lun}"),
        InstallStep::KeepBootLun { lun } => format!("{KEEP_BOOT_LUN_STEP_NAME}:{lun}"),
        InstallStep::Restart => "restart".to_string(),
    }
}
