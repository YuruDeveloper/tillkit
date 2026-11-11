use crate::plan_source::PlanSource;
use front2::WritePlan;
use std::path::PathBuf;

pub const APPROVAL_CODE_LENGTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanReview {
    pub plan: WritePlan,
    pub sha256: String,
    pub directory: PathBuf,
    pub source: PlanSource,
}

impl PlanReview {
    pub fn approval_code(&self) -> &str {
        self.sha256.get(..APPROVAL_CODE_LENGTH).unwrap_or("")
    }
}
