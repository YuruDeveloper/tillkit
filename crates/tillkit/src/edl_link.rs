use front2::install::{InstallProgress, SectorStorage, StoredInstallPlan};

pub struct EdlLink<Storage: SectorStorage> {
    pub storage: Storage,
    pub install: Option<(StoredInstallPlan, InstallProgress)>,
}

impl<Storage: SectorStorage> EdlLink<Storage> {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage,
            install: None,
        }
    }
}
