use crate::error::{Front2Error, Result};
use crate::kernel::kernel_set::BOOT_BASE;
use avbtool_rs::info::DescriptorInfo;
use std::path::Path;

pub fn require_unchanged_except_boot(new_vbmeta: &Path, old_vbmeta: &Path) -> Result<()> {
    let new = other_descriptors(new_vbmeta)?;
    let mut old = other_descriptors(old_vbmeta)?;
    let same_count = new.len() == old.len();
    let all_found = new.iter().all(|descriptor| take(&mut old, descriptor));
    if !same_count || !all_found {
        return Err(Front2Error::Verification(format!(
            "{} and {} differ in a top vbmeta descriptor other than the {BOOT_BASE} hash",
            new_vbmeta.display(),
            old_vbmeta.display()
        )));
    }
    Ok(())
}

fn other_descriptors(vbmeta: &Path) -> Result<Vec<DescriptorInfo>> {
    let info = avbtool_rs::image::inspect_avb_image(vbmeta)
        .map_err(|error| Front2Error::Image(format!("inspect {}: {error}", vbmeta.display())))?;
    Ok(info
        .descriptors
        .into_iter()
        .filter(|descriptor| !is_boot_hash(descriptor))
        .collect())
}

fn is_boot_hash(descriptor: &DescriptorInfo) -> bool {
    matches!(
        descriptor,
        DescriptorInfo::Hash { partition_name, .. } if partition_name == BOOT_BASE
    )
}

fn take(pool: &mut Vec<DescriptorInfo>, wanted: &DescriptorInfo) -> bool {
    let Some(index) = pool.iter().position(|candidate| candidate == wanted) else {
        return false;
    };
    pool.swap_remove(index);
    true
}
