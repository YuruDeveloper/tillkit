#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderRefusal {
    NoImage,
    SeveralImages,
}

impl FolderRefusal {
    pub fn caption(self) -> &'static str {
        match self {
            Self::NoImage => "NO IMAGE",
            Self::SeveralImages => "SEVERAL IMAGES",
        }
    }

    pub fn sentence(self) -> &'static str {
        match self {
            Self::NoImage => NO_IMAGE,
            Self::SeveralImages => SEVERAL_IMAGES,
        }
    }
}

const NO_IMAGE: &str = "이 폴더에는 기록할 이미지가 없습니다. 파일 이름이 init_boot.img, boot.img, system.img 가운데 하나여야 합니다.";
const SEVERAL_IMAGES: &str = "이 폴더에는 기록할 이미지가 둘 이상 있습니다. 한 번에 하나만 기록하므로, 기록할 이미지 하나만 남겨 주세요.";
