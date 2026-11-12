use crate::image_refusal::ImageRefusal;
use front2::kernel::boot_image_header::{BOOT_MAGIC, KERNEL_SIZE_OFFSET};
use std::io::Read;
use std::path::Path;

const HEAD_BYTES: u64 = 2048;
const SPARSE_MAGIC: [u8; 4] = [0x3a, 0xff, 0x26, 0xed];
const EROFS_MAGIC_OFFSET: usize = 1024;
const EROFS_MAGIC: [u8; 4] = [0xe2, 0xe1, 0xf5, 0xe0];
const EXT4_MAGIC_OFFSET: usize = 1080;
const EXT4_MAGIC: [u8; 2] = [0x53, 0xef];
const ROOT_IMAGE_FOUND: &str =
    "커널이 없는 부트 이미지입니다. 루트 패치본으로 보고 init_boot 영역에 기록합니다.";
const KERNEL_FOUND: &str = "커널이 든 부트 이미지입니다. boot 영역에 기록합니다.";
const GSI_FOUND: &str = "시스템 이미지(GSI)입니다. TillKit은 아직 GSI를 기기에 기록하지 않습니다. 지금 기록할 수 있는 것은 Magisk 앱으로 패치한 init_boot 이미지와 커널이 든 boot 이미지입니다.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteKind {
    RootImage,
    Kernel,
    Gsi,
}

impl WriteKind {
    pub fn work(self) -> &'static str {
        match self {
            Self::RootImage => "루트 패치본 기록",
            Self::Kernel => "커널 기록",
            Self::Gsi => "GSI 설치 계획",
        }
    }

    pub fn found(self) -> &'static str {
        match self {
            Self::RootImage => ROOT_IMAGE_FOUND,
            Self::Kernel => KERNEL_FOUND,
            Self::Gsi => GSI_FOUND,
        }
    }

    pub fn writes_to_device(self) -> bool {
        self != Self::Gsi
    }

    fn of_boot_head(head: &[u8]) -> Option<Self> {
        let size = head.get(KERNEL_SIZE_OFFSET..KERNEL_SIZE_OFFSET + 4)?;
        let has_kernel = size != [0; 4];
        Some(if has_kernel {
            Self::Kernel
        } else {
            Self::RootImage
        })
    }

    pub fn of_head(head: &[u8]) -> Result<Self, ImageRefusal> {
        let holds =
            |offset: usize, magic: &[u8]| head.get(offset..offset + magic.len()) == Some(magic);
        if holds(0, BOOT_MAGIC) {
            return Self::of_boot_head(head).ok_or(ImageRefusal::NotKnown);
        }
        let is_system = holds(0, &SPARSE_MAGIC)
            || holds(EROFS_MAGIC_OFFSET, &EROFS_MAGIC)
            || holds(EXT4_MAGIC_OFFSET, &EXT4_MAGIC);
        is_system.then_some(Self::Gsi).ok_or(ImageRefusal::NotKnown)
    }

    pub fn of_file(image: &Path) -> Result<Self, ImageRefusal> {
        let mut head = Vec::new();
        std::fs::File::open(image)
            .and_then(|file| file.take(HEAD_BYTES).read_to_end(&mut head))
            .map_err(|_| ImageRefusal::NotRead)?;
        Self::of_head(&head)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn boot_head(kernel_size: u32) -> Vec<u8> {
        let mut head = vec![0; HEAD_BYTES as usize];
        head[..BOOT_MAGIC.len()].copy_from_slice(BOOT_MAGIC);
        head[KERNEL_SIZE_OFFSET..KERNEL_SIZE_OFFSET + 4]
            .copy_from_slice(&kernel_size.to_le_bytes());
        head
    }

    fn with_magic(offset: usize, magic: &[u8]) -> Vec<u8> {
        let mut head = vec![0; HEAD_BYTES as usize];
        head[offset..offset + magic.len()].copy_from_slice(magic);
        head
    }

    #[test]
    fn a_boot_image_is_told_by_its_kernel_size() {
        assert_eq!(WriteKind::of_head(&boot_head(0)), Ok(WriteKind::RootImage));
        assert_eq!(WriteKind::of_head(&boot_head(1)), Ok(WriteKind::Kernel));
        assert_eq!(
            WriteKind::of_head(&boot_head(40_000_000)),
            Ok(WriteKind::Kernel)
        );
    }

    #[test]
    fn a_system_image_is_told_by_its_file_system() {
        for head in [
            with_magic(0, &SPARSE_MAGIC),
            with_magic(EROFS_MAGIC_OFFSET, &EROFS_MAGIC),
            with_magic(EXT4_MAGIC_OFFSET, &EXT4_MAGIC),
        ] {
            assert_eq!(WriteKind::of_head(&head), Ok(WriteKind::Gsi));
        }
        assert!(!WriteKind::Gsi.writes_to_device());
        assert!(WriteKind::RootImage.writes_to_device() && WriteKind::Kernel.writes_to_device());
    }

    #[test]
    fn other_content_is_refused() {
        let vbmeta = with_magic(0, b"AVB0");
        let vendor_boot = with_magic(0, b"VNDRBOOT");
        let cut_boot = &boot_head(0)[..10];
        for head in [&vbmeta[..], &vendor_boot[..], cut_boot, &[][..]] {
            assert_eq!(WriteKind::of_head(head), Err(ImageRefusal::NotKnown));
        }
    }

    #[test]
    fn a_file_is_read_by_its_content_and_not_by_its_name() {
        let directory = tempfile::tempdir().expect("tempdir");
        let patched = directory.path().join("magisk_patched-31100_AbCdE.img");
        std::fs::write(&patched, boot_head(0)).expect("image");
        assert_eq!(WriteKind::of_file(&patched), Ok(WriteKind::RootImage));
        let named_as_boot = directory.path().join("boot.img");
        std::fs::write(&named_as_boot, b"not an image").expect("file");
        assert_eq!(
            WriteKind::of_file(&named_as_boot),
            Err(ImageRefusal::NotKnown)
        );
        assert_eq!(
            WriteKind::of_file(&directory.path().join("absent.img")),
            Err(ImageRefusal::NotRead)
        );
        assert_eq!(
            WriteKind::of_file(directory.path()),
            Err(ImageRefusal::NotRead)
        );
    }
}
