use crate::install_support::MockStorage;
use front2::install::{SectorStorage, StorageInfo};
use front2::{Front2Error, Result};
use std::collections::HashMap;

pub struct ReadOnlyStorage {
    pub inner: MockStorage,
    pub operations: usize,
    pub failure_at: Option<usize>,
    pub short_read_at: Option<usize>,
    pub sector_bytes: u64,
    pub total_sectors: u64,
    pub reported_lun: Option<u8>,
    pub read_bytes: u64,
    pub corrupt_second_read: Option<(u8, u64)>,
    pub reads: HashMap<(u8, u64), usize>,
}

impl ReadOnlyStorage {
    pub fn new(inner: MockStorage) -> Self {
        Self {
            inner,
            operations: 0,
            failure_at: None,
            short_read_at: None,
            sector_bytes: 4096,
            total_sectors: 4096,
            reported_lun: None,
            read_bytes: 0,
            corrupt_second_read: None,
            reads: HashMap::new(),
        }
    }

    fn advance(&mut self) -> Result<()> {
        self.operations += 1;
        if self.failure_at == Some(self.operations) {
            return Err(Front2Error::Shell("mock backup read failure".into()));
        }
        Ok(())
    }
}

impl SectorStorage for ReadOnlyStorage {
    fn storage_info(&mut self, lun: u8) -> Result<StorageInfo> {
        self.advance()?;
        Ok(StorageInfo {
            lun: self.reported_lun.unwrap_or(lun),
            total_sectors: self.total_sectors,
            sector_bytes: self.sector_bytes,
        })
    }

    fn read_sectors(&mut self, lun: u8, start_sector: u64, sectors: u64) -> Result<Vec<u8>> {
        self.advance()?;
        let mut data = self.inner.read_sectors(lun, start_sector, sectors)?;
        self.read_bytes += data.len() as u64;
        let count = self.reads.entry((lun, start_sector)).or_default();
        *count += 1;
        if self.corrupt_second_read == Some((lun, start_sector)) && *count == 2 {
            data[0] ^= 1;
        }
        if self.short_read_at == Some(self.operations) {
            data.pop();
        }
        Ok(data)
    }

    fn write_sectors(&mut self, _lun: u8, _start_sector: u64, _data: &[u8]) -> Result<()> {
        panic!("full backup must not write device sectors")
    }

    fn set_boot_lun(&mut self, _lun: u8) -> Result<()> {
        panic!("full backup must not change the boot LUN")
    }

    fn reset(&mut self) -> Result<()> {
        panic!("full backup must not reset the device")
    }
}
