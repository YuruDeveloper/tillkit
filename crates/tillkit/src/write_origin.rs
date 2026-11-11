use crate::write_step::WriteStep;

const MAGISK_PREPARE: &str = "Magisk를 준비해서 init_boot를 패치하고, 기록할 내용을 정합니다.";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WriteOrigin {
    #[default]
    Folder,
    Magisk,
}

impl WriteOrigin {
    pub fn screen_name(self) -> &'static str {
        match self {
            Self::Folder => "WRITE",
            Self::Magisk => "ROOT",
        }
    }

    pub fn sentence(self, step: WriteStep) -> &'static str {
        match (self, step) {
            (Self::Magisk, WriteStep::Prepare) => MAGISK_PREPARE,
            _ => step.sentence(),
        }
    }
}
