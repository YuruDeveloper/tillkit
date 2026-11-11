use super::mock_call::MockCall;
use front2::install::{BOOT_WLUN, ORIGINAL_BOOT_LUN, SectorStorage, StorageInfo};
use front2::{Front2Error, Result};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const SECTOR: usize = 4096;

#[derive(Debug)]
pub struct MockStorage {
    lun_count: u8,
    lun_sectors: u64,
    sectors: HashMap<(u8, u64), Vec<u8>>,
    written: HashSet<(u8, u64)>,
    boot_lun: u8,
    ignored_write: Option<(u8, u64)>,
    corrupted_write: Option<(u8, u64)>,
    failing_read_after_write: Option<(u8, u64)>,
    short_read_after_write: Option<(u8, u64)>,
    ignores_boot_lun_command: bool,
    fails_reset: bool,
    calls: Rc<RefCell<Vec<MockCall>>>,
}

impl MockStorage {
    pub fn new(lun_count: u8, lun_sectors: u64) -> Self {
        Self {
            lun_count,
            lun_sectors,
            sectors: HashMap::new(),
            written: HashSet::new(),
            boot_lun: ORIGINAL_BOOT_LUN,
            ignored_write: None,
            corrupted_write: None,
            failing_read_after_write: None,
            short_read_after_write: None,
            ignores_boot_lun_command: false,
            fails_reset: false,
            calls: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn calls(&self) -> Rc<RefCell<Vec<MockCall>>> {
        Rc::clone(&self.calls)
    }

    pub fn load(&mut self, lun: u8, start_sector: u64, data: &[u8]) {
        assert_eq!(data.len() % SECTOR, 0);
        for (i, sector) in data.chunks_exact(SECTOR).enumerate() {
            self.sectors
                .insert((lun, start_sector + i as u64), sector.to_vec());
        }
    }

    pub fn content(&self, lun: u8, start_sector: u64, sectors: u64) -> Vec<u8> {
        let mut data = Vec::new();
        for sector in start_sector..start_sector + sectors {
            data.extend_from_slice(&self.sector(lun, sector));
        }
        data
    }

    pub fn start_with_boot_lun(&mut self, lun: u8) {
        self.boot_lun = lun;
    }

    pub fn ignore_writes_that_cover(&mut self, lun: u8, sector: u64) {
        self.ignored_write = Some((lun, sector));
    }

    pub fn corrupt_writes_that_cover(&mut self, lun: u8, sector: u64) {
        self.corrupted_write = Some((lun, sector));
    }

    pub fn fail_reads_after_a_write_to(&mut self, lun: u8, sector: u64) {
        self.failing_read_after_write = Some((lun, sector));
    }

    pub fn shorten_reads_after_a_write_to(&mut self, lun: u8, sector: u64) {
        self.short_read_after_write = Some((lun, sector));
    }

    pub fn ignore_the_boot_lun_command(&mut self) {
        self.ignores_boot_lun_command = true;
    }

    pub fn fail_the_reset_command(&mut self) {
        self.fails_reset = true;
    }

    fn log(&self, call: MockCall) {
        self.calls.borrow_mut().push(call);
    }

    fn sector(&self, lun: u8, sector: u64) -> Vec<u8> {
        self.sectors
            .get(&(lun, sector))
            .cloned()
            .unwrap_or_else(|| vec![0u8; SECTOR])
    }

    fn physical_lun(&self, lun: u8) -> u8 {
        if lun == BOOT_WLUN { self.boot_lun } else { lun }
    }

    fn require_range(&self, lun: u8, start_sector: u64, sectors: u64) -> Result<()> {
        if lun >= self.lun_count || start_sector + sectors > self.lun_sectors || sectors == 0 {
            return Err(Front2Error::Shell(format!(
                "mock: {sectors} sectors at sector {start_sector} of LUN {lun} are out of range"
            )));
        }
        Ok(())
    }

    fn read_fails(&self, lun: u8, start_sector: u64, sectors: u64) -> bool {
        self.covers_written(self.failing_read_after_write, lun, start_sector, sectors)
    }

    fn read_is_short(&self, lun: u8, start_sector: u64, sectors: u64) -> bool {
        self.covers_written(self.short_read_after_write, lun, start_sector, sectors)
    }

    fn covers_written(
        &self,
        target: Option<(u8, u64)>,
        lun: u8,
        start_sector: u64,
        sectors: u64,
    ) -> bool {
        is_covered(target, lun, start_sector, sectors)
            && target.is_some_and(|position| self.written.contains(&position))
    }

    fn store(&mut self, lun: u8, start_sector: u64, data: &[u8]) {
        for (i, sector) in data.chunks_exact(SECTOR).enumerate() {
            let position = (lun, start_sector + i as u64);
            self.sectors.insert(position, sector.to_vec());
            self.written.insert(position);
        }
    }

    fn corrupt(&mut self, lun: u8, sector: u64) {
        let mut data = self.sector(lun, sector);
        data[SECTOR / 2] ^= 0x01;
        self.sectors.insert((lun, sector), data);
    }
}

impl SectorStorage for MockStorage {
    fn storage_info(&mut self, lun: u8) -> Result<StorageInfo> {
        self.log(MockCall::StorageInfo { lun });
        self.require_range(lun, 0, 1)?;
        Ok(StorageInfo {
            lun,
            total_sectors: self.lun_sectors,
            sector_bytes: SECTOR as u64,
        })
    }

    fn read_sectors(&mut self, lun: u8, start_sector: u64, sectors: u64) -> Result<Vec<u8>> {
        self.log(MockCall::Read {
            lun,
            start_sector,
            sectors,
        });
        let physical = self.physical_lun(lun);
        self.require_range(physical, start_sector, sectors)?;
        if self.read_fails(physical, start_sector, sectors) {
            return Err(Front2Error::Shell("mock: read error".into()));
        }
        let mut data = self.content(physical, start_sector, sectors);
        if self.read_is_short(physical, start_sector, sectors) {
            data.truncate(data.len() - SECTOR / 2);
        }
        Ok(data)
    }

    fn write_sectors(&mut self, lun: u8, start_sector: u64, data: &[u8]) -> Result<()> {
        let sectors = (data.len() / SECTOR) as u64;
        self.log(MockCall::Write {
            lun,
            start_sector,
            sectors,
        });
        self.require_range(lun, start_sector, sectors)?;
        if !data.len().is_multiple_of(SECTOR) {
            return Err(Front2Error::Shell("mock: partial sector".into()));
        }
        if is_covered(self.ignored_write, lun, start_sector, sectors) {
            return Ok(());
        }
        self.store(lun, start_sector, data);
        if let Some((_, sector)) = self
            .corrupted_write
            .filter(|_| is_covered(self.corrupted_write, lun, start_sector, sectors))
        {
            self.corrupt(lun, sector);
        }
        Ok(())
    }

    fn set_boot_lun(&mut self, lun: u8) -> Result<()> {
        self.log(MockCall::SetBootLun { lun });
        if !self.ignores_boot_lun_command {
            self.boot_lun = lun;
        }
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.log(MockCall::Reset);
        if self.fails_reset {
            return Err(Front2Error::Shell("mock: reset error".into()));
        }
        Ok(())
    }
}

fn covers(lun: u8, start_sector: u64, sectors: u64, target_lun: u8, target_sector: u64) -> bool {
    lun == target_lun && start_sector <= target_sector && target_sector < start_sector + sectors
}

fn is_covered(target: Option<(u8, u64)>, lun: u8, start_sector: u64, sectors: u64) -> bool {
    target.is_some_and(|(target_lun, target_sector)| {
        covers(lun, start_sector, sectors, target_lun, target_sector)
    })
}
