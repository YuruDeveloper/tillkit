#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockCall {
    StorageInfo {
        lun: u8,
    },
    Read {
        lun: u8,
        start_sector: u64,
        sectors: u64,
    },
    Write {
        lun: u8,
        start_sector: u64,
        sectors: u64,
    },
    SetBootLun {
        lun: u8,
    },
    Reset,
}

impl MockCall {
    pub fn is_write(&self) -> bool {
        matches!(self, Self::Write { .. })
    }

    pub fn is_boot_lun_command(&self) -> bool {
        matches!(self, Self::SetBootLun { .. })
    }

    pub fn is_reset(&self) -> bool {
        matches!(self, Self::Reset)
    }

    pub fn changes_the_device(&self) -> bool {
        self.is_write() || self.is_boot_lun_command() || self.is_reset()
    }

    pub fn sectors(&self) -> u64 {
        sectors_of(self)
    }

    pub fn writes_into(&self, lun: u8, start_sector: u64, sectors: u64) -> bool {
        write_range_of(self).is_some_and(|(written_lun, written_start, written_sectors)| {
            written_lun == lun
                && written_start < start_sector + sectors
                && start_sector < written_start + written_sectors
        })
    }
}

fn sectors_of(call: &MockCall) -> u64 {
    match call {
        MockCall::Read { sectors, .. } | MockCall::Write { sectors, .. } => *sectors,
        _ => 0,
    }
}

fn write_range_of(call: &MockCall) -> Option<(u8, u64, u64)> {
    match call {
        MockCall::Write {
            lun,
            start_sector,
            sectors,
        } => Some((*lun, *start_sector, *sectors)),
        _ => None,
    }
}
