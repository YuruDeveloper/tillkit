use crate::error::{Front2Error, Result};
use crate::gpt::SECTOR_BYTES;
use crate::gsi::BackupPartition;
use crate::install::sector_transfer::read_sectors_sha256;
use crate::install::{CHUNK_SECTORS, NEW_BOOT_LUN, ORIGINAL_BOOT_LUN, SectorStorage};
use std::fs::File;
use std::io::Read;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreArea {
    pub partition: BackupPartition,
    pub start_sector: u64,
    pub sectors: u64,
}

impl RestoreArea {
    pub fn new(partition: BackupPartition, start_sector: u64, sectors: u64) -> Result<Self> {
        if sectors.checked_mul(SECTOR_BYTES as u64) != Some(partition.bytes) {
            return Err(Front2Error::Image(format!(
                "backup of {} holds {} bytes, the GPT gives it {sectors} sectors",
                partition.name, partition.bytes
            )));
        }
        Ok(Self {
            partition,
            start_sector,
            sectors,
        })
    }

    pub fn is_table(&self) -> bool {
        self.partition.name.starts_with("gpt_main") || self.partition.name.starts_with("gpt_backup")
    }

    pub fn is_first_boot_image(&self) -> bool {
        [ORIGINAL_BOOT_LUN, NEW_BOOT_LUN].contains(&self.partition.lun) && !self.is_table()
    }

    pub fn device_sha256(&self, storage: &mut dyn SectorStorage) -> Result<String> {
        read_sectors_sha256(storage, self.partition.lun, self.start_sector, self.sectors)
    }

    pub fn write_and_read_back(&self, storage: &mut dyn SectorStorage) -> Result<()> {
        let mut source = File::open(&self.partition.path)?;
        let mut done = 0u64;
        while done < self.sectors {
            let count = (self.sectors - done).min(CHUNK_SECTORS);
            let mut chunk = vec![0u8; count as usize * SECTOR_BYTES];
            source.read_exact(&mut chunk)?;
            storage.write_sectors(self.partition.lun, self.start_sector + done, &chunk)?;
            done += count;
        }
        self.require_on_device(storage)
    }

    pub fn require_on_device(&self, storage: &mut dyn SectorStorage) -> Result<()> {
        let current = self.device_sha256(storage)?;
        if current != self.partition.sha256 {
            return Err(Front2Error::Verification(format!(
                "{}: the {} sectors at sector {} of LUN {} have SHA-256 {current}, the backup records {}",
                self.partition.name,
                self.sectors,
                self.start_sector,
                self.partition.lun,
                self.partition.sha256
            )));
        }
        Ok(())
    }
}
