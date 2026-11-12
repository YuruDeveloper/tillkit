use crate::digest::sha256_bytes;
use crate::error::{Front2Error, Result};
use crate::full_backup::backup_io::read_exact_sectors;
use crate::gpt::{GptCopy, GptCopyKind, SECTOR_BYTES};
use crate::gsi::BackupEntry;
use crate::install::{SectorStorage, StorageInfo};

#[derive(Debug, Clone)]
pub(super) struct BackupGpt {
    pub lun: u8,
    pub copy: GptCopy,
}

impl BackupGpt {
    pub fn read(
        storage: &mut dyn SectorStorage,
        info: StorageInfo,
        kind: GptCopyKind,
    ) -> Result<Self> {
        let sectors = (kind.dump_bytes() / SECTOR_BYTES) as u64;
        let start = start_sector(kind, info.total_sectors, sectors)?;
        let bytes = read_exact_sectors(storage, info.lun, start, sectors)?;
        let copy = GptCopy::parse(kind, &bytes)?;
        let end = match kind {
            GptCopyKind::Main => copy.alternate_lba().checked_add(1),
            GptCopyKind::Backup => copy.current_lba().checked_add(1),
        };
        if copy.start_sector() != start || end != Some(info.total_sectors) {
            return Err(Front2Error::Verification(format!(
                "GPT and storage geometry differ on LUN {}",
                info.lun
            )));
        }
        Ok(Self {
            lun: info.lun,
            copy,
        })
    }

    pub fn name(&self) -> String {
        format!("gpt_{}{}", self.copy.kind().label(), self.lun)
    }

    pub fn entry(&self) -> BackupEntry {
        BackupEntry {
            path: BackupEntry::relative_path(&self.name(), self.lun),
            bytes: self.copy.bytes().len() as u64,
            sha256: sha256_bytes(self.copy.bytes()),
        }
    }
}

fn start_sector(kind: GptCopyKind, total_sectors: u64, sectors: u64) -> Result<u64> {
    match kind {
        GptCopyKind::Main => Ok(0),
        GptCopyKind::Backup => total_sectors
            .checked_sub(sectors)
            .ok_or_else(|| Front2Error::Image("storage is shorter than the backup GPT".into())),
    }
}
