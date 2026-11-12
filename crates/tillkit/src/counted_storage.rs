use front2::Result;
use front2::install::{SectorStorage, StorageInfo};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct CountedStorage<Storage: SectorStorage> {
    storage: Storage,
    transferred: Arc<AtomicU64>,
}

impl<Storage: SectorStorage> CountedStorage<Storage> {
    pub fn new(storage: Storage, transferred: Arc<AtomicU64>) -> Self {
        Self {
            storage,
            transferred,
        }
    }

    fn count(&self, bytes: usize) {
        self.transferred.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

impl<Storage: SectorStorage> SectorStorage for CountedStorage<Storage> {
    fn storage_info(&mut self, lun: u8) -> Result<StorageInfo> {
        self.storage.storage_info(lun)
    }

    fn read_sectors(&mut self, lun: u8, start_sector: u64, sectors: u64) -> Result<Vec<u8>> {
        let data = self.storage.read_sectors(lun, start_sector, sectors)?;
        self.count(data.len());
        Ok(data)
    }

    fn write_sectors(&mut self, lun: u8, start_sector: u64, data: &[u8]) -> Result<()> {
        self.storage.write_sectors(lun, start_sector, data)?;
        self.count(data.len());
        Ok(())
    }

    fn set_boot_lun(&mut self, lun: u8) -> Result<()> {
        self.storage.set_boot_lun(lun)
    }

    fn reset(&mut self) -> Result<()> {
        self.storage.reset()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_support::InstallFixture;

    #[test]
    fn the_count_holds_the_bytes_that_the_storage_gave_and_took() {
        let fixture = InstallFixture::standard();
        let misc = fixture.location("misc");
        let transferred = Arc::new(AtomicU64::new(0));
        let mut storage = CountedStorage::new(fixture.storage, transferred.clone());

        let data = storage
            .read_sectors(misc.lun, misc.start_sector, 1)
            .expect("read");
        assert_eq!(transferred.load(Ordering::Relaxed), data.len() as u64);
        storage
            .write_sectors(misc.lun, misc.start_sector, &data)
            .expect("write");
        assert_eq!(transferred.load(Ordering::Relaxed), 2 * data.len() as u64);
        assert!(
            storage
                .read_sectors(misc.lun, misc.start_sector, 0)
                .is_err()
        );
        assert_eq!(transferred.load(Ordering::Relaxed), 2 * data.len() as u64);
    }
}
