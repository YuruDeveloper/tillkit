#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullRestoreStage {
    Check,
    Restore,
    Restart,
}

impl FullRestoreStage {
    pub const ALL: [Self; 3] = [Self::Check, Self::Restore, Self::Restart];

    pub fn number(self) -> &'static str {
        match self {
            Self::Check => "R1",
            Self::Restore => "R2",
            Self::Restart => "R3",
        }
    }

    pub fn caption(self) -> &'static str {
        match self {
            Self::Check => "CHECK",
            Self::Restore => "RESTORE",
            Self::Restart => "RESTART",
        }
    }

    pub fn sentence(self) -> &'static str {
        match self {
            Self::Check => CHECK,
            Self::Restore => RESTORE,
            Self::Restart => RESTART,
        }
    }

    pub fn action(self) -> &'static str {
        match self {
            Self::Check => "CHECK DEVICE",
            Self::Restore => "RESTORE",
            Self::Restart => "RESTART",
        }
    }

    pub fn failed_caption(self) -> &'static str {
        match self {
            Self::Check => "CHECK STOPPED",
            Self::Restore => "RESTORE STOPPED · KEEP THE DEVICE IN EDL",
            Self::Restart => "RESTART NOT CONFIRMED",
        }
    }

    pub fn failed_sentence(self) -> &'static str {
        match self {
            Self::Check => CHECK_FAILED,
            Self::Restore => RESTORE_FAILED,
            Self::Restart => RESTART_FAILED,
        }
    }
}

const CHECK: &str =
    "기기의 지금 내용을 읽어서, 고른 전체 백업과 다른 영역을 찾습니다. 기기에는 기록하지 않습니다.";
const RESTORE: &str =
    "다른 영역만 전체 백업의 내용으로 기록하고, 다시 읽어서 확인한 뒤에 부팅 위치를 맞춥니다.";
const RESTART: &str = "기기를 다시 시작합니다. 다시 시작하지 않고 같은 연결에서 설치의 CHECK DEVICE를 새로 할 수도 있습니다.";
const CHECK_FAILED: &str = "기기를 백업과 비교하다가 멈췄습니다. 기기에는 기록하지 않았습니다.";
const RESTORE_FAILED: &str = "기록하다가 멈췄습니다. 기기를 다시 시작하지 말고 EDL 모드로 연결해 둔 채로, CHECK DEVICE를 다시 눌러 주세요. 다시 비교하면 아직 백업과 다른 영역만 기록합니다.";
const RESTART_FAILED: &str =
    "다시 시작하라는 요청을 보내다가 멈췄습니다. 기기가 다시 시작했는지 직접 확인해 주세요.";
