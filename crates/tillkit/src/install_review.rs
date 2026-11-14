use front2::install::InstallPlan;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReview {
    pub plan: InstallPlan,
    pub sha256: String,
    pub directory: PathBuf,
    pub untested_build: Option<String>,
}
