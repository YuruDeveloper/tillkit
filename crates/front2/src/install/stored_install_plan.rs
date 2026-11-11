use crate::install::install_plan::InstallPlan;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredInstallPlan {
    plan: InstallPlan,
    directory: PathBuf,
    plan_sha256: String,
}

impl StoredInstallPlan {
    pub(crate) fn new(plan: InstallPlan, directory: &Path, plan_sha256: String) -> Self {
        Self {
            plan,
            directory: directory.to_path_buf(),
            plan_sha256,
        }
    }

    pub fn plan(&self) -> &InstallPlan {
        &self.plan
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn plan_sha256(&self) -> &str {
        &self.plan_sha256
    }
}
