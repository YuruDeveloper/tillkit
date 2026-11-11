use super::magisk_package::MagiskPackage;
use crate::error::Result;
use crate::init_boot_payload::payload_bytes;
use ltbox_patch::avb;
use ltbox_patch::magisk::{extract_apk_payload, patch_root_image};
use ltbox_patch::root_pipeline::RootImageTarget;
use std::fs;
use std::path::{Path, PathBuf};

pub fn patch_init_boot(
    stock_init_boot: &Path,
    package: &MagiskPackage,
    preinit_device: &str,
    work_dir: &Path,
    log: &mut Vec<String>,
) -> Result<PathBuf> {
    package.require_unchanged()?;
    if work_dir.exists() {
        fs::remove_dir_all(work_dir)?;
    }
    fs::create_dir_all(work_dir)?;
    let target = RootImageTarget::InitBoot;
    fs::copy(stock_init_boot, work_dir.join(target.filename()))?;
    extract_apk_payload(&package.path, work_dir)?;
    let patched = patch_root_image(work_dir, target, preinit_device, log)?;
    let original = fs::read(stock_init_boot)?;
    let maximum = avb::max_hash_footer_image_size(original.len() as u64)?;
    payload_bytes(&original, &fs::read(&patched)?, maximum)?;
    Ok(patched)
}
