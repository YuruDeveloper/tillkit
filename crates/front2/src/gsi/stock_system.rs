use crate::error::{Front2Error, Result};
use crate::gpt::BootSlot;
use crate::gsi::backup_manifest::BackupManifest;
use crate::gsi::system_origin::SystemOrigin;
use crate::gsi::vbmeta_source::VbmetaSource;
use crate::profile;
use crate::profiles::toss_front2;
use std::path::Path;

pub fn backup_system_origin(backup_directory: &Path, slot: BootSlot) -> Result<SystemOrigin> {
    let manifest = BackupManifest::load(&std::path::absolute(backup_directory)?)?;
    let name = profile::slot_label(toss_front2::CHAINED_VBMETA_BASE, slot.suffix());
    let chained = manifest.verified_partition(&name)?;
    Ok(SystemOrigin::read(&VbmetaSource::read(&chained.path)?))
}

pub fn require_stock_front2_system(stock: &VbmetaSource) -> Result<()> {
    let failure =
        |reason: String| Front2Error::Unsupported(format!("{}: {reason}", stock.path.display()));
    let advice = "pick a stock backup, not one that already has a GSI";
    match SystemOrigin::read(stock) {
        SystemOrigin::Stock => Ok(()),
        SystemOrigin::NoFingerprint => Err(failure(format!(
            "it has no {} property. {advice}",
            toss_front2::SYSTEM_FINGERPRINT_PROPERTY
        ))),
        SystemOrigin::Other(shown) => Err(failure(format!(
            "its {} is {shown:?}, not the form {}<build number>{}. {advice}",
            toss_front2::SYSTEM_FINGERPRINT_PROPERTY,
            toss_front2::STOCK_SYSTEM_FINGERPRINT_PREFIX,
            toss_front2::STOCK_SYSTEM_FINGERPRINT_SUFFIX
        ))),
    }
}
