use crate::digest::is_sha256_hex;
use crate::error::{Front2Error, Result};
use crate::full_backup::backup_extent::BackupExtent;
use crate::full_backup::backup_gpt::BackupGpt;
use crate::full_backup::full_backup_plan::{LUN_COUNT, partition_extents};
use crate::gpt::{GptCopy, GptCopyKind};
use crate::gsi::backup_manifest::BACKUP_MANIFEST_FILE;
use crate::gsi::{BackupEntry, BackupManifest};
use std::fs;
use std::path::Path;

pub fn validate_full_backup(directory: &Path) -> Result<()> {
    let manifest = BackupManifest::load(directory)?;
    let entries: Vec<BackupEntry> =
        serde_json::from_slice(&fs::read(directory.join(BACKUP_MANIFEST_FILE))?)?;
    let mut copies = Vec::new();
    for lun in 0..LUN_COUNT {
        copies.extend(read_copies(&manifest, lun)?);
    }
    let partitions = partition_extents(&copies)?;
    if entries.len() != copies.len() + partitions.len() {
        return Err(Front2Error::Image(
            "backup manifest does not match the complete GPT partition list".into(),
        ));
    }
    for extent in partitions {
        validate_partition(directory, &entries, &extent)?;
    }
    Ok(())
}

fn validate_partition(
    directory: &Path,
    entries: &[BackupEntry],
    extent: &BackupExtent,
) -> Result<()> {
    let mut matching = entries
        .iter()
        .filter(|entry| entry.partition_lun(&extent.name).is_some());
    let (Some(entry), None) = (matching.next(), matching.next()) else {
        return Err(Front2Error::Image(format!(
            "backup must hold one entry for {}",
            extent.name
        )));
    };
    let path = directory.join(BackupEntry::relative_path(
        &extent.name,
        extent.location.lun,
    ));
    let metadata = fs::metadata(path)?;
    if entry.partition_lun(&extent.name) != Some(extent.location.lun)
        || entry.bytes != extent.bytes
        || !is_sha256_hex(&entry.sha256.to_ascii_lowercase())
        || !metadata.is_file()
        || metadata.len() != extent.bytes
    {
        return Err(Front2Error::Verification(format!(
            "backup file does not match the GPT for {}",
            extent.name
        )));
    }
    Ok(())
}

pub(super) fn read_copies(manifest: &BackupManifest, lun: u8) -> Result<Vec<BackupGpt>> {
    let main = read_copy(manifest, lun, GptCopyKind::Main)?;
    let backup = read_copy(manifest, lun, GptCopyKind::Backup)?;
    main.copy
        .require_pair_with(&backup.copy, &format!("backup of LUN {lun}"))?;
    Ok(vec![main, backup])
}

fn read_copy(manifest: &BackupManifest, lun: u8, kind: GptCopyKind) -> Result<BackupGpt> {
    let partition = manifest.verified_partition(&format!("gpt_{}{lun}", kind.label()))?;
    if partition.lun != lun {
        return Err(Front2Error::Image(format!(
            "backup GPT is under a different LUN than {lun}"
        )));
    }
    Ok(BackupGpt {
        lun,
        copy: GptCopy::parse(kind, &fs::read(&partition.path)?)?,
    })
}
