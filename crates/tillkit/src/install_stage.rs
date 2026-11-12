#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStage {
    Connect,
    Backup,
    Plan,
    Check,
    Write,
    Wipe,
    Restart,
}

impl InstallStage {
    pub const ALL: [Self; 7] = [
        Self::Connect,
        Self::Backup,
        Self::Plan,
        Self::Check,
        Self::Write,
        Self::Wipe,
        Self::Restart,
    ];

    pub fn number(self) -> &'static str {
        match self {
            Self::Connect => "01",
            Self::Backup => "02",
            Self::Plan => "03",
            Self::Check => "04",
            Self::Write => "05",
            Self::Wipe => "06",
            Self::Restart => "07",
        }
    }

    pub fn caption(self) -> &'static str {
        match self {
            Self::Connect => "CONNECT",
            Self::Backup => "FULL BACKUP",
            Self::Plan => "PLAN",
            Self::Check => "CHECK",
            Self::Write => "WRITE",
            Self::Wipe => "ERASE USER DATA",
            Self::Restart => "RESTART",
        }
    }

    pub fn sentence(self) -> &'static str {
        match self {
            Self::Connect => "EDL 모드인 기기에 Firehose 로더 파일로 연결합니다.",
            Self::Backup => "기기 전체를 읽어서 PC에 저장합니다. 기기에는 기록하지 않습니다.",
            Self::Plan => PLAN,
            Self::Check => CHECK,
            Self::Write => WRITE,
            Self::Wipe => "다음 부팅 때 사용자 데이터를 지우라는 요청을 기기에 기록합니다.",
            Self::Restart => "기기를 다시 시작합니다.",
        }
    }

    pub fn action(self) -> &'static str {
        match self {
            Self::Connect => "CONNECT",
            Self::Backup => "MAKE FULL BACKUP",
            Self::Plan => "MAKE PLAN",
            Self::Check => "CHECK DEVICE",
            Self::Write | Self::Wipe => "WRITE",
            Self::Restart => "RESTART",
        }
    }

    pub fn changes_the_device(self) -> bool {
        matches!(self, Self::Write | Self::Wipe | Self::Restart)
    }

    pub fn failed_caption(self) -> &'static str {
        match self {
            Self::Connect => "NOT CONNECTED",
            Self::Backup => "BACKUP NOT FINISHED",
            Self::Plan => "NOT PLANNED",
            Self::Check => "CHECK STOPPED",
            Self::Write => "WRITE STOPPED · KEEP THE DEVICE IN EDL",
            Self::Wipe => "REQUEST NOT CONFIRMED · KEEP THE DEVICE IN EDL",
            Self::Restart => "RESTART NOT CONFIRMED",
        }
    }

    pub fn failed_sentence(self) -> &'static str {
        match self {
            Self::Connect => CONNECT_FAILED,
            Self::Backup => BACKUP_FAILED,
            Self::Plan => PLAN_FAILED,
            Self::Check => CHECK_FAILED,
            Self::Write => WRITE_FAILED,
            Self::Wipe => WIPE_FAILED,
            Self::Restart => RESTART_FAILED,
        }
    }
}

const PLAN: &str = "전체 백업과 GSI 이미지로 기록할 내용을 정합니다. 이 이미지로 만든 계획이 있으면 그 계획과 그 계획의 전체 백업을 다시 씁니다. 전체 백업을 직접 고른 때에는 그 백업으로 만든 계획만 다시 씁니다. 기기에는 기록하지 않습니다.";
const CHECK: &str = "기기의 지금 내용을 읽어서, 처음 설치인지 멈춘 설치를 이어 하는지 확인합니다. 기기에는 기록하지 않습니다.";
const WRITE: &str = "GSI와 부팅 이미지, 파티션 표를 기록하고, 다시 읽어서 확인한 뒤에 부팅 위치를 바꿉니다. 이 단계에서 metadata 영역도 지우므로, 사용자 데이터는 이 단계부터 되살릴 수 없다고 보아야 합니다.";
const CONNECT_FAILED: &str = "기기에 연결하지 못했습니다. 기기가 EDL 모드인지, 로더 파일이 이 기기의 것인지 확인해 주세요. 기기에는 기록하지 않았습니다.";
const BACKUP_FAILED: &str = "전체 백업을 끝내지 못했습니다. 기기에는 기록하지 않았습니다. 끝나지 않은 백업 폴더는 설치에 쓰지 않습니다.";
const PLAN_FAILED: &str = "설치 계획을 만들지 못했습니다. 기기에는 기록하지 않았습니다.";
const CHECK_FAILED: &str = "기기를 확인하다가 멈췄습니다. 기기에는 기록하지 않았습니다.";
const WRITE_FAILED: &str = "기록하다가 멈췄습니다. 기기를 다시 시작하지 말고 EDL 모드로 연결해 둔 채로, CHECK AGAIN을 눌러 주세요. 멈춘 곳부터 이어 할 수 있는지 확인합니다.";
const WIPE_FAILED: &str = "요청을 기록한 결과를 확인하지 못했습니다. 기기를 다시 시작하지 말고 EDL 모드로 연결해 둔 채로, CHECK AGAIN을 눌러 주세요. 멈춘 곳부터 이어 할 수 있는지 확인합니다.";
const RESTART_FAILED: &str =
    "다시 시작하라는 요청을 보내다가 멈췄습니다. 기기가 다시 시작했는지 직접 확인해 주세요.";
