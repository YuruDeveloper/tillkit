use crate::device_profile::DeviceProfile;
use crate::error::{Front2Error, Result};
use crate::kernel::hash_descriptor::HashDescriptor;
use crate::kernel::kernel_set::{BOOT_BASE, BOOT_FILE, VBMETA_FILE};
use crate::top_signature::verify_top_signature;
use ltbox_patch::avb;
use std::fs;
use std::path::Path;

pub fn verify_kernel_images(directory: &Path, profile: &DeviceProfile) -> Result<()> {
    let vbmeta = directory.join(VBMETA_FILE);
    let boot = directory.join(BOOT_FILE);
    verify_top_signature(&vbmeta, profile.top_vbmeta_key_spec)?;
    avb::verify_root_backup(&boot, Some(&vbmeta), BOOT_BASE)?;
    let descriptors = HashDescriptor::read_all(&vbmeta)?;
    let descriptor = HashDescriptor::find(&descriptors, BOOT_BASE)?;
    let computed = descriptor.digest_of(&fs::read(&boot)?)?;
    if computed != descriptor.digest {
        return Err(Front2Error::Verification(format!(
            "{} has salted digest {computed} over {} bytes, the top vbmeta records {}",
            boot.display(),
            descriptor.image_bytes,
            descriptor.digest
        )));
    }
    Ok(())
}
