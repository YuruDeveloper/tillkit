#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStep {
    Plan,
    Stage,
    Write,
    Reboot,
}

impl WriteStep {
    pub const ALL: [Self; 4] = [Self::Plan, Self::Stage, Self::Write, Self::Reboot];

    pub fn number(self) -> &'static str {
        match self {
            Self::Plan => "01",
            Self::Stage => "02",
            Self::Write => "03",
            Self::Reboot => "04",
        }
    }

    pub fn caption(self) -> &'static str {
        match self {
            Self::Plan => "PLAN",
            Self::Stage => "STAGE AND CHECK",
            Self::Write => "WRITE PAIR",
            Self::Reboot => "REBOOT",
        }
    }

    pub fn sentence(self) -> &'static str {
        match self {
            Self::Plan => PLAN_SENTENCE,
            Self::Stage => STAGE_SENTENCE,
            Self::Write => WRITE_SENTENCE,
            Self::Reboot => REBOOT_SENTENCE,
        }
    }
}

const PLAN_SENTENCE: &str = "기기의 현재 쌍을 읽어 보관함에서 찾고, 기록 계획을 파일로 저장합니다. 파티션에는 기록하지 않습니다.";
const STAGE_SENTENCE: &str = "새 쌍과 되돌릴 쌍, 기록 스크립트를 기기의 /data/local/tmp에 올리고 사전 점검을 실행합니다. 파티션에는 기록하지 않습니다.";
const WRITE_SENTENCE: &str = "init_boot_b와 vbmeta_b에 새 쌍을 기록하고 다시 읽어 해시를 대조합니다. 중간에 실패하면 되돌릴 쌍을 다시 기록합니다.";
const REBOOT_SENTENCE: &str = "기록 상태와 파티션 해시를 한 번 더 확인한 뒤에 기기를 재부팅합니다.";
