use std::path::PathBuf;

pub const DATA_FOLDER: &str = "frontbox";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    root: PathBuf,
}

impl DataPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn for_user() -> Self {
        let parent = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
        Self::new(parent.join(DATA_FOLDER))
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
}
