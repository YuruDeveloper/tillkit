#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageRefusal {
    NotRead,
    NotKnown,
}

impl ImageRefusal {
    pub fn caption(self) -> &'static str {
        match self {
            Self::NotRead => "IMAGE NOT READ",
            Self::NotKnown => "NOT A BOOT IMAGE",
        }
    }

    pub fn sentence(self) -> &'static str {
        match self {
            Self::NotRead => NOT_READ,
            Self::NotKnown => NOT_KNOWN,
        }
    }
}

const NOT_READ: &str =
    "이 파일을 읽지 못했습니다. 파일이 그 자리에 있는지 확인하고 다시 골라 주세요.";
const NOT_KNOWN: &str = "이 파일은 TillKit이 기록하는 이미지가 아닙니다. Magisk 앱으로 패치한 init_boot 이미지나, 커널이 든 boot 이미지를 골라 주세요. 파일 이름은 무엇이든 괜찮습니다.";
