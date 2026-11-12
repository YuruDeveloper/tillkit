use crate::write_step::WriteStep;

const MAGISK_PREPARE: &str = "Magisk를 준비해서 init_boot를 패치하고, 기록할 내용을 정합니다.";
const BACKUP_PREPARE: &str = "백업과 기기의 지금 상태를 견주어서, 기록할 내용을 정합니다.";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WriteOrigin {
    #[default]
    Image,
    Magisk,
    Backup,
}

impl WriteOrigin {
    pub fn screen_name(self) -> &'static str {
        match self {
            Self::Image => "WRITE",
            Self::Magisk => "ROOT",
            Self::Backup => "BACKUP",
        }
    }

    pub fn sentence(self, step: WriteStep) -> &'static str {
        match (self, step) {
            (Self::Magisk, WriteStep::Prepare) => MAGISK_PREPARE,
            (Self::Backup, WriteStep::Prepare) => BACKUP_PREPARE,
            _ => step.sentence(),
        }
    }
}
