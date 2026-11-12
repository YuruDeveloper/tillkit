use crate::error::{Front2Error, Result};
use crate::full_backup::backup_extent::BackupExtent;
use crate::full_backup::backup_gpt::BackupGpt;
use crate::full_backup::backup_io::{publish_manifest, save_partition, write_new};
use crate::full_backup::full_backup_result::FullBackupResult;
use crate::gpt::{GptCopyKind, PartitionMap, SECTOR_BYTES};
use crate::install::{SectorStorage, StorageInfo};
use crate::profiles::toss_front2;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub(super) const LUN_COUNT: u8 = 6;

#[derive(Debug, Clone)]
pub struct FullBackupPlan {
    geometry: Vec<StorageInfo>,
    copies: Vec<BackupGpt>,
    partitions: Vec<BackupExtent>,
    backup_bytes: u64,
    read_bytes: u64,
}

impl FullBackupPlan {
    pub fn read(storage: &mut dyn SectorStorage) -> Result<Self> {
        let mut geometry = Vec::new();
        let mut copies = Vec::new();
        for lun in 0..LUN_COUNT {
            let info = storage.storage_info(lun)?;
            require_geometry(info, lun)?;
            geometry.push(info);
            copies.extend(read_copies(storage, info)?);
        }
        let partitions = partition_extents(&copies)?;
        let gpt_bytes = total_bytes(copies.iter().map(|entry| entry.copy.bytes().len() as u64))?;
        let backup_bytes = total_bytes(
            partitions
                .iter()
                .map(|extent| extent.bytes)
                .chain([gpt_bytes]),
        )?;
        let read_bytes = total_bytes(
            partitions
                .iter()
                .filter(|extent| !extent.single_read())
                .map(|extent| extent.bytes)
                .chain([backup_bytes]),
        )?;
        Ok(Self {
            geometry,
            copies,
            partitions,
            backup_bytes,
            read_bytes,
        })
    }

    pub fn backup_bytes(&self) -> u64 {
        self.backup_bytes
    }

    pub fn read_bytes(&self) -> u64 {
        self.read_bytes
    }

    pub fn write(
        &self,
        storage: &mut dyn SectorStorage,
        directory: &Path,
    ) -> Result<FullBackupResult> {
        fs::create_dir(directory)?;
        let current = Self::read(storage)?;
        self.require_same_device_layout(&current)?;
        let mut entries = Vec::new();
        fs::create_dir(directory.join("partitions"))?;
        for lun in 0..LUN_COUNT {
            fs::create_dir(directory.join(format!("partitions/lun{lun}")))?;
        }
        for copy in &current.copies {
            let entry = copy.entry();
            write_new(directory, &entry, copy.copy.bytes())?;
            entries.push(entry);
        }
        for extent in &self.partitions {
            entries.push(save_partition(storage, directory, extent)?);
        }
        publish_manifest(directory, &entries)?;
        Ok(FullBackupResult {
            directory: directory.to_path_buf(),
            backup_bytes: self.backup_bytes,
            single_read_partitions: self
                .partitions
                .iter()
                .filter(|extent| extent.single_read())
                .map(|extent| extent.name.clone())
                .collect(),
        })
    }

    fn require_same_device_layout(&self, current: &Self) -> Result<()> {
        let same_copies = self.copies.len() == current.copies.len()
            && self
                .copies
                .iter()
                .zip(&current.copies)
                .all(|(before, after)| {
                    before.lun == after.lun && before.copy.bytes() == after.copy.bytes()
                });
        if self.geometry != current.geometry || !same_copies {
            return Err(Front2Error::Verification(
                "backup geometry or GPT changed after preparation".into(),
            ));
        }
        Ok(())
    }
}

fn require_geometry(info: StorageInfo, lun: u8) -> Result<()> {
    if info.lun != lun || info.sector_bytes != SECTOR_BYTES as u64 || info.total_sectors < 12 {
        return Err(Front2Error::Unsupported(format!(
            "backup storage geometry is not supported on LUN {lun}"
        )));
    }
    Ok(())
}

fn read_copies(storage: &mut dyn SectorStorage, info: StorageInfo) -> Result<Vec<BackupGpt>> {
    let main = BackupGpt::read(storage, info, GptCopyKind::Main)?;
    let backup = BackupGpt::read(storage, info, GptCopyKind::Backup)?;
    main.copy
        .require_pair_with(&backup.copy, &format!("backup of LUN {}", info.lun))?;
    Ok(vec![main, backup])
}

pub(super) fn partition_extents(copies: &[BackupGpt]) -> Result<Vec<BackupExtent>> {
    let main: Vec<_> = copies
        .iter()
        .filter(|entry| entry.copy.kind() == GptCopyKind::Main)
        .map(|entry| (entry.lun, entry.copy.bytes()))
        .collect();
    let map = PartitionMap::from_main_copies(&main)?;
    require_install_partitions(&map)?;
    let partitions = map
        .partitions()
        .iter()
        .map(|(name, location)| BackupExtent::new(name, *location))
        .collect::<Result<Vec<_>>>()?;
    require_unique_paths(&partitions)?;
    Ok(partitions)
}

fn require_unique_paths(partitions: &[BackupExtent]) -> Result<()> {
    let mut paths = HashSet::new();
    for extent in partitions {
        if !paths.insert((extent.location.lun, extent.name.to_ascii_lowercase())) {
            return Err(Front2Error::Image(format!(
                "backup names differ only in letter case on LUN {}: {}",
                extent.location.lun, extent.name
            )));
        }
    }
    Ok(())
}

fn require_install_partitions(map: &PartitionMap) -> Result<()> {
    for base in toss_front2::PHYSICAL_BOOT_PARTITIONS {
        map.locate(&format!("{base}_a"))?;
        map.locate(&format!("{base}_b"))?;
    }
    for name in [
        "vbmeta_a",
        "vbmeta_b",
        "vbmeta_system_a",
        "vbmeta_system_b",
        "metadata",
        "misc",
        "super",
        "userdata",
    ] {
        map.locate(name)?;
    }
    Ok(())
}

fn total_bytes(mut values: impl Iterator<Item = u64>) -> Result<u64> {
    values.try_fold(0u64, |total, bytes| {
        total
            .checked_add(bytes)
            .ok_or_else(|| Front2Error::Image("total backup size is out of range".into()))
    })
}
