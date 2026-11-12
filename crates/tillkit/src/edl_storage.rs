use front2::install::{SectorStorage, StorageInfo};
use front2::{Front2Error, Result};
use ltbox_device::edl::{EdlSession, UfsStorageInfo};
use std::fmt::Display;
use std::path::Path;

pub struct EdlStorage {
    session: EdlSession,
}

impl EdlStorage {
    pub fn open(loader: &Path) -> Result<EdlStorage> {
        let mut log = Vec::new();
        let session = EdlSession::open(loader, &mut log).map_err(device_failure)?;
        Ok(EdlStorage { session })
    }
}

impl SectorStorage for EdlStorage {
    fn storage_info(&mut self, lun: u8) -> Result<StorageInfo> {
        let info = self.session.storage_info(lun).map_err(device_failure)?;
        Ok(storage_info_of(lun, info))
    }

    fn read_sectors(&mut self, lun: u8, start_sector: u64, sectors: u64) -> Result<Vec<u8>> {
        let count = sector_count(sectors)?;
        self.session
            .read_sectors(lun, start_sector, count)
            .map_err(device_failure)
    }

    fn write_sectors(&mut self, lun: u8, start_sector: u64, data: &[u8]) -> Result<()> {
        self.session
            .write_sectors(lun, start_sector, data)
            .map_err(device_failure)
    }

    fn set_boot_lun(&mut self, lun: u8) -> Result<()> {
        self.session
            .set_bootable_storage_drive(lun)
            .map_err(device_failure)
    }

    fn reset(&mut self) -> Result<()> {
        let mut log = Vec::new();
        self.session.reset(&mut log).map_err(device_failure)
    }
}

fn storage_info_of(lun: u8, info: UfsStorageInfo) -> StorageInfo {
    StorageInfo {
        lun,
        total_sectors: info.total_blocks,
        sector_bytes: info.block_bytes,
    }
}

fn sector_count(sectors: u64) -> Result<usize> {
    usize::try_from(sectors).map_err(|_| {
        Front2Error::Plan(format!(
            "a read of {sectors} sectors does not fit in the memory of this computer"
        ))
    })
}

fn device_failure(error: impl Display) -> Front2Error {
    Front2Error::Shell(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ltbox_device::edl::EdlError;

    fn held_across_threads<T: Send>() {}

    #[test]
    fn the_storage_can_move_to_the_install_worker_thread() {
        held_across_threads::<EdlStorage>();
    }

    #[test]
    fn storage_info_keeps_the_lun_and_the_reported_blocks() {
        let info = UfsStorageInfo {
            total_blocks: 0x800,
            block_bytes: 4096,
        };

        assert_eq!(
            storage_info_of(1, info),
            StorageInfo {
                lun: 1,
                total_sectors: 0x800,
                sector_bytes: 4096,
            }
        );
    }

    #[test]
    fn a_chunk_of_the_install_engine_fits_in_one_read() {
        assert_eq!(sector_count(1024).expect("fits"), 1024);
    }

    #[test]
    fn a_device_error_keeps_its_message() {
        let error = device_failure(EdlError::StorageInfo(
            "UFS Logical Block Size: is 0x9".into(),
        ));

        assert!(matches!(&error, Front2Error::Shell(_)));
        assert!(error.to_string().contains("UFS Logical Block Size: is 0x9"));
    }

    #[test]
    fn a_device_error_is_not_a_verification_failure() {
        let error = device_failure(EdlError::Session("Read request was NAKed".into()));

        assert!(!matches!(error, Front2Error::Verification(_)));
    }
}
