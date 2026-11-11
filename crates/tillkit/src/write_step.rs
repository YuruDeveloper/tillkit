#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStep {
    Prepare,
    Stage,
    Write,
    Reboot,
}

impl WriteStep {
    pub const ALL: [Self; 4] = [Self::Prepare, Self::Stage, Self::Write, Self::Reboot];

    pub fn number(self) -> &'static str {
        match self {
            Self::Prepare => "01",
            Self::Stage => "02",
            Self::Write => "03",
            Self::Reboot => "04",
        }
    }

    pub fn caption(self) -> &'static str {
        match self {
            Self::Prepare => "PREPARE",
            Self::Stage => "UPLOAD",
            Self::Write => "WRITE",
            Self::Reboot => "REBOOT",
        }
    }

    pub fn sentence(self) -> &'static str {
        match self {
            Self::Prepare => "이미지를 검사하고 기록할 내용을 정합니다.",
            Self::Stage => "이미지를 기기에 올리고 미리 점검합니다.",
            Self::Write => "기기에 기록하고, 다시 읽어서 확인합니다.",
            Self::Reboot => "기기를 다시 시작합니다.",
        }
    }

    pub fn failed_caption(self) -> &'static str {
        match self {
            Self::Prepare => "NOT PREPARED",
            Self::Stage => "NOT UPLOADED",
            Self::Write => "WRITE NOT CONFIRMED · DO NOT REBOOT",
            Self::Reboot => "REBOOT NOT CONFIRMED",
        }
    }

    pub fn failed_sentence(self) -> &'static str {
        match self {
            Self::Prepare => PREPARE_FAILED,
            Self::Stage => STAGE_FAILED,
            Self::Write => WRITE_FAILED,
            Self::Reboot => REBOOT_FAILED,
        }
    }
}

const PREPARE_FAILED: &str = "준비하지 못했습니다. 기기에는 기록하지 않았습니다. 기기의 지금 상태를 BACKUP 화면에서 먼저 백업했는지 확인해 주세요.";
const STAGE_FAILED: &str =
    "이미지를 올리거나 점검하다가 멈췄습니다. 기기의 부팅 영역에는 기록하지 않았습니다.";
const WRITE_FAILED: &str =
    "기록한 결과를 확인하지 못했습니다. 기기를 다시 시작하지 말고 전원을 유지해 주세요.";
const REBOOT_FAILED: &str =
    "재부팅을 요청하다가 멈췄습니다. 기기가 다시 시작했는지 직접 확인해 주세요.";
