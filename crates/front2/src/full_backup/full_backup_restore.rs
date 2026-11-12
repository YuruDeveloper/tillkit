use crate::error::{Front2Error, Result};
use crate::full_backup::backup_extent::BackupExtent;
use crate::full_backup::backup_gpt::BackupGpt;
use crate::full_backup::completed_backup::{read_copies, validate_full_backup};
use crate::full_backup::full_backup_plan::{LUN_COUNT, partition_extents};
use crate::full_backup::restore_area::RestoreArea;
use crate::full_backup::restore_check::RestoreCheck;
use crate::gpt::{ACTIVE_FLAG, GptCopyKind, SECTOR_BYTES};
use crate::gsi::backup_manifest::BACKUP_MANIFEST_FILE;
use crate::gsi::{BackupEntry, BackupManifest, BackupPartition};
use crate::install::sector_transfer::read_sectors_sha256;
use crate::install::{BOOT_WLUN, NEW_BOOT_LUN, ORIGINAL_BOOT_LUN, SectorStorage};
use std::fs;
use std::path::Path;

const USER_DATA: &str = "userdata";
const BOOT_TABLES: [(u8, &str); 2] = [(ORIGINAL_BOOT_LUN, "xbl_a"), (NEW_BOOT_LUN, "xbl_b")];

#[derive(Debug, Clone)]
pub struct FullBackupRestore {
    areas: Vec<RestoreArea>,
    lun_sectors: Vec<u64>,
    boot_lun: u8,
}

impl FullBackupRestore {
    pub fn load(directory: &Path, with_user_data: bool) -> Result<Self> {
        validate_full_backup(directory)?;
        let manifest = BackupManifest::load(directory)?;
        let entries: Vec<BackupEntry> =
            serde_json::from_slice(&fs::read(directory.join(BACKUP_MANIFEST_FILE))?)?;
        let mut copies = Vec::new();
        for lun in 0..LUN_COUNT {
            copies.extend(read_copies(&manifest, lun)?);
        }
        let boot_lun = boot_lun_of(&copies)?;
        let mut areas = partition_extents(&copies)?
            .iter()
            .filter(|extent| with_user_data || extent.name != USER_DATA)
            .map(|extent| partition_area(directory, &entries, extent))
            .collect::<Result<Vec<_>>>()?;
        for kind in [GptCopyKind::Backup, GptCopyKind::Main] {
            areas.extend(table_areas(directory, &entries, &copies, kind)?);
        }
        Ok(Self {
            areas,
            lun_sectors: lun_sectors(&copies),
            boot_lun,
        })
    }

    pub fn areas(&self) -> &[RestoreArea] {
        &self.areas
    }

    pub fn bytes(&self) -> u64 {
        self.areas.iter().map(|area| area.partition.bytes).sum()
    }

    pub fn boot_lun(&self) -> u8 {
        self.boot_lun
    }

    pub fn compare(&self, storage: &mut dyn SectorStorage) -> Result<RestoreCheck> {
        self.require_geometry(storage)?;
        Ok(RestoreCheck {
            areas: self.areas.len(),
            different: different_areas(&self.areas, storage)?,
            boot_lun_selected: self.boot_lun_is_selected(storage)?,
        })
    }

    pub fn write(&self, storage: &mut dyn SectorStorage, check: &RestoreCheck) -> Result<()> {
        let first_boot_images = check.first_boot_images();
        if !first_boot_images.is_empty() {
            return Err(Front2Error::Unsupported(format!(
                "the restore does not write the first boot images, and the device has other data than the backup in: {}",
                first_boot_images.join(", ")
            )));
        }
        for area in &check.different {
            area.partition.verify()?;
        }
        for area in &check.different {
            area.write_and_read_back(storage)?;
        }
        self.select_boot_lun(storage)?;
        self.require_boot_tables(storage)
    }

    fn select_boot_lun(&self, storage: &mut dyn SectorStorage) -> Result<()> {
        if self.boot_lun_is_selected(storage)? {
            return Ok(());
        }
        storage.set_boot_lun(self.boot_lun)?;
        if !self.boot_lun_is_selected(storage)? {
            return Err(Front2Error::Verification(format!(
                "the boot WLUN {BOOT_WLUN} does not give the main GPT of LUN {} after the boot LUN command",
                self.boot_lun
            )));
        }
        Ok(())
    }

    fn boot_lun_is_selected(&self, storage: &mut dyn SectorStorage) -> Result<bool> {
        let table = self.area(&table_name(GptCopyKind::Main, self.boot_lun))?;
        let readback = read_sectors_sha256(storage, BOOT_WLUN, table.start_sector, table.sectors)?;
        Ok(readback == table.partition.sha256)
    }

    fn require_boot_tables(&self, storage: &mut dyn SectorStorage) -> Result<()> {
        let names = BOOT_TABLES.iter().flat_map(|(lun, _)| {
            [GptCopyKind::Main, GptCopyKind::Backup].map(|kind| table_name(kind, *lun))
        });
        for name in names {
            self.area(&name)?.require_on_device(storage)?;
        }
        Ok(())
    }

    fn require_geometry(&self, storage: &mut dyn SectorStorage) -> Result<()> {
        for (lun, sectors) in (0..LUN_COUNT).zip(&self.lun_sectors) {
            require_lun(storage, lun, *sectors)?;
        }
        Ok(())
    }

    fn area(&self, name: &str) -> Result<&RestoreArea> {
        self.areas
            .iter()
            .find(|area| area.partition.name == name)
            .ok_or_else(|| Front2Error::Image(format!("the backup holds no area {name}")))
    }
}

fn table_name(kind: GptCopyKind, lun: u8) -> String {
    format!("gpt_{}{lun}", kind.label())
}

fn require_lun(storage: &mut dyn SectorStorage, lun: u8, sectors: u64) -> Result<()> {
    let info = storage.storage_info(lun)?;
    if info.lun != lun || info.sector_bytes != SECTOR_BYTES as u64 || info.total_sectors != sectors
    {
        return Err(Front2Error::Unsupported(format!(
            "LUN {lun} of the device holds {} sectors of {} bytes, the backup is of a LUN with {sectors} sectors of {SECTOR_BYTES} bytes",
            info.total_sectors, info.sector_bytes
        )));
    }
    Ok(())
}

fn different_areas(
    areas: &[RestoreArea],
    storage: &mut dyn SectorStorage,
) -> Result<Vec<RestoreArea>> {
    let mut different = Vec::new();
    for area in areas {
        if area.device_sha256(storage)? != area.partition.sha256 {
            different.push(area.clone());
        }
    }
    Ok(different)
}

fn partition_area(
    directory: &Path,
    entries: &[BackupEntry],
    extent: &BackupExtent,
) -> Result<RestoreArea> {
    let location = extent.location;
    let partition = recorded(directory, entries, &extent.name, location.lun)?;
    RestoreArea::new(partition, location.start_sector, location.sectors)
}

fn table_areas(
    directory: &Path,
    entries: &[BackupEntry],
    copies: &[BackupGpt],
    kind: GptCopyKind,
) -> Result<Vec<RestoreArea>> {
    copies
        .iter()
        .filter(|table| table.copy.kind() == kind)
        .map(|table| table_area(directory, entries, table))
        .collect()
}

fn table_area(directory: &Path, entries: &[BackupEntry], table: &BackupGpt) -> Result<RestoreArea> {
    let partition = recorded(directory, entries, &table.name(), table.lun)?;
    let sectors = (table.copy.bytes().len() / SECTOR_BYTES) as u64;
    RestoreArea::new(partition, table.copy.start_sector(), sectors)
}

fn recorded(
    directory: &Path,
    entries: &[BackupEntry],
    name: &str,
    lun: u8,
) -> Result<BackupPartition> {
    let entry = entries
        .iter()
        .find(|entry| entry.partition_lun(name) == Some(lun))
        .ok_or_else(|| Front2Error::Image(format!("the backup records no dump of {name}")))?;
    Ok(BackupPartition {
        name: name.to_string(),
        lun,
        path: directory.join(BackupEntry::relative_path(name, lun)),
        bytes: entry.bytes,
        sha256: entry.sha256.to_ascii_lowercase(),
    })
}

fn lun_sectors(copies: &[BackupGpt]) -> Vec<u64> {
    copies
        .iter()
        .filter(|table| table.copy.kind() == GptCopyKind::Main)
        .map(|table| table.copy.alternate_lba().saturating_add(1))
        .collect()
}

fn boot_lun_of(copies: &[BackupGpt]) -> Result<u8> {
    let active: Vec<u8> = BOOT_TABLES
        .iter()
        .filter(|(lun, name)| is_active(copies, *lun, name))
        .map(|(lun, _)| *lun)
        .collect();
    match active.as_slice() {
        [lun] => Ok(*lun),
        _ => Err(Front2Error::Unsupported(format!(
            "the backup does not show one boot LUN: the active flag is on {} of xbl_a on LUN {ORIGINAL_BOOT_LUN} and xbl_b on LUN {NEW_BOOT_LUN}",
            active.len()
        ))),
    }
}

fn is_active(copies: &[BackupGpt], lun: u8, name: &str) -> bool {
    copies
        .iter()
        .filter(|table| table.lun == lun && table.copy.kind() == GptCopyKind::Main)
        .filter_map(|table| table.copy.entry(name))
        .any(|entry| entry.flags & ACTIVE_FLAG != 0)
}
