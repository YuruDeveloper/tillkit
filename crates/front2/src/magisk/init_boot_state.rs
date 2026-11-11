use crate::error::{Front2Error, Result};
use ltbox_patch::boot;
use std::fs;
use std::path::Path;

const RAMDISK: &str = "ramdisk.cpio";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitBootState {
    Stock,
    MagiskPatched,
}

impl InitBootState {
    pub fn inspect(image: &Path, directory: &Path) -> Result<Self> {
        fs::create_dir(directory)?;
        boot::unpack(image, directory)?;
        if !directory.join(RAMDISK).is_file() {
            return Err(Front2Error::Image(format!(
                "{} has no {RAMDISK}",
                image.display()
            )));
        }
        match boot::cpio(directory, RAMDISK, &["test"])? {
            0 => Ok(Self::Stock),
            1 => Ok(Self::MagiskPatched),
            status => Err(Front2Error::Image(format!(
                "unsupported ramdisk layout, cpio test status {status}"
            ))),
        }
    }

    pub fn require_stock(image: &Path, directory: &Path) -> Result<()> {
        match Self::inspect(image, directory)? {
            Self::Stock => Ok(()),
            Self::MagiskPatched => Err(Front2Error::AlreadyPatched(image.display().to_string())),
        }
    }
}
