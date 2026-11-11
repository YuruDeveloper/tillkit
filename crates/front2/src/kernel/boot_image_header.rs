use crate::error::{Front2Error, Result};
use serde::{Deserialize, Serialize};

pub const BOOT_MAGIC: &[u8; 8] = b"ANDROID!";
pub const HEADER_PAGE_BYTES: usize = 4096;
const IMAGE_ALIGNMENT: u64 = HEADER_PAGE_BYTES as u64;
pub const BOOT_V4_HEADER_VERSION: u32 = 4;
pub const BOOT_V4_HEADER_SIZE: u32 = 1584;
pub const KERNEL_SIZE_OFFSET: usize = 8;
const RAMDISK_SIZE_OFFSET: usize = 12;
const OS_VERSION_OFFSET: usize = 16;
const HEADER_SIZE_OFFSET: usize = 20;
const HEADER_VERSION_OFFSET: usize = 40;
const SIGNATURE_SIZE_OFFSET: usize = 1580;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootImageHeader {
    pub header_version: u32,
    pub header_size: u32,
    pub kernel_size: u32,
    pub ramdisk_size: u32,
    pub os_version: u32,
    pub signature_size: u32,
}

impl BootImageHeader {
    pub fn parse(image: &[u8]) -> Result<Self> {
        if image.len() < HEADER_PAGE_BYTES || &image[..BOOT_MAGIC.len()] != BOOT_MAGIC {
            return Err(Front2Error::Image(
                "has no Android boot image header".into(),
            ));
        }
        let field = |offset: usize| {
            u32::from_le_bytes(
                image[offset..offset + 4]
                    .try_into()
                    .expect("four header bytes"),
            )
        };
        let header = Self {
            header_version: field(HEADER_VERSION_OFFSET),
            header_size: field(HEADER_SIZE_OFFSET),
            kernel_size: field(KERNEL_SIZE_OFFSET),
            ramdisk_size: field(RAMDISK_SIZE_OFFSET),
            os_version: field(OS_VERSION_OFFSET),
            signature_size: field(SIGNATURE_SIZE_OFFSET),
        };
        if header.header_version != BOOT_V4_HEADER_VERSION
            || header.header_size != BOOT_V4_HEADER_SIZE
        {
            return Err(Front2Error::Image(format!(
                "is not a version 4 boot image: header version {}, header size {}",
                header.header_version, header.header_size
            )));
        }
        Ok(header)
    }

    pub fn require_kernel_only(&self) -> Result<()> {
        if self.ramdisk_size != 0 {
            return Err(Front2Error::Image(format!(
                "carries a ramdisk of {} bytes, the boot partition of this device carries none",
                self.ramdisk_size
            )));
        }
        if self.signature_size != 0 {
            return Err(Front2Error::Image(format!(
                "carries a boot signature block of {} bytes",
                self.signature_size
            )));
        }
        if self.kernel_size == 0 {
            return Err(Front2Error::Image("carries no kernel".into()));
        }
        Ok(())
    }

    pub fn payload_bytes(&self) -> u64 {
        HEADER_PAGE_BYTES as u64
            + u64::from(self.kernel_size).div_ceil(IMAGE_ALIGNMENT) * IMAGE_ALIGNMENT
    }

    pub fn kernel<'a>(&self, image: &'a [u8]) -> Result<&'a [u8]> {
        let end = HEADER_PAGE_BYTES + self.kernel_size as usize;
        image.get(HEADER_PAGE_BYTES..end).ok_or_else(|| {
            Front2Error::Image(format!(
                "records a kernel of {} bytes, the image has {} bytes",
                self.kernel_size,
                image.len()
            ))
        })
    }
}
