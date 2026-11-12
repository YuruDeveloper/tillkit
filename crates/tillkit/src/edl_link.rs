use crate::full_restore_review::FullRestoreReview;
use front2::full_backup::{FullBackupRestore, RestoreCheck};
use front2::install::{InstallProgress, SectorStorage, StoredInstallPlan};

pub struct EdlLink<Storage: SectorStorage> {
    pub storage: Storage,
    pub install: Option<(StoredInstallPlan, InstallProgress)>,
    pub full_restore: Option<(FullBackupRestore, RestoreCheck, FullRestoreReview)>,
}

impl<Storage: SectorStorage> EdlLink<Storage> {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage,
            install: None,
            full_restore: None,
        }
    }
}
