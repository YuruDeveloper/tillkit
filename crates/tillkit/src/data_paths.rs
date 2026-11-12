use std::path::{Path, PathBuf};

pub const DEFAULT_FOLDER: &str = "tillkit-data";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    root: PathBuf,
}

impl DataPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn beside_program(program_folder: &Path) -> Self {
        Self::new(program_folder.join(DEFAULT_FOLDER))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn pairs(&self) -> PathBuf {
        self.root.join("pairs")
    }

    pub fn plans(&self) -> PathBuf {
        self.root.join("plans")
    }

    pub fn kernel_sets(&self) -> PathBuf {
        self.root.join("kernel-sets")
    }

    pub fn install_plans(&self) -> PathBuf {
        self.root.join("install-plans")
    }

    pub fn gsi_candidates(&self) -> PathBuf {
        self.root.join("gsi")
    }

    pub fn magisk_cache(&self) -> PathBuf {
        self.root.join("magisk")
    }

    pub fn magisk_work(&self) -> PathBuf {
        self.root.join("magisk-work")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_beside_the_program_holds_each_store() {
        let directory = tempfile::tempdir().expect("tempdir");
        let paths = DataPaths::beside_program(directory.path());
        let root = directory.path().join(DEFAULT_FOLDER);
        assert_eq!(paths.root(), root);
        assert_eq!(paths.pairs(), root.join("pairs"));
        assert_ne!(paths.pairs(), paths.kernel_sets());
    }
}
