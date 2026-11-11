use crate::device_profile::DeviceProfile;
use crate::error::Result;
use crate::pair_manifest::{INIT_BOOT_FILE, VBMETA_FILE};
use crate::profile;
use crate::top_signature::verify_top_signature;
use ltbox_patch::avb;
use std::path::Path;

pub fn verify_pair_images(directory: &Path, device: &DeviceProfile) -> Result<()> {
    let vbmeta = directory.join(VBMETA_FILE);
    verify_top_signature(&vbmeta, device.top_vbmeta_key_spec)?;
    avb::verify_root_backup(
        &directory.join(INIT_BOOT_FILE),
        Some(&vbmeta),
        profile::INIT_BOOT_BASE,
    )?;
    Ok(())
}
