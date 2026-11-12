use super::ReadOnlyStorage;
use crate::gpt_support::{LAST_LBA, SyntheticGpt};
use crate::install_support::InstallFixture;
use front2::install::InstallInputs;
use std::path::PathBuf;
use tempfile::TempDir;

pub struct BackupFixture {
    pub directory: TempDir,
    pub inputs: InstallInputs,
    pub storage: ReadOnlyStorage,
}

impl BackupFixture {
    pub fn new() -> Self {
        Self::with_super_sectors(16)
    }

    pub fn with_super_sectors(sectors: u64) -> Self {
        let fixture = InstallFixture::with_super_sectors(sectors);
        let mut storage = ReadOnlyStorage::new(fixture.storage);
        let table = Self::extra_table();
        storage.inner.load(5, 0, &table.main());
        storage.inner.load(5, LAST_LBA - 4, &table.backup());
        Self {
            directory: fixture.directory,
            inputs: fixture.inputs,
            storage,
        }
    }

    pub fn output(&self) -> PathBuf {
        self.directory.path().join("full-backup")
    }

    pub fn extra_table() -> SyntheticGpt {
        SyntheticGpt::empty()
            .with_entry("fsg", 0)
            .with_entry("dtbo_a", 0)
            .with_entry("init_boot_a", 0)
            .with_entry("recovery_a", 0)
            .with_entry("vendor_boot_a", 0)
    }
}
