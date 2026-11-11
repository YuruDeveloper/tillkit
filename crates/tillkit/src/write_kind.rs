use crate::folder_refusal::FolderRefusal;
use std::path::Path;

const GSI_FOUND: &str =
    "system.img를 찾았습니다. GSI 설치 계획까지 만들고, 기기에는 기록하지 않습니다.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    RootImage,
    Kernel,
    Gsi,
}

impl WriteKind {
    pub const ALL: [Self; 3] = [Self::RootImage, Self::Kernel, Self::Gsi];

    pub fn file_name(self) -> &'static str {
        match self {
            Self::RootImage => "init_boot.img",
            Self::Kernel => "boot.img",
            Self::Gsi => "system.img",
        }
    }

    pub fn work(self) -> &'static str {
        match self {
            Self::RootImage => "루트 패치본 기록",
            Self::Kernel => "커널 기록",
            Self::Gsi => "GSI 설치 계획",
        }
    }

    pub fn found(self) -> &'static str {
        match self {
            Self::RootImage => "init_boot.img를 찾았습니다. 루트 패치본을 기록합니다.",
            Self::Kernel => "boot.img를 찾았습니다. 커널을 기록합니다.",
            Self::Gsi => GSI_FOUND,
        }
    }

    pub fn among(has_file: impl Fn(&str) -> bool) -> Result<Self, FolderRefusal> {
        let mut found = Self::ALL
            .into_iter()
            .filter(|kind| has_file(kind.file_name()));
        match (found.next(), found.next()) {
            (Some(kind), None) => Ok(kind),
            (Some(_), Some(_)) => Err(FolderRefusal::SeveralImages),
            (None, _) => Err(FolderRefusal::NoImage),
        }
    }

    pub fn in_folder(folder: &Path) -> Result<Self, FolderRefusal> {
        Self::among(|name| folder.join(name).is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn among(names: &[&str]) -> Result<WriteKind, FolderRefusal> {
        WriteKind::among(|name| names.contains(&name))
    }

    #[test]
    fn one_known_image_decides_the_work() {
        assert_eq!(among(&["init_boot.img"]), Ok(WriteKind::RootImage));
        assert_eq!(among(&["boot.img", "notes.txt"]), Ok(WriteKind::Kernel));
        assert_eq!(among(&["system.img"]), Ok(WriteKind::Gsi));
    }

    #[test]
    fn a_folder_with_no_known_image_or_with_several_is_refused() {
        assert_eq!(among(&[]), Err(FolderRefusal::NoImage));
        assert_eq!(
            among(&["magisk_patched.img", "vbmeta.img"]),
            Err(FolderRefusal::NoImage)
        );
        assert_eq!(
            among(&["init_boot.img", "boot.img"]),
            Err(FolderRefusal::SeveralImages)
        );
        assert_eq!(
            among(&["boot.img", "system.img"]),
            Err(FolderRefusal::SeveralImages)
        );
    }

    #[test]
    fn a_folder_on_disk_is_read_by_file_name() {
        let directory = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            WriteKind::in_folder(directory.path()),
            Err(FolderRefusal::NoImage)
        );
        std::fs::create_dir(directory.path().join("boot.img")).expect("a folder with the name");
        assert_eq!(
            WriteKind::in_folder(directory.path()),
            Err(FolderRefusal::NoImage)
        );
        std::fs::write(directory.path().join("init_boot.img"), b"image").expect("image");
        assert_eq!(
            WriteKind::in_folder(directory.path()),
            Ok(WriteKind::RootImage)
        );
    }
}
