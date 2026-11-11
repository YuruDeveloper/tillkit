use crate::install::layout::BOOT_LUN_STEP_NAME;
use crate::install::write_step::WriteStep;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstallStep {
    Write(WriteStep),
    SetBootLun { lun: u8 },
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

    pub fn label(&self) -> String {
        match (self.as_write(), self.boot_lun()) {
            (Some(step), _) => format!("write:{}", step.name),
            (None, Some(lun)) => format!("{BOOT_LUN_STEP_NAME}:{lun}"),
            (None, None) => "restart".to_string(),
        }
    }
}

fn boot_lun_of(step: &InstallStep) -> Option<u8> {
    match step {
        InstallStep::SetBootLun { lun } => Some(*lun),
        _ => None,
    }
}
