use crate::error::{Front2Error, Result};
use crate::gpt::BootSlot;
use crate::gsi::backup_manifest::BackupManifest;
use crate::gsi::vbmeta_source::VbmetaSource;
use crate::profile;
use crate::profiles::toss_front2;
use std::path::Path;

/// Find the slot whose top and chained vbmeta are signed with the test key.
///
/// The tool can only reproduce the test key signatures, so the install target
/// must be that slot. A device may boot the other slot with a vendor key; this
/// query reads the backup and does not look at the boot slot.
pub fn find_testkey_slot(backup_directory: &Path) -> Result<BootSlot> {
    let manifest = BackupManifest::load(&std::path::absolute(backup_directory)?)?;
    let mut testkey_slots = Vec::new();
    for slot in [BootSlot::A, BootSlot::B] {
        if slot_signed_with_test_key(&manifest, slot)? {
            testkey_slots.push(slot);
        }
    }
    match testkey_slots.as_slice() {
        [slot] => Ok(*slot),
        [] => Err(Front2Error::Unsupported(
            "neither slot has a top and chained vbmeta that is signed with the test key".into(),
        )),
        _ => Err(Front2Error::Unsupported(
            "both slots are signed with the test key, so the install target is not clear".into(),
        )),
    }
}

fn slot_signed_with_test_key(manifest: &BackupManifest, slot: BootSlot) -> Result<bool> {
    let top_name = profile::slot_label(profile::VBMETA_BASE, slot.suffix());
    let chained_name = profile::slot_label(toss_front2::CHAINED_VBMETA_BASE, slot.suffix());
    let top = VbmetaSource::read(&manifest.verified_partition(&top_name)?.path)?;
    let chained = VbmetaSource::read(&manifest.verified_partition(&chained_name)?.path)?;
    let top_test_key = top
        .require_signed_with(
            toss_front2::TOP_VBMETA_KEY_SPEC,
            toss_front2::TOP_VBMETA_ALGORITHM,
        )
        .is_ok();
    let chained_test_key = chained
        .require_signed_with(
            toss_front2::CHAINED_VBMETA_KEY_SPEC,
            toss_front2::CHAINED_VBMETA_ALGORITHM,
        )
        .is_ok();
    Ok(top_test_key && chained_test_key)
}
