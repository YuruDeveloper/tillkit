use crate::digest::hex;
use crate::error::{Front2Error, Result};
use crate::full_backup::backup_extent::BackupExtent;
use crate::gpt::SECTOR_BYTES;
use crate::gsi::BackupEntry;
use crate::gsi::backup_manifest::BACKUP_MANIFEST_FILE;
use crate::install::{CHUNK_SECTORS, SectorStorage};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

pub(super) fn read_exact_sectors(
    storage: &mut dyn SectorStorage,
    lun: u8,
    start: u64,
    sectors: u64,
) -> Result<Vec<u8>> {
    let data = storage.read_sectors(lun, start, sectors)?;
    if sectors.checked_mul(SECTOR_BYTES as u64) != Some(data.len() as u64) {
        return Err(Front2Error::Verification(format!(
            "backup read on LUN {lun} at sector {start} returned an incorrect byte count"
        )));
    }
    Ok(data)
}

pub(super) fn write_new(directory: &Path, entry: &BackupEntry, bytes: &[u8]) -> Result<()> {
    let path = directory.join(&entry.path);
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    output.write_all(bytes)?;
    output.sync_all()?;
    Ok(())
}

pub(super) fn read_partition(
    storage: &mut dyn SectorStorage,
    extent: &BackupExtent,
    mut output: Option<&mut fs::File>,
) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut done = 0;
    while done < extent.location.sectors {
        let count = (extent.location.sectors - done).min(CHUNK_SECTORS);
        let start = extent
            .location
            .start_sector
            .checked_add(done)
            .ok_or_else(|| Front2Error::Image("backup sector position is out of range".into()))?;
        let data = read_exact_sectors(storage, extent.location.lun, start, count)?;
        if let Some(file) = output.as_mut() {
            file.write_all(&data)?;
        }
        hasher.update(&data);
        done += count;
    }
    Ok(hex(&hasher.finalize()))
}

pub(super) fn save_partition(
    storage: &mut dyn SectorStorage,
    directory: &Path,
    extent: &BackupExtent,
) -> Result<BackupEntry> {
    let path = directory.join(BackupEntry::relative_path(
        &extent.name,
        extent.location.lun,
    ));
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    let sha256 = read_partition(storage, extent, Some(&mut output))?;
    output.sync_all()?;
    if output.metadata()?.len() != extent.bytes {
        return Err(Front2Error::Verification(format!(
            "backup file length differs for {}",
            extent.name
        )));
    }
    if !extent.single_read() && read_partition(storage, extent, None)? != sha256 {
        return Err(Front2Error::Verification(format!(
            "backup second read differs for {}",
            extent.name
        )));
    }
    Ok(extent.entry(sha256))
}

pub(super) fn publish_manifest(directory: &Path, entries: &[BackupEntry]) -> Result<()> {
    let temporary = directory.join("sha256-manifest.json.partial");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    serde_json::to_writer_pretty(&mut output, entries)?;
    output.write_all(b"\n")?;
    output.sync_all()?;
    drop(output);
    fs::rename(temporary, directory.join(BACKUP_MANIFEST_FILE))?;
    Ok(())
}
