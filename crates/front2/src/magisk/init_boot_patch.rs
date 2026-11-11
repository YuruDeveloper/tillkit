use super::init_boot_state::InitBootState;
use super::patch_inputs::PatchInputs;
use crate::error::{Front2Error, Result};
use crate::init_boot_payload::payload_bytes;
use ltbox_patch::avb;
use ltbox_patch::magisk::{extract_apk_payload, patch_root_image};
use ltbox_patch::root_pipeline::RootImageTarget;
use std::fs;
use std::path::{Path, PathBuf};

const INSPECTION_FOLDER: &str = "inspection";

pub fn patch_init_boot(
    stock_init_boot: &Path,
    inputs: PatchInputs<'_>,
    log: &mut Vec<String>,
) -> Result<PathBuf> {
    inputs.package.require_unchanged()?;
    create_new_folder(inputs.work_dir)?;
    InitBootState::require_stock(stock_init_boot, &inputs.work_dir.join(INSPECTION_FOLDER))?;
    let target = RootImageTarget::InitBoot;
    fs::copy(stock_init_boot, inputs.work_dir.join(target.filename()))?;
    extract_apk_payload(&inputs.package.path, inputs.work_dir)?;
    let patched = patch_root_image(inputs.work_dir, target, inputs.preinit_device, log)?;
    let original = fs::read(stock_init_boot)?;
    let maximum = avb::max_hash_footer_image_size(original.len() as u64)?;
    payload_bytes(&original, &fs::read(&patched)?, maximum)?;
    Ok(patched)
}

fn create_new_folder(work_dir: &Path) -> Result<()> {
    if let Some(parent) = work_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(work_dir).map_err(|error| {
        Front2Error::Plan(format!(
            "the patch needs a new work folder, {}: {error}",
            work_dir.display()
        ))
    })
}
