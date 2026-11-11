use crate::plan_source::PlanSource;
use front2::WritePlan;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanReview {
    pub plan: WritePlan,
    pub sha256: String,
    pub directory: PathBuf,
    pub source: PlanSource,
}
