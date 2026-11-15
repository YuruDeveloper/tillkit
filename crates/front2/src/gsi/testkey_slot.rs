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
            "neither slot has a top and chained vbmeta that uses the test key and the supported algorithm".into(),
        )),
        _ => Err(Front2Error::Unsupported(
            "both slots use the test key and the supported algorithm, so the install target is not clear".into(),
        )),
    }
}

fn slot_signed_with_test_key(manifest: &BackupManifest, slot: BootSlot) -> Result<bool> {
    let top_name = profile::slot_label(profile::VBMETA_BASE, slot.suffix());
    let chained_name = profile::slot_label(toss_front2::CHAINED_VBMETA_BASE, slot.suffix());
    let top = VbmetaSource::read(&manifest.verified_partition(&top_name)?.path)?;
    let chained = VbmetaSource::read(&manifest.verified_partition(&chained_name)?.path)?;
    Ok(signed_with(
        &top,
        toss_front2::TOP_VBMETA_KEY_SPEC,
        toss_front2::TOP_VBMETA_ALGORITHM,
    )? && signed_with(
        &chained,
        toss_front2::CHAINED_VBMETA_KEY_SPEC,
        toss_front2::CHAINED_VBMETA_ALGORITHM,
    )?)
}

/// Report whether the vbmeta uses the key and the algorithm.
///
/// A key or algorithm mismatch (Unsupported) means the slot is not the
/// target, so the result is false. Any other error, such as a damaged
/// signature or a read failure, propagates. The investigation does not hide
/// it behind a "neither slot" result.
fn signed_with(source: &VbmetaSource, key_spec: &str, algorithm_name: &str) -> Result<bool> {
    match source.require_signed_with(key_spec, algorithm_name) {
        Ok(()) => Ok(true),
        Err(Front2Error::Unsupported(_)) => Ok(false),
        Err(other) => Err(other),
    }
}
