use crate::error::Result;
use crate::install::storage_info::StorageInfo;

pub trait SectorStorage {
    fn storage_info(&mut self, lun: u8) -> Result<StorageInfo>;

    fn read_sectors(&mut self, lun: u8, start_sector: u64, sectors: u64) -> Result<Vec<u8>>;

    fn write_sectors(&mut self, lun: u8, start_sector: u64, data: &[u8]) -> Result<()>;

    fn set_boot_lun(&mut self, lun: u8) -> Result<()>;

    fn reset(&mut self) -> Result<()>;
}
